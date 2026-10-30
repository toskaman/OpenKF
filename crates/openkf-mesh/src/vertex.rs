//! Interleaved vertex stream layouts for Unreal Engine 2.5 static meshes.
//!
//! Specified in `scratch/recon_09_static_meshes.md`:
//! - 36-byte interleaved vertex stream: Position (12B), Normal (12B), UV0 (8B), UV1 (4B).
//! - Aligned to 4 bytes for standard GPU vertex buffer bindings.

use bytemuck::{Pod, Zeroable};

/// Convert 32-bit float to IEEE-754 16-bit half precision float.
pub fn f32_to_f16(val: f32) -> u16 {
    let bits = val.to_bits();
    let sign = (bits >> 16) & 0x8000;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let frac = (bits >> 13) & 0x3ff;

    if exp <= 0 {
        sign as u16
    } else if exp >= 31 {
        (sign | 0x7c00 | (frac != 0) as u32) as u16
    } else {
        (sign | ((exp as u32) << 10) | frac) as u16
    }
}

/// Convert IEEE-754 16-bit half precision float to 32-bit float.
pub fn f16_to_f32(val: u16) -> f32 {
    let sign = (val as u32 & 0x8000) << 16;
    let exp = (val as u32 & 0x7c00) >> 10;
    let frac = (val as u32 & 0x03ff) << 13;

    if exp == 0 {
        if frac == 0 {
            f32::from_bits(sign)
        } else {
            let mut e = 127 - 15 + 1;
            let mut m = frac;
            while (m & 0x0080_0000) == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= !0x0080_0000;
            f32::from_bits(sign | ((e as u32) << 23) | m)
        }
    } else if exp == 0x1f {
        f32::from_bits(sign | 0x7f80_0000 | frac)
    } else {
        let e = (exp as i32 - 15 + 127) as u32;
        f32::from_bits(sign | (e << 23) | frac)
    }
}

/// Hardware-aligned interleaved static mesh vertex stream (`sizeof = 36 bytes`).
///
/// Memory layout matching Unreal Engine 2.5 / Killing Floor retail meshes:
/// | Offset | Field Name | Type | Description |
/// |:---|:---|:---|:---|
/// | `0x00` | `position` | `float[3]` | Object-space coordinates ($X, Y, Z$) |
/// | `0x0C` | `normal`   | `float[3]` | Surface normal vector |
/// | `0x18` | `uv0`      | `float[2]` | Diffuse / Normal map texture UV ($U, V$) |
/// | `0x20` | `uv1`      | `uint16[2]`| Precomputed Lightmap half-precision UV ($U_L, V_L$) |
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StaticMeshVertex {
    /// Object-space position coordinates (X, Y, Z). 12 bytes at offset 0x00.
    pub position: [f32; 3],
    /// Normal vector (Nx, Ny, Nz). 12 bytes at offset 0x0C.
    pub normal: [f32; 3],
    /// Diffuse texture coordinates (U0, V0). 8 bytes at offset 0x18.
    pub uv0: [f32; 2],
    /// Lightmap texture coordinates (U1, V1) stored as packed 16-bit floats. 4 bytes at offset 0x20.
    pub uv1: [u16; 2],
}

// Compile-time layout guarantees
const _: () = assert!(std::mem::size_of::<StaticMeshVertex>() == 36);
const _: () = assert!(std::mem::align_of::<StaticMeshVertex>() == 4);

impl StaticMeshVertex {
    /// Construct a vertex from position, normal, UV0 and UV1 (given as full f32 coordinates).
    #[inline]
    pub fn new(position: [f32; 3], normal: [f32; 3], uv0: [f32; 2], uv1: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv0,
            uv1: [f32_to_f16(uv1[0]), f32_to_f16(uv1[1])],
        }
    }

    /// Construct a vertex with raw packed half-float UV1 coordinates.
    #[inline]
    pub fn with_packed_uv1(position: [f32; 3], normal: [f32; 3], uv0: [f32; 2], uv1_packed: [u16; 2]) -> Self {
        Self {
            position,
            normal,
            uv0,
            uv1: uv1_packed,
        }
    }

    /// Decode the packed lightmap UV coordinates to full 32-bit floats.
    #[inline]
    pub fn uv1_f32(&self) -> [f32; 2] {
        [f16_to_f32(self.uv1[0]), f16_to_f32(self.uv1[1])]
    }
}

/// Extended static mesh vertex with uncompressed 32-bit float UV1 (`sizeof = 40 bytes`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StaticMeshVertexExt {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
}

const _: () = assert!(std::mem::size_of::<StaticMeshVertexExt>() == 40);
const _: () = assert!(std::mem::align_of::<StaticMeshVertexExt>() == 4);

impl From<StaticMeshVertexExt> for StaticMeshVertex {
    #[inline]
    fn from(v: StaticMeshVertexExt) -> Self {
        Self::new(v.position, v.normal, v.uv0, v.uv1)
    }
}

impl From<StaticMeshVertex> for StaticMeshVertexExt {
    #[inline]
    fn from(v: StaticMeshVertex) -> Self {
        Self {
            position: v.position,
            normal: v.normal,
            uv0: v.uv0,
            uv1: v.uv1_f32(),
        }
    }
}
