//! Binary structure definitions for Unreal Engine 2 / Killing Floor BSP world geometry.
//!
//! Sizes and alignments verified against retail Engine.dll:
//! - `FBspNode`: 64 bytes (aligned)
//! - `FBspSurf`: 60 bytes
//! - `FVert`: 8 bytes
//! - `FVector`: 12 bytes
//! - `FPlane`: 16 bytes

use bitflags::bitflags;
use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};

/// 3D Vector in Unreal Engine coordinate space (X=Forward, Y=Right, Z=Up).
/// Exactly 12 bytes: 3 x 32-bit IEEE-754 floats.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct FVector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl FVector {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    pub const UP: Self = Self { x: 0.0, y: 0.0, z: 1.0 };
    pub const FORWARD: Self = Self { x: 1.0, y: 0.0, z: 0.0 };
    pub const RIGHT: Self = Self { x: 0.0, y: 1.0, z: 0.0 };

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[inline]
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > 1e-12 {
            let inv_len = 1.0 / len_sq.sqrt();
            Self {
                x: self.x * inv_len,
                y: self.y * inv_len,
                z: self.z * inv_len,
            }
        } else {
            Self::ZERO
        }
    }

    #[inline]
    pub fn to_vec3(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    #[inline]
    pub fn from_vec3(v: Vec3) -> Self {
        Self { x: v.x, y: v.y, z: v.z }
    }
}

impl std::ops::Add for FVector {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::ops::Sub for FVector {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl std::ops::Mul<f32> for FVector {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl From<Vec3> for FVector {
    #[inline]
    fn from(v: Vec3) -> Self {
        Self::from_vec3(v)
    }
}

impl From<FVector> for Vec3 {
    #[inline]
    fn from(v: FVector) -> Self {
        v.to_vec3()
    }
}

/// 3D Hyperplane equation in Unreal Engine (Ax + By + Cz - W = 0, where W = D).
/// Exactly 16 bytes: 4 x 32-bit floats.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct FPlane {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl FPlane {
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    #[inline]
    pub fn from_point_and_normal(point: FVector, normal: FVector) -> Self {
        let n = normal.normalize();
        let w = n.dot(point);
        Self {
            x: n.x,
            y: n.y,
            z: n.z,
            w,
        }
    }

    #[inline]
    pub fn normal(&self) -> FVector {
        FVector::new(self.x, self.y, self.z)
    }

    #[inline]
    pub fn distance_to_point(&self, p: FVector) -> f32 {
        self.x * p.x + self.y * p.y + self.z * p.z - self.w
    }

    #[inline]
    pub fn to_vec4(self) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, self.w)
    }
}

/// Bounding box serialized as 2 FVectors + 1 u8 validity flag (25 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct FBox {
    pub min: FVector,
    pub max: FVector,
    pub is_valid: u8,
}

/// Bounding sphere serialized as center FVector + radius float (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct FSphere {
    pub center: FVector,
    pub radius: f32,
}

bitflags! {
    /// Node flags for `FBspNode`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct NodeFlags: u8 {
        const IS_FRONT = 0x01;
        const IS_BACK  = 0x02;
        const IS_PLANE = 0x04;
    }
}

bitflags! {
    /// Surface polygon rendering and physical flags for `FBspSurf`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct PolyFlags: u32 {
        const INVISIBLE     = 0x00000001;
        const MASKED        = 0x00000002;
        const TRANSLUCENT   = 0x00000004;
        const NOT_SOLID     = 0x00000008;
        const ENVIRONMENT   = 0x00000010;
        const SEMISOLID     = 0x00000020;
        const MODULATED     = 0x00000040;
        const FAKE_BACKDROP = 0x00000080;
        const TWO_SIDED     = 0x00000100;
        const AUTO_U_PAN    = 0x00000200;
        const AUTO_V_PAN    = 0x00000400;
        const NO_SMOOTH     = 0x00000800;
        const SPECIAL_POLY  = 0x00001000;
        const ALPHA_BLEND   = 0x00004000;
        const UNLIT         = 0x00400000;
        const PORTAL        = 0x04000000;
        const MIRRORED      = 0x20000000;
    }
}

/// Node in Binary Partition Tree (FBspNode).
/// Exactly 64 bytes (aligned). Disassembled from `Engine.dll:0x1045c6c0`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct FBspNode {
    /// 0x00: Hyperplane equation (Ax + By + Cz - D = 0)
    pub plane: FPlane,
    /// 0x10: Bitmask of visibility zones this node intersects
    pub zone_mask: u64,
    /// 0x18: Node property bitmask (NF_IsFront, NF_IsBack)
    pub node_flags: u8,
    /// 0x19: Alignment padding
    pub _pad0: [u8; 3],
    /// 0x1C: Offset into Verts array where node vertices begin
    pub i_vert_pool: i32,
    /// 0x20: Index into BspSurfs array describing surface material
    pub i_surf: i32,
    /// 0x24: Front and back child node indices (-1 if leaf)
    pub i_child: [i32; 2],
    /// 0x2C: kDOP collision box index
    pub i_collision_bound: i32,
    /// 0x30: Front and back zone indices
    pub i_zone: [u8; 2],
    /// 0x32: Number of coplanar polygon vertices (typically 3..8)
    pub num_vertices: u8,
    /// 0x33: Remaining 13 bytes padding / alignment to 64 bytes
    pub _pad1: [u8; 13],
}

impl Default for FBspNode {
    fn default() -> Self {
        Self {
            plane: FPlane::default(),
            zone_mask: 0,
            node_flags: 0,
            _pad0: [0; 3],
            i_vert_pool: -1,
            i_surf: -1,
            i_child: [-1, -1],
            i_collision_bound: -1,
            i_zone: [0, 0],
            num_vertices: 0,
            _pad1: [0; 13],
        }
    }
}

impl FBspNode {
    #[inline]
    pub fn is_leaf(&self) -> bool {
        self.i_child[0] == -1 && self.i_child[1] == -1
    }

    #[inline]
    pub fn front_child(&self) -> Option<usize> {
        if self.i_child[0] >= 0 {
            Some(self.i_child[0] as usize)
        } else {
            None
        }
    }

    #[inline]
    pub fn back_child(&self) -> Option<usize> {
        if self.i_child[1] >= 0 {
            Some(self.i_child[1] as usize)
        } else {
            None
        }
    }
}

/// Surface Polygon & Material Descriptor (FBspSurf).
/// Exactly 60 bytes. Disassembled from `Engine.dll:0x1045baf0`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct FBspSurf {
    /// 0x00: Index into Import/Export table for surface material
    pub material: i32,
    /// 0x04: Surface flags (PF_Translucent, PF_Masked, PF_Unlit, PF_Portal)
    pub poly_flags: u32,
    /// 0x08: Index into Points array (origin point of texture coordinate space)
    pub p_base: i32,
    /// 0x0C: Index into Vectors array (surface normal vector)
    pub v_normal: i32,
    /// 0x10: Index into Vectors array (texture U axis direction and scale)
    pub v_texture_u: i32,
    /// 0x14: Index into Vectors array (texture V axis direction and scale)
    pub v_texture_v: i32,
    /// 0x18: Index into lightmap texture atlas
    pub i_light_map: i32,
    /// 0x1C: World units per lightmap texel (typically 32.0)
    pub light_map_scale: f32,
    /// 0x20: Remaining fields / extra descriptors up to 60 bytes
    pub _extra: [u8; 28],
}

impl Default for FBspSurf {
    fn default() -> Self {
        Self {
            material: -1,
            poly_flags: 0,
            p_base: -1,
            v_normal: -1,
            v_texture_u: -1,
            v_texture_v: -1,
            i_light_map: -1,
            light_map_scale: 32.0,
            _extra: [0; 28],
        }
    }
}

/// Vertex Reference (FVert).
/// Exactly 8 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default)]
pub struct FVert {
    /// 0x00: Index into Points array for 3D coordinate (X, Y, Z)
    pub p_vertex: i32,
    /// 0x04: Index of polygon edge shared with adjacent node
    pub i_side: i32,
}

/// Zone visibility descriptor within a UModel.
/// 20 to 24 bytes in retail packages.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BspZone {
    pub connectivity: u64,
    pub visibility: u64,
    pub last_render_time: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn test_binary_struct_sizes() {
        assert_eq!(size_of::<FBspNode>(), 64, "FBspNode must be exactly 64 bytes");
        assert_eq!(size_of::<FBspSurf>(), 60, "FBspSurf must be exactly 60 bytes");
        assert_eq!(size_of::<FVert>(), 8, "FVert must be exactly 8 bytes");
        assert_eq!(size_of::<FVector>(), 12, "FVector must be exactly 12 bytes");
        assert_eq!(size_of::<FPlane>(), 16, "FPlane must be exactly 16 bytes");
        assert_eq!(size_of::<FBox>(), 25, "FBox must be exactly 25 bytes");
        assert_eq!(size_of::<FSphere>(), 16, "FSphere must be exactly 16 bytes");
    }

    #[test]
    fn test_fbsp_node_offsets() {
        assert_eq!(offset_of!(FBspNode, plane), 0x00);
        assert_eq!(offset_of!(FBspNode, zone_mask), 0x10);
        assert_eq!(offset_of!(FBspNode, node_flags), 0x18);
        assert_eq!(offset_of!(FBspNode, i_vert_pool), 0x1C);
        assert_eq!(offset_of!(FBspNode, i_surf), 0x20);
        assert_eq!(offset_of!(FBspNode, i_child), 0x24);
        assert_eq!(offset_of!(FBspNode, i_collision_bound), 0x2C);
        assert_eq!(offset_of!(FBspNode, i_zone), 0x30);
        assert_eq!(offset_of!(FBspNode, num_vertices), 0x32);
        assert_eq!(offset_of!(FBspNode, _pad1), 0x33);
    }

    #[test]
    fn test_fbsp_surf_offsets() {
        assert_eq!(offset_of!(FBspSurf, material), 0x00);
        assert_eq!(offset_of!(FBspSurf, poly_flags), 0x04);
        assert_eq!(offset_of!(FBspSurf, p_base), 0x08);
        assert_eq!(offset_of!(FBspSurf, v_normal), 0x0C);
        assert_eq!(offset_of!(FBspSurf, v_texture_u), 0x10);
        assert_eq!(offset_of!(FBspSurf, v_texture_v), 0x14);
        assert_eq!(offset_of!(FBspSurf, i_light_map), 0x18);
        assert_eq!(offset_of!(FBspSurf, light_map_scale), 0x1C);
        assert_eq!(offset_of!(FBspSurf, _extra), 0x20);
    }

    #[test]
    fn test_fvert_offsets() {
        assert_eq!(offset_of!(FVert, p_vertex), 0x00);
        assert_eq!(offset_of!(FVert, i_side), 0x04);
    }
}
