//! Static mesh Level of Detail (LOD) model (`FStaticMeshLODModel`).
//!
//! Specified in `scratch/recon_09_static_meshes.md`:
//! Holds sections, 36-byte interleaved vertex stream, and index buffers for a single LOD tier.

use crate::section::{FBox, FSphere, FStaticMeshSection};
use crate::vertex::StaticMeshVertex;
use glam::Vec3;

/// Single Level of Detail representation for a static mesh (`FStaticMeshLODModel`).
#[derive(Clone, Debug, PartialEq)]
pub struct FStaticMeshLODModel {
    /// Render sections grouped by material assignment.
    pub sections: Vec<FStaticMeshSection>,
    /// Contiguous 36-byte interleaved vertex buffer (Pos, Normal, UV0, UV1).
    pub vertices: Vec<StaticMeshVertex>,
    /// 16-bit triangle index buffer (3 indices per triangle).
    pub indices: Vec<u16>,
    /// 16-bit wireframe line index buffer (2 indices per line).
    pub wireframe_indices: Vec<u16>,
    /// Camera distance threshold for switching to this LOD.
    pub lod_distance: f32,
}

impl FStaticMeshLODModel {
    pub fn new(lod_distance: f32) -> Self {
        Self {
            sections: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            wireframe_indices: Vec::new(),
            lod_distance,
        }
    }

    /// Total number of triangles across all sections.
    pub fn total_triangles(&self) -> u32 {
        self.sections.iter().map(|s| s.num_triangles).sum()
    }

    /// Access vertex buffer as raw byte slice for GPU buffer uploads (Vulkan/Direct3D).
    pub fn vertex_buffer_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.vertices)
    }

    /// Access index buffer as raw byte slice for GPU buffer uploads.
    pub fn index_buffer_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.indices)
    }

    /// Validate index buffer integrity against the vertex buffer bounds.
    pub fn validate_indices(&self) -> Result<(), String> {
        let vertex_count = self.vertices.len();
        for (i, &idx) in self.indices.iter().enumerate() {
            if idx as usize >= vertex_count {
                return Err(format!(
                    "Index at position {} references vertex index {}, but vertex count is {}",
                    i, idx, vertex_count
                ));
            }
        }

        for (s_idx, section) in self.sections.iter().enumerate() {
            let start = section.first_index as usize;
            let end = start + (section.num_triangles as usize * 3);
            if end > self.indices.len() {
                return Err(format!(
                    "Section {} bounds [{}..{}] exceed index buffer length {}",
                    s_idx, start, end, self.indices.len()
                ));
            }
        }

        Ok(())
    }

    /// Compute tight bounding box (FBox) from all vertex positions.
    pub fn calculate_bounds(&self) -> (FBox, FSphere) {
        if self.vertices.is_empty() {
            return (FBox::invalid(), FSphere::new(Vec3::ZERO, 0.0));
        }

        let mut bbox = FBox::invalid();
        for v in &self.vertices {
            bbox.extend_by_point(Vec3::from_array(v.position));
        }

        let center = (bbox.min + bbox.max) * 0.5;
        let mut max_radius_sq = 0.0f32;
        for v in &self.vertices {
            let dist_sq = (Vec3::from_array(v.position) - center).length_squared();
            if dist_sq > max_radius_sq {
                max_radius_sq = dist_sq;
            }
        }

        let sphere = FSphere::new(center, max_radius_sq.sqrt());
        (bbox, sphere)
    }
}
