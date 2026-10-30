//! Planar texture coordinate projector and lightmap UV generator for Unreal BSP surfaces.

use glam::{Vec2, Vec3};

/// Projects a 3D world-space vertex coordinate into 2D texture UV space.
///
/// Formula from Unreal Engine 2 `Engine.dll:0x1045baf0`:
/// ```text
/// u = ((P - Base) . U) / ||U||^2
/// v = ((P - Base) . V) / ||V||^2
/// ```
/// Returns `Vec2::ZERO` if vector length is near-zero to prevent divide-by-zero.
#[inline]
pub fn project_texture_uv(pos: Vec3, base: Vec3, tex_u: Vec3, tex_v: Vec3) -> Vec2 {
    let diff = pos - base;

    let len_sq_u = tex_u.length_squared();
    let u = if len_sq_u > 1e-12 {
        diff.dot(tex_u) / len_sq_u
    } else {
        0.0
    };

    let len_sq_v = tex_v.length_squared();
    let v = if len_sq_v > 1e-12 {
        diff.dot(tex_v) / len_sq_v
    } else {
        0.0
    };

    Vec2::new(u, v)
}

/// Projects a 3D world-space vertex coordinate into 2D lightmap UV space.
///
/// Divides texture coordinate by `light_map_scale` (typically 32.0 world units per luxel).
#[inline]
pub fn project_lightmap_uv(pos: Vec3, base: Vec3, tex_u: Vec3, tex_v: Vec3, light_map_scale: f32) -> Vec2 {
    let scale = if light_map_scale > 1e-4 {
        light_map_scale
    } else {
        32.0
    };
    let tex_uv = project_texture_uv(pos, base, tex_u, tex_v);
    tex_uv / scale
}

/// Computes standard Unreal Engine planar texture axes for a given surface normal.
/// Conforms to UnrealEd / Killing Floor coordinate rules:
/// - Ceiling (Normal = (0, 0, +1)): U = (0, -1, 0), V = (-1, 0, 0)
/// - Floor (Normal = (0, 0, -1)): U = (0, +1, 0), V = (-1, 0, 0)
/// - Vertical walls: U = Up x Normal = (-Ny, Nx, 0), V = (0, 0, -1)
pub fn compute_unreal_texture_axes(normal: Vec3, u_scale: f32, v_scale: f32) -> (Vec3, Vec3) {
    let n = normal.normalize();

    let (u_dir, v_dir) = if n.z.abs() > 0.999 {
        if n.z > 0.0 {
            // Ceiling
            (Vec3::new(0.0, -1.0, 0.0), Vec3::new(-1.0, 0.0, 0.0))
        } else {
            // Floor
            (Vec3::new(0.0, 1.0, 0.0), Vec3::new(-1.0, 0.0, 0.0))
        }
    } else {
        let up = Vec3::new(0.0, 0.0, 1.0);
        let u_raw = up.cross(n);
        let u_norm = u_raw.normalize();
        let v_raw = u_norm.cross(n);
        (u_norm, v_raw.normalize())
    };

    (u_dir * u_scale, v_dir * v_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_texture_uv_floor() {
        let base = Vec3::new(0.0, 0.0, 0.0);
        let tex_u = Vec3::new(0.0, 1.0, 0.0);
        let tex_v = Vec3::new(1.0, 0.0, 0.0);

        let p0 = Vec3::new(0.0, 0.0, 0.0);
        let p1 = Vec3::new(100.0, 200.0, 0.0);

        let uv0 = project_texture_uv(p0, base, tex_u, tex_v);
        assert_eq!(uv0, Vec2::new(0.0, 0.0));

        let uv1 = project_texture_uv(p1, base, tex_u, tex_v);
        // diff = (100, 200, 0)
        // u = diff . (0, 1, 0) = 200.0
        // v = diff . (1, 0, 0) = 100.0
        assert_eq!(uv1, Vec2::new(200.0, 100.0));
    }

    #[test]
    fn test_project_lightmap_uv() {
        let base = Vec3::new(0.0, 0.0, 0.0);
        let tex_u = Vec3::new(1.0, 0.0, 0.0);
        let tex_v = Vec3::new(0.0, 1.0, 0.0);
        let pt = Vec3::new(64.0, 32.0, 0.0);

        let lm_uv = project_lightmap_uv(pt, base, tex_u, tex_v, 32.0);
        assert_eq!(lm_uv, Vec2::new(2.0, 1.0));
    }

    #[test]
    fn test_zero_vector_safety() {
        let base = Vec3::ZERO;
        let pt = Vec3::new(10.0, 20.0, 30.0);
        let uv = project_texture_uv(pt, base, Vec3::ZERO, Vec3::ZERO);
        assert_eq!(uv, Vec2::ZERO);
    }
}
