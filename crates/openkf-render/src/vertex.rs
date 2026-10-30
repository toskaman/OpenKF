use bytemuck::{Pod, Zeroable};

/// Interleaved vertex format for static meshes serialized in `.usx` packages.
/// Matches retail Unreal Engine 2.5 `D3D9Drv.dll` static mesh stream 0 layout.
/// Exactly 36 bytes with 4-byte alignment:
/// - 0x00: position [f32; 3] (12 bytes)
/// - 0x0C: normal [f32; 3] (12 bytes)
/// - 0x18: uv0 [f32; 2] (8 bytes)
/// - 0x20: uv1 [u16; 2] (4 bytes, half-precision / 16-bit packed lightmap coordinates)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StaticMeshVertex {
    /// Object-space 3D coordinates (X, Y, Z). Offset 0x00 (12 bytes).
    pub position: [f32; 3],
    /// Surface normal vector (X, Y, Z). Offset 0x0C (12 bytes).
    pub normal: [f32; 3],
    /// Diffuse / Normal map texture coordinates (U, V). Offset 0x18 (8 bytes).
    pub uv0: [f32; 2],
    /// Precomputed lightmap texture coordinates (UL, VL) in 16-bit half/unorm format. Offset 0x20 (4 bytes).
    pub uv1: [u16; 2],
}

impl StaticMeshVertex {
    pub const SIZE: usize = 36;

    #[inline]
    pub fn new(position: [f32; 3], normal: [f32; 3], uv0: [f32; 2], uv1: [u16; 2]) -> Self {
        Self { position, normal, uv0, uv1 }
    }

    /// Construct from standard f32 UV coordinates, quantizing lightmap UV to 16-bit unorm/half.
    pub fn from_f32(position: [f32; 3], normal: [f32; 3], uv0: [f32; 2], uv1: [f32; 2]) -> Self {
        let u1 = (uv1[0].clamp(0.0, 1.0) * 65535.0) as u16;
        let v1 = (uv1[1].clamp(0.0, 1.0) * 65535.0) as u16;
        Self {
            position,
            normal,
            uv0,
            uv1: [u1, v1],
        }
    }

    /// Unpack lightmap UV to f32.
    #[inline]
    pub fn lightmap_uv_f32(&self) -> [f32; 2] {
        [
            self.uv1[0] as f32 / 65535.0,
            self.uv1[1] as f32 / 65535.0,
        ]
    }

    /// Returns the vertex input layout description for Vulkan / modern HAL.
    pub fn vertex_layout() -> VertexLayout {
        VertexLayout {
            stride: Self::SIZE as u32,
            input_rate: VertexInputRate::PerVertex,
            attributes: vec![
                VertexAttribute {
                    location: 0,
                    binding: 0,
                    format: VertexFormat::Float3,
                    offset: 0,
                },
                VertexAttribute {
                    location: 1,
                    binding: 0,
                    format: VertexFormat::Float3,
                    offset: 12,
                },
                VertexAttribute {
                    location: 2,
                    binding: 0,
                    format: VertexFormat::Float2,
                    offset: 24,
                },
                VertexAttribute {
                    location: 3,
                    binding: 0,
                    format: VertexFormat::Half2,
                    offset: 32,
                },
            ],
        }
    }
}

/// Dynamic skeletal mesh vertex format for characters, monsters and weapons (`.ukx`).
/// Contains bind-pose position, normal, UV and up to 4 bone influences.
/// Exactly 56 bytes with 4-byte alignment.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SkeletalMeshVertex {
    /// Bind-pose position (X, Y, Z). Offset 0x00 (12 bytes).
    pub position: [f32; 3],
    /// Bind-pose surface normal (X, Y, Z). Offset 0x0C (12 bytes).
    pub normal: [f32; 3],
    /// Diffuse UV coordinates. Offset 0x18 (8 bytes).
    pub uv: [f32; 2],
    /// 4 bone indices for skinning matrix lookup. Offset 0x20 (8 bytes).
    pub bone_indices: [u16; 4],
    /// 4 bone influence weights (summing to 1.0). Offset 0x28 (16 bytes).
    pub bone_weights: [f32; 4],
}

impl SkeletalMeshVertex {
    pub const SIZE: usize = 56;

    #[inline]
    pub fn new(
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
        bone_indices: [u16; 4],
        bone_weights: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            uv,
            bone_indices,
            bone_weights,
        }
    }

    /// Returns the vertex input layout description for compute/rasterizer pipelines.
    pub fn vertex_layout() -> VertexLayout {
        VertexLayout {
            stride: Self::SIZE as u32,
            input_rate: VertexInputRate::PerVertex,
            attributes: vec![
                VertexAttribute {
                    location: 0,
                    binding: 0,
                    format: VertexFormat::Float3,
                    offset: 0,
                },
                VertexAttribute {
                    location: 1,
                    binding: 0,
                    format: VertexFormat::Float3,
                    offset: 12,
                },
                VertexAttribute {
                    location: 2,
                    binding: 0,
                    format: VertexFormat::Float2,
                    offset: 24,
                },
                VertexAttribute {
                    location: 3,
                    binding: 0,
                    format: VertexFormat::Uint16x4,
                    offset: 32,
                },
                VertexAttribute {
                    location: 4,
                    binding: 0,
                    format: VertexFormat::Float4,
                    offset: 40,
                },
            ],
        }
    }
}

/// Skinned output vertex produced by compute shader skinning (`SV_DispatchThreadID`).
/// Directly consumable by rasterizer. Exactly 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SkinnedOutputVertex {
    /// World or object space deformed coordinate. Offset 0x00 (12 bytes).
    pub position: [f32; 3],
    /// Normalized deformed normal. Offset 0x0C (12 bytes).
    pub normal: [f32; 3],
    /// Pass-through UV coordinates. Offset 0x18 (8 bytes).
    pub uv: [f32; 2],
}

impl SkinnedOutputVertex {
    pub const SIZE: usize = 32;

    #[inline]
    pub fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self { position, normal, uv }
    }

    pub fn vertex_layout() -> VertexLayout {
        VertexLayout {
            stride: Self::SIZE as u32,
            input_rate: VertexInputRate::PerVertex,
            attributes: vec![
                VertexAttribute {
                    location: 0,
                    binding: 0,
                    format: VertexFormat::Float3,
                    offset: 0,
                },
                VertexAttribute {
                    location: 1,
                    binding: 0,
                    format: VertexFormat::Float3,
                    offset: 12,
                },
                VertexAttribute {
                    location: 2,
                    binding: 0,
                    format: VertexFormat::Float2,
                    offset: 24,
                },
            ],
        }
    }
}

/// 2D UI canvas vertex for HUD tiles, text glyphs, crosshair, and GUI windows.
/// Exactly 32 bytes with 4-byte alignment.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct UIVertex {
    /// 2D Screen pixel coordinates (X, Y). Offset 0x00 (8 bytes).
    pub position: [f32; 2],
    /// Normalized texture coordinates (U, V). Offset 0x08 (8 bytes).
    pub uv: [f32; 2],
    /// RGBA vertex tint color (R, G, B, A). Offset 0x10 (16 bytes).
    pub color: [f32; 4],
}

impl UIVertex {
    pub const SIZE: usize = 32;

    #[inline]
    pub fn new(position: [f32; 2], uv: [f32; 2], color: [f32; 4]) -> Self {
        Self { position, uv, color }
    }

    pub fn vertex_layout() -> VertexLayout {
        VertexLayout {
            stride: Self::SIZE as u32,
            input_rate: VertexInputRate::PerVertex,
            attributes: vec![
                VertexAttribute {
                    location: 0,
                    binding: 0,
                    format: VertexFormat::Float2,
                    offset: 0,
                },
                VertexAttribute {
                    location: 1,
                    binding: 0,
                    format: VertexFormat::Float2,
                    offset: 8,
                },
                VertexAttribute {
                    location: 2,
                    binding: 0,
                    format: VertexFormat::Float4,
                    offset: 16,
                },
            ],
        }
    }
}

/// Instance buffer layout for modern GPU Indirect Instanced Rendering (`Pillar 09`).
/// Packed 80 bytes (64-byte 4x4 matrix + 16-byte lightmap UV scale/offset).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MeshInstanceData {
    /// 4x4 World transform matrix (column-major). Offset 0x00 (64 bytes).
    pub world_matrix: [f32; 16],
    /// Lightmap atlas U/V offset (X, Y) and scale (Z, W). Offset 0x40 (16 bytes).
    pub lightmap_offset_scale: [f32; 4],
}

impl MeshInstanceData {
    pub const SIZE: usize = 80;

    #[inline]
    pub fn new(world_matrix: [f32; 16], lightmap_offset_scale: [f32; 4]) -> Self {
        Self {
            world_matrix,
            lightmap_offset_scale,
        }
    }

    pub fn instance_layout(binding: u32) -> VertexLayout {
        VertexLayout {
            stride: Self::SIZE as u32,
            input_rate: VertexInputRate::PerInstance,
            attributes: vec![
                // Matrix column 0
                VertexAttribute {
                    location: 4,
                    binding,
                    format: VertexFormat::Float4,
                    offset: 0,
                },
                // Matrix column 1
                VertexAttribute {
                    location: 5,
                    binding,
                    format: VertexFormat::Float4,
                    offset: 16,
                },
                // Matrix column 2
                VertexAttribute {
                    location: 6,
                    binding,
                    format: VertexFormat::Float4,
                    offset: 32,
                },
                // Matrix column 3
                VertexAttribute {
                    location: 7,
                    binding,
                    format: VertexFormat::Float4,
                    offset: 48,
                },
                // Lightmap offset scale
                VertexAttribute {
                    location: 8,
                    binding,
                    format: VertexFormat::Float4,
                    offset: 64,
                },
            ],
        }
    }
}

/// Vertex input rate for pipeline state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VertexInputRate {
    PerVertex,
    PerInstance,
}

/// Supported hardware vertex attribute data formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VertexFormat {
    Float2,
    Float3,
    Float4,
    Half2,
    Uint16x2,
    Uint16x4,
    Unorm8x4,
}

impl VertexFormat {
    pub fn size_in_bytes(&self) -> usize {
        match self {
            VertexFormat::Float2 => 8,
            VertexFormat::Float3 => 12,
            VertexFormat::Float4 => 16,
            VertexFormat::Half2 => 4,
            VertexFormat::Uint16x2 => 4,
            VertexFormat::Uint16x4 => 8,
            VertexFormat::Unorm8x4 => 4,
        }
    }
}

/// Individual attribute description in vertex declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VertexAttribute {
    pub location: u32,
    pub binding: u32,
    pub format: VertexFormat,
    pub offset: u32,
}

/// Complete vertex stream layout declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VertexLayout {
    pub stride: u32,
    pub input_rate: VertexInputRate,
    pub attributes: Vec<VertexAttribute>,
}
