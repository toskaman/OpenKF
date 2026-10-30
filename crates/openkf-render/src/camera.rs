use glam::{Mat4, Vec3, Vec4};

/// Unreal Engine 2 rotator represented as 32-bit integer units (0..65535 per 360 degrees).
///
/// Coordinate system is Left-Handed Cartesian Z-Up:
/// - `+X`: Forward
/// - `+Y`: Right
/// - `+Z`: Up
///
/// Rotation angles:
/// - `pitch`: Tilt up/down (rotation around local right axis)
/// - `yaw`: Turn left/right (rotation around world +Z axis)
/// - `roll`: Bank/tilt (rotation around local forward axis)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CameraRotator {
    pub pitch: i32,
    pub yaw: i32,
    pub roll: i32,
}

impl CameraRotator {
    pub const ZERO: Self = Self {
        pitch: 0,
        yaw: 0,
        roll: 0,
    };

    pub const UNITS_PER_CIRCLE: f32 = 65536.0;
    pub const UNITS_TO_DEGREES: f32 = 360.0 / 65536.0;
    pub const DEGREES_TO_UNITS: f32 = 65536.0 / 360.0;
    pub const UNITS_TO_RADIANS: f32 = std::f32::consts::TAU / 65536.0;
    pub const RADIANS_TO_UNITS: f32 = 65536.0 / std::f32::consts::TAU;

    #[inline]
    pub const fn new(pitch: i32, yaw: i32, roll: i32) -> Self {
        Self { pitch, yaw, roll }
    }

    /// Construct from angles in degrees.
    #[inline]
    pub fn from_degrees(pitch_deg: f32, yaw_deg: f32, roll_deg: f32) -> Self {
        Self {
            pitch: (pitch_deg * Self::DEGREES_TO_UNITS).round() as i32,
            yaw: (yaw_deg * Self::DEGREES_TO_UNITS).round() as i32,
            roll: (roll_deg * Self::DEGREES_TO_UNITS).round() as i32,
        }
    }

    /// Construct from angles in radians.
    #[inline]
    pub fn from_radians(pitch_rad: f32, yaw_rad: f32, roll_rad: f32) -> Self {
        Self {
            pitch: (pitch_rad * Self::RADIANS_TO_UNITS).round() as i32,
            yaw: (yaw_rad * Self::RADIANS_TO_UNITS).round() as i32,
            roll: (roll_rad * Self::RADIANS_TO_UNITS).round() as i32,
        }
    }

    /// Convert to (pitch, yaw, roll) in degrees.
    #[inline]
    pub fn to_degrees(&self) -> (f32, f32, f32) {
        (
            self.pitch as f32 * Self::UNITS_TO_DEGREES,
            self.yaw as f32 * Self::UNITS_TO_DEGREES,
            self.roll as f32 * Self::UNITS_TO_DEGREES,
        )
    }

    /// Convert to (pitch, yaw, roll) in radians.
    #[inline]
    pub fn to_radians(&self) -> (f32, f32, f32) {
        (
            self.pitch as f32 * Self::UNITS_TO_RADIANS,
            self.yaw as f32 * Self::UNITS_TO_RADIANS,
            self.roll as f32 * Self::UNITS_TO_RADIANS,
        )
    }

    /// Normalize rotator angles into the range `[0, 65535]`.
    #[inline]
    pub fn normalize(&self) -> Self {
        Self {
            pitch: self.pitch.rem_euclid(65536),
            yaw: self.yaw.rem_euclid(65536),
            roll: self.roll.rem_euclid(65536),
        }
    }

    /// Extract orthonormal basis vectors `(forward, right, up)` matching canonical UE2 `GetAxes`.
    pub fn get_axes(&self) -> (Vec3, Vec3, Vec3) {
        let rad = std::f32::consts::PI / 32768.0;
        let cp = (self.pitch as f32 * rad).cos();
        let sp = (self.pitch as f32 * rad).sin();
        let cy = (self.yaw as f32 * rad).cos();
        let sy = (self.yaw as f32 * rad).sin();
        let cr = (self.roll as f32 * rad).cos();
        let sr = (self.roll as f32 * rad).sin();

        let forward = Vec3::new(cp * cy, cp * sy, sp);
        let right = Vec3::new(
            sr * sp * cy - cr * sy,
            sr * sp * sy + cr * cy,
            -sr * cp,
        );
        let up = Vec3::new(
            -(cr * sp * cy + sr * sy),
            -cr * sp * sy + sr * cy,
            cr * cp,
        );

        (forward, right, up)
    }

    #[inline]
    pub fn forward(&self) -> Vec3 {
        let (fwd, _, _) = self.get_axes();
        fwd
    }

    #[inline]
    pub fn right(&self) -> Vec3 {
        let (_, right, _) = self.get_axes();
        right
    }

    #[inline]
    pub fn up(&self) -> Vec3 {
        let (_, _, up) = self.get_axes();
        up
    }
}

/// 3D Infinite or semi-infinite ray for picking, queries, and culling tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    #[inline]
    pub fn point_at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

/// Frustum plane represented in Hessian normal form: `normal · P + distance = 0`.
///
/// A point `P` is inside or on the plane if `normal · P + distance >= 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrustumPlane {
    pub normal: Vec3,
    pub distance: f32,
}

impl FrustumPlane {
    pub fn new(normal: Vec3, distance: f32) -> Self {
        let len = normal.length();
        if len > 0.0 {
            Self {
                normal: normal / len,
                distance: distance / len,
            }
        } else {
            Self {
                normal: Vec3::ZERO,
                distance: 0.0,
            }
        }
    }

    #[inline]
    pub fn signed_distance(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

/// 6-plane viewing frustum extracted from a View-Projection matrix targeting Vulkan clip space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    pub planes: [FrustumPlane; 6],
}

impl Frustum {
    pub const LEFT: usize = 0;
    pub const RIGHT: usize = 1;
    pub const TOP: usize = 2;
    pub const BOTTOM: usize = 3;
    pub const NEAR: usize = 4;
    pub const FAR: usize = 5;

    /// Extract the 6 normalized culling planes from a View-Projection matrix targeting Vulkan NDC.
    ///
    /// In Vulkan:
    /// - Left:   `w + x >= 0`
    /// - Right:  `w - x >= 0`
    /// - Top:    `w + y >= 0`  (since Vulkan NDC Y: -1 is top)
    /// - Bottom: `w - y >= 0`  (since Vulkan NDC Y: +1 is bottom)
    /// - Near:   `z >= 0`      (Vulkan depth [0, 1])
    /// - Far:    `w - z >= 0`
    pub fn from_view_proj(vp: Mat4) -> Self {
        let r0 = vp.row(0);
        let r1 = vp.row(1);
        let r2 = vp.row(2);
        let r3 = vp.row(3);

        let left = r3 + r0;
        let right = r3 - r0;
        let top = r3 + r1;
        let bottom = r3 - r1;
        let near = r2;
        let far = r3 - r2;

        Self {
            planes: [
                FrustumPlane::new(left.truncate(), left.w),
                FrustumPlane::new(right.truncate(), right.w),
                FrustumPlane::new(top.truncate(), top.w),
                FrustumPlane::new(bottom.truncate(), bottom.w),
                FrustumPlane::new(near.truncate(), near.w),
                FrustumPlane::new(far.truncate(), far.w),
            ],
        }
    }

    #[inline]
    pub fn left(&self) -> &FrustumPlane {
        &self.planes[Self::LEFT]
    }

    #[inline]
    pub fn right(&self) -> &FrustumPlane {
        &self.planes[Self::RIGHT]
    }

    #[inline]
    pub fn top(&self) -> &FrustumPlane {
        &self.planes[Self::TOP]
    }

    #[inline]
    pub fn bottom(&self) -> &FrustumPlane {
        &self.planes[Self::BOTTOM]
    }

    #[inline]
    pub fn near(&self) -> &FrustumPlane {
        &self.planes[Self::NEAR]
    }

    #[inline]
    pub fn far(&self) -> &FrustumPlane {
        &self.planes[Self::FAR]
    }

    /// Check if a 3D point is inside all 6 frustum planes.
    pub fn contains_point(&self, point: Vec3) -> bool {
        self.planes.iter().all(|p| p.signed_distance(point) >= 0.0)
    }

    /// Check if a bounding sphere intersects or is inside the frustum.
    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        self.planes.iter().all(|p| p.signed_distance(center) >= -radius)
    }

    /// Check if an Axis-Aligned Bounding Box (AABB) intersects or is inside the frustum.
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for plane in &self.planes {
            // Select positive vertex (furthest corner along plane normal)
            let p = Vec3::new(
                if plane.normal.x >= 0.0 { max.x } else { min.x },
                if plane.normal.y >= 0.0 { max.y } else { min.y },
                if plane.normal.z >= 0.0 { max.z } else { min.z },
            );
            if plane.signed_distance(p) < 0.0 {
                return false;
            }
        }
        true
    }
}

/// Convert horizontal FOV (radians) to vertical FOV (radians) given aspect ratio (width / height).
#[inline]
pub fn fov_x_to_fov_y(fov_x_rad: f32, aspect_ratio: f32) -> f32 {
    2.0 * ((fov_x_rad * 0.5).tan() / aspect_ratio).atan()
}

/// Convert vertical FOV (radians) to horizontal FOV (radians) given aspect ratio (width / height).
#[inline]
pub fn fov_y_to_fov_x(fov_y_rad: f32, aspect_ratio: f32) -> f32 {
    2.0 * (aspect_ratio * (fov_y_rad * 0.5).tan()).atan()
}

/// Convert horizontal FOV (degrees) to vertical FOV (radians) given aspect ratio.
#[inline]
pub fn fov_x_deg_to_fov_y_rad(fov_x_deg: f32, aspect_ratio: f32) -> f32 {
    fov_x_to_fov_y(fov_x_deg.to_radians(), aspect_ratio)
}

/// Convert vertical FOV (radians) to horizontal FOV (degrees) given aspect ratio.
#[inline]
pub fn fov_y_rad_to_fov_x_deg(fov_y_rad: f32, aspect_ratio: f32) -> f32 {
    fov_y_to_fov_x(fov_y_rad, aspect_ratio).to_degrees()
}

/// Compute a world-to-camera View matrix from camera position and rotator in UE2 Z-Up coordinates.
/// Output camera space: `+X` is Right, `+Y` is Up, `+Z` is Forward into screen.
pub fn compute_view_matrix(position: Vec3, rotator: &CameraRotator) -> Mat4 {
    let (forward, right, up) = rotator.get_axes();
    Mat4::from_cols(
        Vec4::new(right.x, up.x, forward.x, 0.0),
        Vec4::new(right.y, up.y, forward.y, 0.0),
        Vec4::new(right.z, up.z, forward.z, 0.0),
        Vec4::new(-right.dot(position), -up.dot(position), -forward.dot(position), 1.0),
    )
}

/// Compute a view matrix using eye, target, and world up vectors.
pub fn compute_look_at_matrix(eye: Vec3, target: Vec3, world_up: Vec3) -> Mat4 {
    let forward = (target - eye).normalize_or_zero();
    let right = (world_up.cross(forward)).normalize_or_zero();
    let up = (forward.cross(right)).normalize_or_zero();
    Mat4::from_cols(
        Vec4::new(right.x, up.x, forward.x, 0.0),
        Vec4::new(right.y, up.y, forward.y, 0.0),
        Vec4::new(right.z, up.z, forward.z, 0.0),
        Vec4::new(-right.dot(eye), -up.dot(eye), -forward.dot(eye), 1.0),
    )
}

/// Compute a Vulkan perspective projection matrix from camera space (+X Right, +Y Up, +Z Forward)
/// to Vulkan NDC (X [-1, 1] Right, Y [-1, 1] Down, Z [0, 1] Near-to-Far).
pub fn compute_vulkan_perspective(fov_y: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
    let mut proj = Mat4::perspective_lh(fov_y, aspect_ratio, z_near, z_far);
    proj.y_axis.y *= -1.0;
    proj
}

/// Compute an infinite far plane Vulkan perspective projection matrix.
pub fn compute_vulkan_perspective_infinite(fov_y: f32, aspect_ratio: f32, z_near: f32) -> Mat4 {
    let mut proj = Mat4::perspective_infinite_lh(fov_y, aspect_ratio, z_near);
    proj.y_axis.y *= -1.0;
    proj
}

/// Compute a reverse-Z Vulkan perspective projection matrix (Z: 1.0 at near, 0.0 at far).
pub fn compute_vulkan_perspective_reverse_z(fov_y: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    let s_x = f / aspect_ratio;
    let s_y = -f; // Vulkan Y-down
    let a = z_near / (z_near - z_far);
    let b = -(z_far * z_near) / (z_near - z_far);

    Mat4::from_cols(
        Vec4::new(s_x, 0.0, 0.0, 0.0),
        Vec4::new(0.0, s_y, 0.0, 0.0),
        Vec4::new(0.0, 0.0, a, 1.0),
        Vec4::new(0.0, 0.0, b, 0.0),
    )
}

/// First-Person 3D Camera system for OpenKF.
///
/// Converts Unreal Engine 2 world coordinates (Left-Handed Z-Up) into Vulkan Normalized Device Coordinates
/// (NDC: X [-1, 1] Right, Y [-1, 1] Down, Z [0, 1] Depth).
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    /// World position of the camera in UE2 Z-Up coordinates (+X Forward, +Y Right, +Z Up).
    pub position: Vec3,
    /// Pitch angle in radians (looking up/down).
    pub pitch: f32,
    /// Yaw angle in radians (looking left/right).
    pub yaw: f32,
    /// Roll angle in radians (tilt/banking).
    pub roll: f32,
    /// Field of view in degrees (default 90.0).
    pub fov_degrees: f32,
    /// Viewport aspect ratio (width / height).
    pub aspect_ratio: f32,
    /// Near clipping plane distance in Unreal units (cm).
    pub near_clip: f32,
    /// Far clipping plane distance in Unreal units (cm).
    pub far_clip: f32,

    // Cached matrices
    view_matrix: Mat4,
    proj_matrix: Mat4,
    view_proj_matrix: Mat4,
    inv_view_proj_matrix: Mat4,
}

impl Default for Camera {
    fn default() -> Self {
        let mut cam = Self {
            position: Vec3::ZERO,
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
            fov_degrees: 90.0,
            aspect_ratio: 16.0 / 9.0,
            near_clip: 1.0,
            far_clip: 40000.0,
            view_matrix: Mat4::IDENTITY,
            proj_matrix: Mat4::IDENTITY,
            view_proj_matrix: Mat4::IDENTITY,
            inv_view_proj_matrix: Mat4::IDENTITY,
        };
        cam.recompute_matrices();
        cam
    }
}

impl Camera {
    /// Create a camera at a given position with FOV and aspect ratio.
    pub fn new(position: Vec3, fov_degrees: f32, aspect_ratio: f32) -> Self {
        let mut cam = Self {
            position,
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
            fov_degrees,
            aspect_ratio: if aspect_ratio > 0.0 { aspect_ratio } else { 1.0 },
            near_clip: 1.0,
            far_clip: 40000.0,
            view_matrix: Mat4::IDENTITY,
            proj_matrix: Mat4::IDENTITY,
            view_proj_matrix: Mat4::IDENTITY,
            inv_view_proj_matrix: Mat4::IDENTITY,
        };
        cam.recompute_matrices();
        cam
    }

    /// Construct from Unreal Engine rotator units (65536 units = 360 degrees = TAU radians).
    pub fn from_ue_rotator(
        position: Vec3,
        pitch_ue: i32,
        yaw_ue: i32,
        roll_ue: i32,
        fov_degrees: f32,
        aspect_ratio: f32,
    ) -> Self {
        let rot = CameraRotator::new(pitch_ue, yaw_ue, roll_ue);
        let (p, y, r) = rot.to_radians();
        let mut cam = Self {
            position,
            pitch: p,
            yaw: y,
            roll: r,
            fov_degrees,
            aspect_ratio: if aspect_ratio > 0.0 { aspect_ratio } else { 1.0 },
            near_clip: 1.0,
            far_clip: 40000.0,
            view_matrix: Mat4::IDENTITY,
            proj_matrix: Mat4::IDENTITY,
            view_proj_matrix: Mat4::IDENTITY,
            inv_view_proj_matrix: Mat4::IDENTITY,
        };
        cam.recompute_matrices();
        cam
    }

    /// Full constructor specifying horizontal FOV in degrees.
    pub fn from_fov_x(
        position: Vec3,
        pitch_ue: i32,
        yaw_ue: i32,
        roll_ue: i32,
        fov_x_deg: f32,
        aspect_ratio: f32,
        z_near: f32,
        z_far: f32,
    ) -> Self {
        let rot = CameraRotator::new(pitch_ue, yaw_ue, roll_ue);
        let (p, y, r) = rot.to_radians();
        let mut cam = Self {
            position,
            pitch: p,
            yaw: y,
            roll: r,
            fov_degrees: fov_x_deg,
            aspect_ratio: if aspect_ratio > 0.0 { aspect_ratio } else { 1.0 },
            near_clip: z_near,
            far_clip: z_far,
            view_matrix: Mat4::IDENTITY,
            proj_matrix: Mat4::IDENTITY,
            view_proj_matrix: Mat4::IDENTITY,
            inv_view_proj_matrix: Mat4::IDENTITY,
        };
        cam.recompute_matrices();
        cam
    }

    /// Update camera position and orientation using UE2 Rotator integer units (0..65535).
    pub fn update(&mut self, pos: Vec3, pitch: i32, yaw: i32, roll: i32) {
        let rot = CameraRotator::new(pitch, yaw, roll);
        let (p, y, r) = rot.to_radians();
        self.position = pos;
        self.pitch = p;
        self.yaw = y;
        self.roll = r;
        self.recompute_matrices();
    }

    /// Update camera position and orientation using Euler angles in degrees.
    pub fn update_deg(&mut self, pos: Vec3, pitch_deg: f32, yaw_deg: f32, roll_deg: f32) {
        self.position = pos;
        self.pitch = pitch_deg.to_radians();
        self.yaw = yaw_deg.to_radians();
        self.roll = roll_deg.to_radians();
        self.recompute_matrices();
    }

    /// Update camera position and orientation using Euler angles in radians.
    pub fn update_rad(&mut self, pos: Vec3, pitch_rad: f32, yaw_rad: f32, roll_rad: f32) {
        self.position = pos;
        self.pitch = pitch_rad;
        self.yaw = yaw_rad;
        self.roll = roll_rad;
        self.recompute_matrices();
    }

    /// Update camera position without altering rotation.
    pub fn set_position(&mut self, pos: Vec3) {
        self.position = pos;
        self.recompute_matrices();
    }

    /// Update camera orientation using UE2 Rotator integer units.
    pub fn set_rotation(&mut self, pitch: i32, yaw: i32, roll: i32) {
        let rot = CameraRotator::new(pitch, yaw, roll);
        let (p, y, r) = rot.to_radians();
        self.pitch = p;
        self.yaw = y;
        self.roll = r;
        self.recompute_matrices();
    }

    /// Updates aspect ratio from viewport pixel dimensions.
    pub fn update_aspect_ratio(&mut self, width: u32, height: u32) {
        if height > 0 {
            self.aspect_ratio = width as f32 / height as f32;
            self.recompute_matrices();
        }
    }

    /// Update viewport aspect ratio (width / height).
    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.aspect_ratio = aspect_ratio;
        self.recompute_matrices();
    }

    /// Update horizontal FOV in degrees.
    pub fn set_fov_degrees(&mut self, fov_deg: f32) {
        self.fov_degrees = fov_deg;
        self.recompute_matrices();
    }

    /// Update near and far clipping planes.
    pub fn set_clip_planes(&mut self, z_near: f32, z_far: f32) {
        self.near_clip = z_near;
        self.far_clip = z_far;
        self.recompute_matrices();
    }

    /// Re-orients the camera to look directly at a target point in world coordinates.
    pub fn look_at(&mut self, target: Vec3, _up: Vec3) {
        let dir = (target - self.position).normalize_or_zero();
        self.yaw = dir.y.atan2(dir.x);
        self.pitch = dir.z.asin();
        self.roll = 0.0;
        self.recompute_matrices();
    }

    /// Recompute view, projection, and combined view-projection matrices.
    pub fn recompute_matrices(&mut self) {
        let rot = self.rotator();
        self.view_matrix = compute_view_matrix(self.position, &rot);
        let fov_y = fov_x_deg_to_fov_y_rad(self.fov_degrees, self.aspect_ratio);
        self.proj_matrix = compute_vulkan_perspective(fov_y, self.aspect_ratio, self.near_clip, self.far_clip);
        self.view_proj_matrix = self.proj_matrix * self.view_matrix;
        self.inv_view_proj_matrix = self.view_proj_matrix.inverse();
    }

    #[inline]
    pub fn get_view_proj_matrix(&self) -> Mat4 {
        self.view_proj_matrix
    }

    #[inline]
    pub fn view_proj_matrix(&self) -> Mat4 {
        self.view_proj_matrix
    }

    #[inline]
    pub fn view_projection_matrix(&self) -> Mat4 {
        self.view_proj_matrix
    }

    #[inline]
    pub fn get_view_matrix(&self) -> Mat4 {
        self.view_matrix
    }

    #[inline]
    pub fn view_matrix(&self) -> Mat4 {
        self.view_matrix
    }

    #[inline]
    pub fn get_proj_matrix(&self) -> Mat4 {
        self.proj_matrix
    }

    #[inline]
    pub fn proj_matrix(&self) -> Mat4 {
        self.proj_matrix
    }

    #[inline]
    pub fn projection_matrix(&self) -> Mat4 {
        self.proj_matrix
    }

    #[inline]
    pub fn inv_view_proj_matrix(&self) -> Mat4 {
        self.inv_view_proj_matrix
    }

    #[inline]
    pub fn inv_view_matrix(&self) -> Mat4 {
        self.view_matrix.inverse()
    }

    #[inline]
    pub fn inv_proj_matrix(&self) -> Mat4 {
        self.proj_matrix.inverse()
    }

    #[inline]
    pub fn position(&self) -> Vec3 {
        self.position
    }

    #[inline]
    pub fn rotator(&self) -> CameraRotator {
        CameraRotator::from_radians(self.pitch, self.yaw, self.roll)
    }

    #[inline]
    pub fn forward(&self) -> Vec3 {
        self.rotator().forward()
    }

    #[inline]
    pub fn right(&self) -> Vec3 {
        self.rotator().right()
    }

    #[inline]
    pub fn up(&self) -> Vec3 {
        self.rotator().up()
    }

    #[inline]
    pub fn get_axes(&self) -> (Vec3, Vec3, Vec3) {
        self.rotator().get_axes()
    }

    /// Extract viewing frustum for CPU/GPU visibility culling.
    pub fn frustum(&self) -> Frustum {
        Frustum::from_view_proj(self.view_proj_matrix)
    }

    /// Export to `SceneUniforms` for binding 0 global shader uniform buffer.
    pub fn to_scene_uniforms(&self, time_seconds: f32, screen_size: [f32; 2]) -> crate::scene::SceneUniforms {
        crate::scene::SceneUniforms::new(
            self.view_matrix,
            self.proj_matrix,
            [self.position.x, self.position.y, self.position.z],
            time_seconds,
            screen_size,
        )
    }

    /// Unproject a 2D viewport screen coordinate in pixels to a 3D world space picking Ray.
    ///
    /// - `screen_pos`: (x, y) with (0,0) at top-left.
    /// - `screen_size`: (width, height) in pixels.
    pub fn screen_to_ray(&self, screen_pos: glam::Vec2, screen_size: glam::Vec2) -> Ray {
        let ndc_x = (2.0 * screen_pos.x / screen_size.x) - 1.0;
        let ndc_y = (2.0 * screen_pos.y / screen_size.y) - 1.0;

        let near_clip = Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far_clip = Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

        let near_world = self.inv_view_proj_matrix * near_clip;
        let far_world = self.inv_view_proj_matrix * far_clip;

        let near_pt = near_world.truncate() / near_world.w;
        let far_pt = far_world.truncate() / far_world.w;

        Ray::new(near_pt, far_pt - near_pt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-4;

    #[test]
    fn test_ue2_rotator_axes_identity() {
        let rot = CameraRotator::ZERO;
        let (fwd, right, up) = rot.get_axes();

        // At identity: Forward is +X, Right is +Y, Up is +Z
        assert!((fwd - Vec3::X).length() < EPSILON);
        assert!((right - Vec3::Y).length() < EPSILON);
        assert!((up - Vec3::Z).length() < EPSILON);

        // Orthonormality checks
        assert!(fwd.dot(right).abs() < EPSILON);
        assert!(right.dot(up).abs() < EPSILON);
        assert!(fwd.dot(up).abs() < EPSILON);
        assert!((fwd.length() - 1.0).abs() < EPSILON);
        assert!((right.length() - 1.0).abs() < EPSILON);
        assert!((up.length() - 1.0).abs() < EPSILON);
    }

    #[test]
    fn test_ue2_rotator_yaw_90() {
        // Yaw = 16384 (90 degrees turning right)
        let rot = CameraRotator::new(0, 16384, 0);
        let (fwd, right, up) = rot.get_axes();

        assert!((fwd - Vec3::Y).length() < EPSILON);
        assert!((right - (-Vec3::X)).length() < EPSILON);
        assert!((up - Vec3::Z).length() < EPSILON);
    }

    #[test]
    fn test_ue2_rotator_pitch_90() {
        // Pitch = 16384 (90 degrees looking straight up)
        let rot = CameraRotator::new(16384, 0, 0);
        let (fwd, right, up) = rot.get_axes();

        assert!((fwd - Vec3::Z).length() < EPSILON);
        assert!((right - Vec3::Y).length() < EPSILON);
        assert!((up - (-Vec3::X)).length() < EPSILON);
    }

    #[test]
    fn test_fov_conversions() {
        let aspect = 16.0 / 9.0;
        let fov_x_deg = 90.0;
        let fov_y_rad = fov_x_deg_to_fov_y_rad(fov_x_deg, aspect);
        let fov_y_deg = fov_y_rad.to_degrees();

        // 90 deg horizontal FOV at 16:9 gives ~58.7155 degrees vertical FOV
        assert!((fov_y_deg - 58.7155).abs() < 0.01);

        // Roundtrip back to horizontal degrees
        let roundtrip_fov_x_deg = fov_y_rad_to_fov_x_deg(fov_y_rad, aspect);
        assert!((roundtrip_fov_x_deg - fov_x_deg).abs() < 0.01);
    }

    #[test]
    fn test_vulkan_perspective_ndc_mapping() {
        let fov_y = 90.0_f32.to_radians();
        let aspect = 1.0_f32;
        let z_near = 1.0_f32;
        let z_far = 100.0_f32;

        let proj = compute_vulkan_perspective(fov_y, aspect, z_near, z_far);

        // In camera space: +X Right, +Y Up, +Z Forward
        // Near plane center: (0, 0, 1)
        let near_clip = proj * Vec4::new(0.0, 0.0, 1.0, 1.0);
        let near_ndc = near_clip.truncate() / near_clip.w;
        assert!(near_ndc.x.abs() < EPSILON);
        assert!(near_ndc.y.abs() < EPSILON);
        assert!((near_ndc.z - 0.0).abs() < EPSILON); // Vulkan Near is 0.0

        // Far plane center: (0, 0, 100)
        let far_clip = proj * Vec4::new(0.0, 0.0, 100.0, 1.0);
        let far_ndc = far_clip.truncate() / far_clip.w;
        assert!(far_ndc.x.abs() < EPSILON);
        assert!(far_ndc.y.abs() < EPSILON);
        assert!((far_ndc.z - 1.0).abs() < EPSILON); // Vulkan Far is 1.0

        // Top edge at z=1: tan(45 deg) = 1.0 -> y = +1.0
        // In Vulkan NDC, top of screen is Y = -1.0
        let top_clip = proj * Vec4::new(0.0, 1.0, 1.0, 1.0);
        let top_ndc = top_clip.truncate() / top_clip.w;
        assert!((top_ndc.y - (-1.0)).abs() < EPSILON);

        // Right edge at z=1: x = +1.0 -> x_ndc = +1.0
        let right_clip = proj * Vec4::new(1.0, 0.0, 1.0, 1.0);
        let right_ndc = right_clip.truncate() / right_clip.w;
        assert!((right_ndc.x - 1.0).abs() < EPSILON);
    }

    #[test]
    fn test_camera_update_and_view_matrix() {
        let mut cam = Camera::new(Vec3::new(100.0, 200.0, 50.0), 90.0, 16.0 / 9.0);
        assert_eq!(cam.position(), Vec3::new(100.0, 200.0, 50.0));

        // Update to new pos and orientation (yaw = 90 deg -> facing +Y)
        cam.update(Vec3::new(0.0, 0.0, 0.0), 0, 16384, 0);
        assert_eq!(cam.position(), Vec3::ZERO);
        assert_eq!(cam.rotator().yaw, 16384);

        let (fwd, right, up) = cam.get_axes();
        assert!((fwd - Vec3::Y).length() < EPSILON);
        assert!((right - (-Vec3::X)).length() < EPSILON);
        assert!((up - Vec3::Z).length() < EPSILON);

        let view = cam.get_view_matrix();
        // Camera position (0,0,0) in world maps to (0,0,0,1) in camera space
        let origin_cam = view * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((origin_cam.truncate() - Vec3::ZERO).length() < EPSILON);

        // Point 10 units along forward (0, 10, 0) maps to (0, 0, 10, 1) in camera space
        let fwd_pt = view * Vec4::new(0.0, 10.0, 0.0, 1.0);
        assert!((fwd_pt.truncate() - Vec3::new(0.0, 0.0, 10.0)).length() < EPSILON);
    }

    #[test]
    fn test_frustum_point_culling() {
        let cam = Camera::from_fov_x(
            Vec3::ZERO,
            0,
            0,
            0, // Looking along +X
            90.0,
            1.0,
            1.0,
            100.0,
        );

        let frustum = cam.frustum();

        // Point inside frustum (ahead at x=50)
        assert!(frustum.contains_point(Vec3::new(50.0, 0.0, 0.0)));

        // Point behind camera (x = -5)
        assert!(!frustum.contains_point(Vec3::new(-5.0, 0.0, 0.0)));

        // Point beyond far plane (x = 150)
        assert!(!frustum.contains_point(Vec3::new(150.0, 0.0, 0.0)));

        // Point closer than near plane (x = 0.5)
        assert!(!frustum.contains_point(Vec3::new(0.5, 0.0, 0.0)));

        // Point outside horizontal FOV (x=10, y=50)
        assert!(!frustum.contains_point(Vec3::new(10.0, 50.0, 0.0)));

        // Point outside vertical FOV (x=10, z=50)
        assert!(!frustum.contains_point(Vec3::new(10.0, 0.0, 50.0)));
    }

    #[test]
    fn test_frustum_sphere_culling() {
        let cam = Camera::from_fov_x(
            Vec3::ZERO,
            0,
            0,
            0, // Looking along +X
            90.0,
            1.0,
            1.0,
            100.0,
        );
        let frustum = cam.frustum();

        // Sphere well inside frustum
        assert!(frustum.intersects_sphere(Vec3::new(20.0, 0.0, 0.0), 5.0));

        // Sphere straddling near plane (center at x=0.5, radius=2.0 -> reaches x=2.5 inside)
        assert!(frustum.intersects_sphere(Vec3::new(0.5, 0.0, 0.0), 2.0));

        // Sphere completely behind camera
        assert!(!frustum.intersects_sphere(Vec3::new(-20.0, 0.0, 0.0), 5.0));

        // Sphere completely to the right
        assert!(!frustum.intersects_sphere(Vec3::new(20.0, 100.0, 0.0), 5.0));
    }

    #[test]
    fn test_frustum_aabb_culling() {
        let cam = Camera::from_fov_x(
            Vec3::ZERO,
            0,
            0,
            0, // Looking along +X
            90.0,
            1.0,
            1.0,
            100.0,
        );
        let frustum = cam.frustum();

        // AABB inside frustum
        assert!(frustum.intersects_aabb(Vec3::new(10.0, -2.0, -2.0), Vec3::new(20.0, 2.0, 2.0)));

        // AABB behind camera
        assert!(!frustum.intersects_aabb(Vec3::new(-30.0, -2.0, -2.0), Vec3::new(-10.0, 2.0, 2.0)));

        // AABB far out to the right
        assert!(!frustum.intersects_aabb(Vec3::new(10.0, 150.0, -2.0), Vec3::new(20.0, 180.0, 2.0)));
    }

    #[test]
    fn test_screen_to_ray() {
        let cam = Camera::new(Vec3::new(0.0, 0.0, 100.0), 90.0, 16.0 / 9.0);
        let screen_size = glam::Vec2::new(1920.0, 1080.0);
        let screen_center = glam::Vec2::new(960.0, 540.0);

        let ray = cam.screen_to_ray(screen_center, screen_size);

        // Screen center ray should shoot along camera forward direction (+X at identity)
        assert!((ray.direction - cam.forward()).length() < 0.01);
        assert!((ray.direction.length() - 1.0).abs() < EPSILON);
    }

    #[test]
    fn test_scene_uniforms_export() {
        let cam = Camera::new(Vec3::new(12.0, 34.0, 56.0), 90.0, 16.0 / 9.0);
        let uniforms = cam.to_scene_uniforms(2.5, [1920.0, 1080.0]);

        assert_eq!(uniforms.camera_pos, [12.0, 34.0, 56.0]);
        assert_eq!(uniforms.time_seconds, 2.5);
        assert_eq!(uniforms.screen_size, [1920.0, 1080.0]);
        assert_eq!(uniforms.view, cam.get_view_matrix().to_cols_array());
        assert_eq!(uniforms.proj, cam.get_proj_matrix().to_cols_array());
        assert_eq!(uniforms.view_proj, cam.get_view_proj_matrix().to_cols_array());
    }
}
