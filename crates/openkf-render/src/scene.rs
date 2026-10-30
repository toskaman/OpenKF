use bytemuck::{Pod, Zeroable};
use glam::Mat4;

/// Global scene uniform parameters passed to all shaders in binding 0.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SceneUniforms {
    /// Combined View-Projection matrix (column-major). Offset 0x00 (64 bytes).
    pub view_proj: [f32; 16],
    /// World-to-Camera View matrix. Offset 0x40 (64 bytes).
    pub view: [f32; 16],
    /// Camera-to-Clip Projection matrix. Offset 0x80 (64 bytes).
    pub proj: [f32; 16],
    /// Camera world position (X, Y, Z). Offset 0xC0 (12 bytes).
    pub camera_pos: [f32; 3],
    /// Total elapsed level time in seconds. Offset 0xCC (4 bytes).
    pub time_seconds: f32,
    /// Screen dimensions in pixels (Width, Height). Offset 0xD0 (8 bytes).
    pub screen_size: [f32; 2],
    /// Padding for 16-byte alignment. Offset 0xD8 (8 bytes).
    pub _pad: [f32; 2],
}

impl SceneUniforms {
    pub const SIZE: usize = 224;

    pub fn new(view: Mat4, proj: Mat4, camera_pos: [f32; 3], time_seconds: f32, screen_size: [f32; 2]) -> Self {
        let view_proj = proj * view;
        Self {
            view_proj: view_proj.to_cols_array(),
            view: view.to_cols_array(),
            proj: proj.to_cols_array(),
            camera_pos,
            time_seconds,
            screen_size,
            _pad: [0.0; 2],
        }
    }
}

/// Static mesh LOD section draw descriptor (`FStaticMeshSection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticMeshSectionDraw {
    pub material_index: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub min_vertex_index: u32,
    pub max_vertex_index: u32,
}

impl StaticMeshSectionDraw {
    #[inline]
    pub fn num_triangles(&self) -> u32 {
        self.index_count / 3
    }
}
