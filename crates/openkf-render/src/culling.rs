//! CPU frustum culling for BSP batches and skeletal actors.
//!
//! Extracts 6 planes from view-projection matrix and tests bounding
//! spheres / AABBs before issuing draw calls. Hierarchical Z-buffer
//! occlusion is approximated by front-to-back sorting + early-Z.

use glam::{Mat4, Vec3, Vec4};

/// Single frustum plane: normal + distance (ax+by+cz+d=0, inside is >=0).
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub normal: Vec3,
    pub d: f32,
}

impl Plane {
    #[inline]
    pub fn distance(&self, p: Vec3) -> f32 {
        self.normal.dot(p) + self.d
    }
}

/// 6-plane view frustum extracted from view-projection matrix.
#[derive(Clone, Debug)]
pub struct Frustum {
    pub planes: [Plane; 6], // left, right, bottom, top, near, far
}

impl Frustum {
    /// Extract frustum planes from combined view-projection matrix (column-major).
    pub fn from_view_proj(vp: Mat4) -> Self {
        let m = vp.to_cols_array();
        // Rows from column-major: row = [m[col*4+row]]
        let row = |r: usize| Vec4::new(m[r], m[4 + r], m[8 + r], m[12 + r]);
        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);
        let norm = |v: Vec4| {
            let n = Vec3::new(v.x, v.y, v.z);
            let l = n.length();
            if l > 1e-6 { Plane { normal: n / l, d: v.w / l } } else { Plane { normal: n, d: v.w } }
        };
        Self {
            planes: [
                norm(r3 + r0), // left
                norm(r3 - r0), // right
                norm(r3 + r1), // bottom
                norm(r3 - r1), // top
                norm(r3 + r2), // near
                norm(r3 - r2), // far
            ],
        }
    }

    /// Sphere test: true if sphere is at least partially inside.
    #[inline]
    pub fn sphere_visible(&self, center: Vec3, radius: f32) -> bool {
        for p in &self.planes {
            if p.distance(center) < -radius { return false; }
        }
        true
    }

    /// AABB test (center + half-extents).
    #[inline]
    pub fn aabb_visible(&self, center: Vec3, half: Vec3) -> bool {
        for p in &self.planes {
            let r = half.x * p.normal.x.abs() + half.y * p.normal.y.abs() + half.z * p.normal.z.abs();
            if p.distance(center) < -r { return false; }
        }
        true
    }
}

/// Indirect draw args matching VkDrawIndexedIndirectCommand / D3D12.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndexedIndirectArgs {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frustum_culls_behind() {
        let vp = Mat4::perspective_rh(1.0, 1.0, 0.1, 100.0) * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let f = Frustum::from_view_proj(vp);
        assert!(f.sphere_visible(Vec3::ZERO, 1.0));
        assert!(!f.sphere_visible(Vec3::new(0.0, 0.0, 20.0), 1.0)); // behind camera
    }
}
