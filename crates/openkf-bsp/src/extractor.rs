//! Modern Rust BSP Extractor converting level BSP trees into indexed triangle meshes with lightmap UVs.

use crate::model::UModel;
use crate::triangulate::triangulate_fan;
use crate::uv::{project_lightmap_uv, project_texture_uv};
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use std::collections::HashMap;

/// Interleaved GPU vertex representation for modern Vulkan / Direct3D 12 pipelines.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct BspVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub lightmap_uv: [f32; 2],
}

impl BspVertex {
    pub fn new(position: Vec3, normal: Vec3, uv: Vec2, lightmap_uv: Vec2) -> Self {
        Self {
            position: position.to_array(),
            normal: normal.to_array(),
            uv: uv.to_array(),
            lightmap_uv: lightmap_uv.to_array(),
        }
    }
}

/// Extracted triangle mesh representing a material batch of BSP surfaces.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BspMesh {
    /// 3D vertex positions in world space.
    pub positions: Vec<Vec3>,
    /// Geometric surface normals per vertex.
    pub normals: Vec<Vec3>,
    /// Primary diffuse texture UV coordinates.
    pub uvs: Vec<Vec2>,
    /// Secondary lightmap UV coordinates.
    pub lightmap_uvs: Vec<Vec2>,
    /// Triangle index buffer (3 indices per triangle).
    pub indices: Vec<u32>,
    /// Material index from the level's package import/export table.
    pub material_index: u32,
    /// Lightmap texture atlas index (-1 if none/unlit).
    pub lightmap_index: i32,
}

impl BspMesh {
    /// Returns total number of triangles in the mesh.
    #[inline]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Converts separate attribute arrays into an interleaved GPU vertex buffer.
    pub fn to_interleaved_vertices(&self) -> Vec<BspVertex> {
        let count = self.positions.len();
        let mut vertices = Vec::with_capacity(count);
        for i in 0..count {
            let pos = self.positions[i];
            let norm = self.normals.get(i).copied().unwrap_or(Vec3::Y);
            let uv = self.uvs.get(i).copied().unwrap_or(Vec2::ZERO);
            let lm_uv = self.lightmap_uvs.get(i).copied().unwrap_or(Vec2::ZERO);
            vertices.push(BspVertex::new(pos, norm, uv, lm_uv));
        }
        vertices
    }
}

/// Extracts all coplanar polygon surfaces from a UModel into indexed triangle meshes,
/// grouped by `material_index` and sorted deterministically.
pub fn extract_bsp_geometry(model: &UModel) -> Vec<BspMesh> {
    let mut meshes: HashMap<u32, BspMesh> = HashMap::new();

    for node in &model.bsp_nodes {
        if node.num_vertices < 3 {
            continue;
        }
        if node.i_surf < 0 || (node.i_surf as usize) >= model.bsp_surfs.len() {
            continue;
        }

        let surf = &model.bsp_surfs[node.i_surf as usize];

        // Safe lookups for base, normal, and texture vectors
        let base = if surf.p_base >= 0 && (surf.p_base as usize) < model.points.len() {
            model.points[surf.p_base as usize].to_vec3()
        } else {
            Vec3::ZERO
        };

        let normal = if surf.v_normal >= 0 && (surf.v_normal as usize) < model.vectors.len() {
            model.vectors[surf.v_normal as usize].to_vec3()
        } else {
            node.plane.normal().to_vec3()
        };

        let tex_u = if surf.v_texture_u >= 0 && (surf.v_texture_u as usize) < model.vectors.len() {
            model.vectors[surf.v_texture_u as usize].to_vec3()
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        };

        let tex_v = if surf.v_texture_v >= 0 && (surf.v_texture_v as usize) < model.vectors.len() {
            model.vectors[surf.v_texture_v as usize].to_vec3()
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };

        let mat_idx = if surf.material >= 0 { surf.material as u32 } else { 0 };

        let mesh = meshes.entry(mat_idx).or_insert_with(|| BspMesh {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            lightmap_uvs: Vec::new(),
            indices: Vec::new(),
            material_index: mat_idx,
            lightmap_index: surf.i_light_map,
        });

        let base_idx = mesh.positions.len() as u32;
        let mut valid_poly = true;
        let mut node_positions = Vec::with_capacity(node.num_vertices as usize);
        let mut node_uvs = Vec::with_capacity(node.num_vertices as usize);
        let mut node_lm_uvs = Vec::with_capacity(node.num_vertices as usize);

        for i in 0..(node.num_vertices as usize) {
            let vert_idx = (node.i_vert_pool as usize).checked_add(i);
            let vert = match vert_idx.and_then(|idx| model.verts.get(idx)) {
                Some(v) => v,
                None => {
                    valid_poly = false;
                    break;
                }
            };

            let pos = match model.points.get(vert.p_vertex as usize) {
                Some(p) => p.to_vec3(),
                None => {
                    valid_poly = false;
                    break;
                }
            };

            let uv = project_texture_uv(pos, base, tex_u, tex_v);
            let lm_uv = project_lightmap_uv(pos, base, tex_u, tex_v, surf.light_map_scale);

            node_positions.push(pos);
            node_uvs.push(uv);
            node_lm_uvs.push(lm_uv);
        }

        if !valid_poly {
            continue;
        }

        let num_v = node_positions.len();
        for i in 0..num_v {
            mesh.positions.push(node_positions[i]);
            mesh.normals.push(normal);
            mesh.uvs.push(node_uvs[i]);
            mesh.lightmap_uvs.push(node_lm_uvs[i]);
        }

        // Fan triangulation
        triangulate_fan(num_v, base_idx, &mut mesh.indices);
    }

    let mut result: Vec<BspMesh> = meshes.into_values().collect();
    result.sort_by_key(|m| m.material_index);
    result
}

/// Extracts all geometry into a single unified mesh with all triangles combined.
pub fn extract_unified_mesh(model: &UModel) -> BspMesh {
    let mut unified = BspMesh::default();
    let meshes = extract_bsp_geometry(model);

    for mesh in meshes {
        let base_idx = unified.positions.len() as u32;
        unified.positions.extend(mesh.positions);
        unified.normals.extend(mesh.normals);
        unified.uvs.extend(mesh.uvs);
        unified.lightmap_uvs.extend(mesh.lightmap_uvs);

        for idx in mesh.indices {
            unified.indices.push(base_idx + idx);
        }
    }

    unified
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FBspNode, FBspSurf, FPlane, FVector, FVert};

    #[test]
    fn test_extract_single_quad() {
        let mut model = UModel::new();
        // 4 points forming a 256x256 horizontal quad (floor)
        model.points = vec![
            FVector::new(0.0, 0.0, 0.0),
            FVector::new(256.0, 0.0, 0.0),
            FVector::new(256.0, 256.0, 0.0),
            FVector::new(0.0, 256.0, 0.0),
        ];

        // Normal (0, 0, 1), TexU (1, 0, 0), TexV (0, 1, 0)
        model.vectors = vec![
            FVector::new(0.0, 0.0, 1.0),
            FVector::new(1.0, 0.0, 0.0),
            FVector::new(0.0, 1.0, 0.0),
        ];

        // Vertices referencing points
        model.verts = vec![
            FVert { p_vertex: 0, i_side: 0 },
            FVert { p_vertex: 1, i_side: 1 },
            FVert { p_vertex: 2, i_side: 2 },
            FVert { p_vertex: 3, i_side: 3 },
        ];

        let mut surf = FBspSurf::default();
        surf.material = 42;
        surf.p_base = 0;
        surf.v_normal = 0;
        surf.v_texture_u = 1;
        surf.v_texture_v = 2;
        surf.i_light_map = 5;
        surf.light_map_scale = 32.0;
        model.bsp_surfs.push(surf);

        let mut node = FBspNode::default();
        node.plane = FPlane::new(0.0, 0.0, 1.0, 0.0);
        node.i_surf = 0;
        node.i_vert_pool = 0;
        node.num_vertices = 4;
        model.bsp_nodes.push(node);

        let meshes = extract_bsp_geometry(&model);
        assert_eq!(meshes.len(), 1);

        let mesh = &meshes[0];
        assert_eq!(mesh.material_index, 42);
        assert_eq!(mesh.lightmap_index, 5);
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3]);

        // Verify UVs
        assert_eq!(mesh.uvs[0], Vec2::new(0.0, 0.0));
        assert_eq!(mesh.uvs[1], Vec2::new(256.0, 0.0));
        assert_eq!(mesh.uvs[2], Vec2::new(256.0, 256.0));
        assert_eq!(mesh.uvs[3], Vec2::new(0.0, 256.0));

        // Verify Lightmap UVs (scale = 32.0)
        assert_eq!(mesh.lightmap_uvs[0], Vec2::new(0.0, 0.0));
        assert_eq!(mesh.lightmap_uvs[1], Vec2::new(8.0, 0.0));
        assert_eq!(mesh.lightmap_uvs[2], Vec2::new(8.0, 8.0));
        assert_eq!(mesh.lightmap_uvs[3], Vec2::new(0.0, 8.0));

        // Interleaved vertices test
        let interleaved = mesh.to_interleaved_vertices();
        assert_eq!(interleaved.len(), 4);
        assert_eq!(interleaved[1].position, [256.0, 0.0, 0.0]);
        assert_eq!(interleaved[1].normal, [0.0, 0.0, 1.0]);
        assert_eq!(interleaved[1].uv, [256.0, 0.0]);
        assert_eq!(interleaved[1].lightmap_uv, [8.0, 0.0]);
    }
}
