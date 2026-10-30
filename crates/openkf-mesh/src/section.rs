//! Static mesh section definition.
//!
//! Specified in `scratch/recon_09_static_meshes.md`:
//! Each section represents a contiguous partition of triangles in the index buffer
//! assigned to a specific material index.

use glam::Vec3;

/// 3D Axis-Aligned Bounding Box (FBox).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FBox {
    pub min: Vec3,
    pub max: Vec3,
    pub is_valid: bool,
}

impl FBox {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max, is_valid: true }
    }

    pub fn invalid() -> Self {
        Self { min: Vec3::ZERO, max: Vec3::ZERO, is_valid: false }
    }

    pub fn extend_by_point(&mut self, pt: Vec3) {
        if !self.is_valid {
            self.min = pt;
            self.max = pt;
            self.is_valid = true;
        } else {
            self.min = self.min.min(pt);
            self.max = self.max.max(pt);
        }
    }
}

/// 3D Bounding Sphere (FSphere).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FSphere {
    pub center: Vec3,
    pub radius: f32,
}

impl FSphere {
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }
}

/// A render section of a static mesh Level of Detail (LOD) model (`FStaticMeshSection`).
#[derive(Clone, Debug, PartialEq)]
pub struct FStaticMeshSection {
    /// Index into the parent mesh `Materials` table.
    pub material_index: i32,
    /// Offset to the first index in the LOD index buffer.
    pub first_index: u32,
    /// Number of triangles in this section.
    pub num_triangles: u32,
    /// Minimum vertex index referenced by triangles in this section.
    pub min_vertex_index: u32,
    /// Maximum vertex index referenced by triangles in this section.
    pub max_vertex_index: u32,
    /// Optional section bounding box.
    pub bounding_box: Option<FBox>,
    /// Optional section bounding sphere.
    pub bounding_sphere: Option<FSphere>,
}

impl FStaticMeshSection {
    pub fn new(
        material_index: i32,
        first_index: u32,
        num_triangles: u32,
        min_vertex_index: u32,
        max_vertex_index: u32,
    ) -> Self {
        Self {
            material_index,
            first_index,
            num_triangles,
            min_vertex_index,
            max_vertex_index,
            bounding_box: None,
            bounding_sphere: None,
        }
    }

    /// Number of index elements (triangles * 3).
    #[inline]
    pub fn index_count(&self) -> u32 {
        self.num_triangles * 3
    }
}
