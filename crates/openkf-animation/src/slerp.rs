use glam::{Quat, Vec3};

use crate::bone::BoneTransform;

/// Spherical Linear Interpolation (Slerp) between two unit quaternions `q_a` and `q_b`
/// at blend factor `t` in [0, 1].
///
/// Implements the exact shortest-path Slerp equation specified in `recon_08_skeletal_animation.md`:
/// $$\text{Slerp}(\mathbf{q}_A, \mathbf{q}_B, t) = \frac{\sin((1-t)\theta)}{\sin\theta}\mathbf{q}_A + \frac{\sin(t\theta)}{\sin\theta}\mathbf{q}_B$$
/// Where $\cos\theta = \mathbf{q}_A \cdot \mathbf{q}_B$. If $\cos\theta < 0$, $\mathbf{q}_B$ is negated to ensure
/// the shortest rotational geodesic path.
/// If $\cos\theta > 0.9995$, falls back to normalized linear interpolation (Nlerp) to prevent division by zero.
pub fn slerp(q_a: Quat, mut q_b: Quat, t: f32) -> Quat {
    if t <= 0.0 {
        return q_a;
    }
    if t >= 1.0 {
        return q_b;
    }

    // Dot product between two quaternions corresponds to cos(theta)
    let mut cos_theta = q_a.dot(q_b);

    // If negative, negate q_b to take the shorter arc on the 4D hypersphere
    if cos_theta < 0.0 {
        q_b = -q_b;
        cos_theta = -cos_theta;
    }

    // If quaternions are extremely close, perform Nlerp to avoid 0/0 division
    if cos_theta > 0.9995 {
        let one_minus_t = 1.0 - t;
        let blended = Quat::from_xyzw(
            one_minus_t * q_a.x + t * q_b.x,
            one_minus_t * q_a.y + t * q_b.y,
            one_minus_t * q_a.z + t * q_b.z,
            one_minus_t * q_a.w + t * q_b.w,
        );
        return blended.normalize();
    }

    let cos_theta = cos_theta.clamp(-1.0, 1.0);
    let theta = cos_theta.acos();
    let sin_theta = theta.sin();

    if sin_theta.abs() < 1e-6 {
        return q_a;
    }

    let scale_a = ((1.0 - t) * theta).sin() / sin_theta;
    let scale_b = (t * theta).sin() / sin_theta;

    Quat::from_xyzw(
        scale_a * q_a.x + scale_b * q_b.x,
        scale_a * q_a.y + scale_b * q_b.y,
        scale_a * q_a.z + scale_b * q_b.z,
        scale_a * q_a.w + scale_b * q_b.w,
    )
}

/// Linear interpolation between two 3D vectors.
#[inline]
pub fn lerp_vec3(v_a: Vec3, v_b: Vec3, t: f32) -> Vec3 {
    v_a.lerp(v_b, t)
}

/// Blends two local bone transforms using Slerp for rotation and Lerp for translation & scale.
pub fn blend_transforms(a: &BoneTransform, b: &BoneTransform, t: f32) -> BoneTransform {
    BoneTransform {
        rotation: slerp(a.rotation, b.rotation, t),
        translation: lerp_vec3(a.translation, b.translation, t),
        scale: lerp_vec3(a.scale, b.scale, t),
    }
}

/// Blends two full skeletal poses (arrays of local bone transforms) given a blend weight in [0, 1].
pub fn blend_poses(pose_a: &[BoneTransform], pose_b: &[BoneTransform], weight: f32) -> Vec<BoneTransform> {
    let count = pose_a.len().min(pose_b.len());
    let mut blended = Vec::with_capacity(count);

    for i in 0..count {
        blended.push(blend_transforms(&pose_a[i], &pose_b[i], weight));
    }

    blended
}
