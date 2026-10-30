//! Discrete Oriented Polytope (k-DOP) collision tree implementation.
//!
//! Specified in `scratch/recon_04_physics_collision.md` and `scratch/recon_09_static_meshes.md`:
//!  - kDOP-18: 9 plane pairs (3 axial + 6 diagonal edges)
//!  - kDOP-26: 13 plane pairs (9 pairs + 4 3D corner diagonals)
//!
//! Provides fast ray-slab testing and leaf triangle collision intersection.

use glam::Vec3;
use std::io::{self, Read, Write};

/// Plane normals for kDOP-18 and kDOP-26 bounding volumes.
pub const KDOP_NORMALS_26: [Vec3; 13] = [
    // 3 Axial pairs
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 1.0, 0.0),
    Vec3::new(0.0, 0.0, 1.0),
    // 6 Edge pairs (1/sqrt(2) normalized)
    Vec3::new(std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2, 0.0),
    Vec3::new(std::f32::consts::FRAC_1_SQRT_2, -std::f32::consts::FRAC_1_SQRT_2, 0.0),
    Vec3::new(std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2),
    Vec3::new(std::f32::consts::FRAC_1_SQRT_2, 0.0, -std::f32::consts::FRAC_1_SQRT_2),
    Vec3::new(0.0, std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2),
    Vec3::new(0.0, std::f32::consts::FRAC_1_SQRT_2, -std::f32::consts::FRAC_1_SQRT_2),
    // 4 Corner pairs (1/sqrt(3) normalized)
    Vec3::new(0.57735026, 0.57735026, 0.57735026),
    Vec3::new(0.57735026, 0.57735026, -0.57735026),
    Vec3::new(0.57735026, -0.57735026, 0.57735026),
    Vec3::new(0.57735026, -0.57735026, -0.57735026),
];

/// Supported kDOP polytope configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KdopType {
    /// 9 plane pairs (18 bounding half-spaces)
    Kdop18 = 9,
    /// 13 plane pairs (26 bounding half-spaces)
    Kdop26 = 13,
}

/// A node in the kDOP bounding volume hierarchy (`FkdOPNode`).
#[derive(Clone, Debug, PartialEq)]
pub struct KdopNode {
    /// Minimum projection scalar along each plane normal.
    pub min_values: [f32; 13],
    /// Maximum projection scalar along each plane normal.
    pub max_values: [f32; 13],
    /// Indices of child nodes in the node array (negative if leaf node).
    pub child_indices: [i32; 2],
    /// Offset into the collision triangle array (for leaf nodes).
    pub triangle_index: i32,
    /// Number of collision triangles contained in this leaf.
    pub num_triangles: i32,
}

impl KdopNode {
    pub fn new_leaf(min_values: [f32; 13], max_values: [f32; 13], triangle_index: i32, num_triangles: i32) -> Self {
        Self {
            min_values,
            max_values,
            child_indices: [-1, -1],
            triangle_index,
            num_triangles,
        }
    }

    pub fn new_branch(min_values: [f32; 13], max_values: [f32; 13], left_child: i32, right_child: i32) -> Self {
        Self {
            min_values,
            max_values,
            child_indices: [left_child, right_child],
            triangle_index: -1,
            num_triangles: 0,
        }
    }

    #[inline]
    pub fn is_leaf(&self) -> bool {
        self.child_indices[0] < 0 && self.child_indices[1] < 0
    }

    /// Fast ray-slab intersection test against this kDOP bounding node.
    pub fn intersect_ray(&self, ray_origin: Vec3, ray_dir: Vec3, kdop_type: KdopType, mut t_min: f32, mut t_max: f32) -> bool {
        let plane_count = kdop_type as usize;
        for i in 0..plane_count {
            let normal = KDOP_NORMALS_26[i];
            let ray_dot_n = ray_dir.dot(normal);
            let origin_dot_n = ray_origin.dot(normal);

            if ray_dot_n.abs() > 1e-6 {
                let mut t1 = (self.min_values[i] - origin_dot_n) / ray_dot_n;
                let mut t2 = (self.max_values[i] - origin_dot_n) / ray_dot_n;
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                t_min = t_min.max(t1);
                t_max = t_max.min(t2);

                if t_min > t_max {
                    return false;
                }
            } else {
                // Ray is parallel to the plane slab: must be inside slab
                if origin_dot_n < self.min_values[i] || origin_dot_n > self.max_values[i] {
                    return false;
                }
            }
        }
        t_max >= t_min && t_max >= 0.0
    }
}

/// A simplified collision triangle stored within leaf nodes (`FkdOPCollisionTriangle`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KdopCollisionTriangle {
    /// 3D vertices of the collision triangle.
    pub vertices: [Vec3; 3],
    /// Surface normal vector.
    pub normal: Vec3,
    /// Material or physics surface index.
    pub material_index: i32,
}

impl KdopCollisionTriangle {
    pub fn new(v0: Vec3, v1: Vec3, v2: Vec3, material_index: i32) -> Self {
        let normal = (v1 - v0).cross(v2 - v0).normalize_or_zero();
        Self {
            vertices: [v0, v1, v2],
            normal,
            material_index,
        }
    }

    /// Möller-Trumbore ray-triangle intersection test.
    pub fn intersect_ray(&self, ray_origin: Vec3, ray_dir: Vec3) -> Option<(f32, Vec3)> {
        let [v0, v1, v2] = self.vertices;
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = ray_dir.cross(edge2);
        let a = edge1.dot(h);

        if a.abs() < 1e-6 {
            return None; // Parallel to triangle plane
        }

        let f = 1.0 / a;
        let s = ray_origin - v0;
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            return None;
        }

        let q = s.cross(edge1);
        let v = f * ray_dir.dot(q);
        if v < 0.0 || u + v > 1.0 {
            return None;
        }

        let t = f * edge2.dot(q);
        if t > 1e-6 {
            let hit_point = ray_origin + ray_dir * t;
            Some((t, hit_point))
        } else {
            None
        }
    }
}

/// Result of a kDOP ray-cast query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KdopRayHit {
    /// Distance along ray direction to the hit point.
    pub distance: f32,
    /// World-space hit position.
    pub position: Vec3,
    /// Surface normal at hit location.
    pub normal: Vec3,
    /// Material slot index of the hit triangle.
    pub material_index: i32,
    /// Index of the hit triangle in the kDOP triangle array.
    pub triangle_index: usize,
}

/// A complete Discrete Oriented Polytope collision acceleration tree (`FkdOPTree`).
#[derive(Clone, Debug, PartialEq)]
pub struct KdopTree {
    pub kdop_type: KdopType,
    pub root_index: i32,
    pub nodes: Vec<KdopNode>,
    pub triangles: Vec<KdopCollisionTriangle>,
}

impl KdopTree {
    pub fn new(kdop_type: KdopType) -> Self {
        Self {
            kdop_type,
            root_index: -1,
            nodes: Vec::new(),
            triangles: Vec::new(),
        }
    }

    /// Perform a ray-cast against the entire kDOP BVH hierarchy.
    pub fn ray_cast(&self, ray_origin: Vec3, ray_dir: Vec3, max_distance: f32) -> Option<KdopRayHit> {
        if self.root_index < 0 || self.nodes.is_empty() {
            return None;
        }

        let mut closest_hit: Option<KdopRayHit> = None;
        let mut closest_dist = max_distance;

        let mut stack = Vec::with_capacity(32);
        stack.push(self.root_index as usize);

        while let Some(node_idx) = stack.pop() {
            if node_idx >= self.nodes.len() {
                continue;
            }
            let node = &self.nodes[node_idx];

            if !node.intersect_ray(ray_origin, ray_dir, self.kdop_type, 0.0, closest_dist) {
                continue;
            }

            if node.is_leaf() {
                let start = node.triangle_index as usize;
                let count = node.num_triangles as usize;
                for i in 0..count {
                    let tri_idx = start + i;
                    if tri_idx < self.triangles.len() {
                        let tri = &self.triangles[tri_idx];
                        if let Some((t, hit_pos)) = tri.intersect_ray(ray_origin, ray_dir) {
                            if t < closest_dist {
                                closest_dist = t;
                                closest_hit = Some(KdopRayHit {
                                    distance: t,
                                    position: hit_pos,
                                    normal: tri.normal,
                                    material_index: tri.material_index,
                                    triangle_index: tri_idx,
                                });
                            }
                        }
                    }
                }
            } else {
                let [left, right] = node.child_indices;
                if right >= 0 {
                    stack.push(right as usize);
                }
                if left >= 0 {
                    stack.push(left as usize);
                }
            }
        }

        closest_hit
    }

    /// Deserialize a kDOP collision tree from an Unreal Engine binary stream.
    pub fn deserialize<R: Read>(reader: &mut R, kdop_type: KdopType) -> io::Result<Self> {
        let mut buf4 = [0u8; 4];

        // 1. Read node count
        reader.read_exact(&mut buf4)?;
        let node_count = i32::from_le_bytes(buf4);
        if !(0..=100_000).contains(&node_count) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid kDOP node count"));
        }

        let plane_count = kdop_type as usize;
        let mut nodes = Vec::with_capacity(node_count as usize);

        for _ in 0..node_count {
            let mut min_values = [0.0f32; 13];
            let mut max_values = [0.0f32; 13];

            for p in 0..plane_count {
                reader.read_exact(&mut buf4)?;
                min_values[p] = f32::from_le_bytes(buf4);
            }
            for p in 0..plane_count {
                reader.read_exact(&mut buf4)?;
                max_values[p] = f32::from_le_bytes(buf4);
            }

            reader.read_exact(&mut buf4)?;
            let child0 = i32::from_le_bytes(buf4);
            reader.read_exact(&mut buf4)?;
            let child1 = i32::from_le_bytes(buf4);

            reader.read_exact(&mut buf4)?;
            let tri_idx = i32::from_le_bytes(buf4);
            reader.read_exact(&mut buf4)?;
            let num_tris = i32::from_le_bytes(buf4);

            nodes.push(KdopNode {
                min_values,
                max_values,
                child_indices: [child0, child1],
                triangle_index: tri_idx,
                num_triangles: num_tris,
            });
        }

        // 2. Read triangle count
        reader.read_exact(&mut buf4)?;
        let tri_count = i32::from_le_bytes(buf4);
        if !(0..=500_000).contains(&tri_count) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid kDOP triangle count"));
        }

        let mut triangles = Vec::with_capacity(tri_count as usize);
        for _ in 0..tri_count {
            let mut v = [Vec3::ZERO; 3];
            for vert in &mut v {
                reader.read_exact(&mut buf4)?;
                let x = f32::from_le_bytes(buf4);
                reader.read_exact(&mut buf4)?;
                let y = f32::from_le_bytes(buf4);
                reader.read_exact(&mut buf4)?;
                let z = f32::from_le_bytes(buf4);
                *vert = Vec3::new(x, y, z);
            }

            reader.read_exact(&mut buf4)?;
            let nx = f32::from_le_bytes(buf4);
            reader.read_exact(&mut buf4)?;
            let ny = f32::from_le_bytes(buf4);
            reader.read_exact(&mut buf4)?;
            let nz = f32::from_le_bytes(buf4);
            let normal = Vec3::new(nx, ny, nz);

            reader.read_exact(&mut buf4)?;
            let mat_idx = i32::from_le_bytes(buf4);

            triangles.push(KdopCollisionTriangle {
                vertices: v,
                normal,
                material_index: mat_idx,
            });
        }

        // 3. Read root index (or default to 0 if nodes exist)
        let root_index = if !nodes.is_empty() { 0 } else { -1 };

        Ok(Self {
            kdop_type,
            root_index,
            nodes,
            triangles,
        })
    }

    /// Serialize a kDOP collision tree to binary format.
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let node_count = self.nodes.len() as i32;
        writer.write_all(&node_count.to_le_bytes())?;

        let plane_count = self.kdop_type as usize;
        for node in &self.nodes {
            for p in 0..plane_count {
                writer.write_all(&node.min_values[p].to_le_bytes())?;
            }
            for p in 0..plane_count {
                writer.write_all(&node.max_values[p].to_le_bytes())?;
            }
            writer.write_all(&node.child_indices[0].to_le_bytes())?;
            writer.write_all(&node.child_indices[1].to_le_bytes())?;
            writer.write_all(&node.triangle_index.to_le_bytes())?;
            writer.write_all(&node.num_triangles.to_le_bytes())?;
        }

        let tri_count = self.triangles.len() as i32;
        writer.write_all(&tri_count.to_le_bytes())?;

        for tri in &self.triangles {
            for v in &tri.vertices {
                writer.write_all(&v.x.to_le_bytes())?;
                writer.write_all(&v.y.to_le_bytes())?;
                writer.write_all(&v.z.to_le_bytes())?;
            }
            writer.write_all(&tri.normal.x.to_le_bytes())?;
            writer.write_all(&tri.normal.y.to_le_bytes())?;
            writer.write_all(&tri.normal.z.to_le_bytes())?;
            writer.write_all(&tri.material_index.to_le_bytes())?;
        }

        Ok(())
    }
}
