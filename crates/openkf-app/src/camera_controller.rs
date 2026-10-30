//! Camera Rotation & Sensitivity Controller (v1065 / UE2.5 Clean-Room)
//!
//! Handles raw mouse delta conversion to camera yaw and pitch angles,
//! configurable sensitivity, Source/UE style `m_yaw` and `m_pitch` (0.022 default),
//! pitch clamping to [-89.0, +89.0] degrees to prevent gimbal lock,
//! and smooth FOV scaling for iron sights / scope aiming (ADS).

use glam::{Mat4, Quat, Vec3};

/// Standard mouse angle multiplier (degrees per mouse count) used in Quake / Source / UE2.
pub const DEFAULT_M_YAW: f32 = 0.022;
pub const DEFAULT_M_PITCH: f32 = 0.022;

/// Default camera pitch limits in degrees to avoid gimbal lock flip.
pub const DEFAULT_MIN_PITCH: f32 = -89.0;
pub const DEFAULT_MAX_PITCH: f32 = 89.0;

/// Default baseline Field of View (FOV) in degrees.
pub const DEFAULT_BASE_FOV: f32 = 90.0;

/// Mode used to calculate mouse sensitivity scaling when FOV changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FovScalingMode {
    /// Linear scaling: `scale = fov / base_fov`.
    /// Classic fast scaling used in many competitive FPS titles.
    Linear,
    /// Tangent / Focal length scaling: `tan(fov / 2) / tan(base_fov / 2)`.
    /// Preserves exact 1:1 on-screen pixel tracking distance (0% monitor match).
    FocalLength,
    /// No scaling: sensitivity remains unchanged regardless of zoom.
    Disabled,
}

/// Configuration options for the camera aim and sensitivity controller.
#[derive(Debug, Clone)]
pub struct CameraControllerConfig {
    /// Horizontal angle multiplier in degrees per mouse count (default: 0.022).
    pub m_yaw: f32,
    /// Vertical angle multiplier in degrees per mouse count (default: 0.022).
    pub m_pitch: f32,
    /// Minimum pitch angle clamp in degrees (default: -89.0).
    pub min_pitch: f32,
    /// Maximum pitch angle clamp in degrees (default: 89.0).
    pub max_pitch: f32,
    /// Base reference FOV in degrees (default: 90.0).
    pub base_fov: f32,
    /// Invert vertical pitch axis (default: false).
    pub invert_pitch: bool,
    /// FOV scaling mode for iron sights / scoping (default: Linear).
    pub fov_scaling_mode: FovScalingMode,
    /// Smooth interpolation speed for FOV transitions (units / sec, default: 12.0).
    pub fov_transition_speed: f32,
}

impl Default for CameraControllerConfig {
    fn default() -> Self {
        Self {
            m_yaw: DEFAULT_M_YAW,
            m_pitch: DEFAULT_M_PITCH,
            min_pitch: DEFAULT_MIN_PITCH,
            max_pitch: DEFAULT_MAX_PITCH,
            base_fov: DEFAULT_BASE_FOV,
            invert_pitch: false,
            fov_scaling_mode: FovScalingMode::Linear,
            fov_transition_speed: 12.0,
        }
    }
}

/// Camera rotation and aim controller.
///
/// Converts raw mouse input deltas into yaw/pitch rotations, handles pitch clamping,
/// sensitivity factors, and FOV scaling for hip-fire and Aim-Down-Sights (ADS) modes.
#[derive(Debug, Clone)]
pub struct CameraAimController {
    /// Current yaw angle in degrees [0.0, 360.0).
    pub yaw: f32,
    /// Current pitch angle in degrees [min_pitch, max_pitch].
    pub pitch: f32,
    /// Current roll angle in degrees (default: 0.0).
    pub roll: f32,
    /// Controller configuration parameters.
    pub config: CameraControllerConfig,
    /// Current smooth FOV in degrees.
    pub current_fov: f32,
    /// Target FOV in degrees (for ADS / scoping transitions).
    pub target_fov: f32,
}

impl Default for CameraAimController {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraAimController {
    /// Create a new camera aim controller with default configuration.
    pub fn new() -> Self {
        Self::with_config(CameraControllerConfig::default())
    }

    /// Create a new camera aim controller with specified initial yaw and pitch.
    pub fn with_angles(yaw: f32, pitch: f32) -> Self {
        let mut controller = Self::new();
        controller.set_angles(yaw, pitch);
        controller
    }

    /// Create a new camera aim controller with custom configuration.
    pub fn with_config(config: CameraControllerConfig) -> Self {
        let base_fov = config.base_fov;
        Self {
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            config,
            current_fov: base_fov,
            target_fov: base_fov,
        }
    }

    /// Returns current (yaw, pitch) in degrees.
    #[inline]
    pub fn angles(&self) -> (f32, f32) {
        (self.yaw, self.pitch)
    }

    /// Explicitly set camera yaw and pitch angles, applying pitch clamping and yaw wrapping.
    pub fn set_angles(&mut self, yaw: f32, pitch: f32) {
        self.yaw = normalize_yaw(yaw);
        self.pitch = pitch.clamp(self.config.min_pitch, self.config.max_pitch);
    }

    /// Calculate the sensitivity scale multiplier for a given FOV.
    pub fn calculate_fov_scale(&self, fov: f32) -> f32 {
        let safe_fov = fov.clamp(1.0, 179.0);
        let safe_base = self.config.base_fov.clamp(1.0, 179.0);

        match self.config.fov_scaling_mode {
            FovScalingMode::Disabled => 1.0,
            FovScalingMode::Linear => safe_fov / safe_base,
            FovScalingMode::FocalLength => {
                let half_fov_rad = (safe_fov * 0.5).to_radians();
                let half_base_rad = (safe_base * 0.5).to_radians();
                half_fov_rad.tan() / half_base_rad.tan()
            }
        }
    }

    /// Compute the yaw and pitch delta without mutating internal state.
    ///
    /// - `dx`: raw horizontal mouse delta.
    /// - `dy`: raw vertical mouse delta.
    /// - `sensitivity`: user sensitivity multiplier.
    /// - `fov`: current FOV used for scaling.
    ///
    /// Returns `(delta_yaw, delta_pitch)` in degrees.
    pub fn compute_mouse_delta(&self, dx: f32, dy: f32, sensitivity: f32, fov: f32) -> (f32, f32) {
        let fov_scale = self.calculate_fov_scale(fov);
        let delta_yaw = dx * sensitivity * self.config.m_yaw * fov_scale;

        let pitch_sign = if self.config.invert_pitch { -1.0 } else { 1.0 };
        let delta_pitch = dy * sensitivity * self.config.m_pitch * fov_scale * pitch_sign;

        (delta_yaw, delta_pitch)
    }

    /// Apply raw mouse deltas to yaw and pitch based on sensitivity and FOV.
    ///
    /// Pitch is clamped to `[min_pitch, max_pitch]` (default `[-89.0, +89.0]`),
    /// and yaw is normalized to `[0.0, 360.0)`.
    ///
    /// Returns the updated `(yaw, pitch)` in degrees.
    pub fn apply_mouse_delta(
        &mut self,
        dx: f32,
        dy: f32,
        sensitivity: f32,
        fov: f32,
    ) -> (f32, f32) {
        let (delta_yaw, delta_pitch) = self.compute_mouse_delta(dx, dy, sensitivity, fov);

        self.yaw = normalize_yaw(self.yaw + delta_yaw);
        self.pitch = (self.pitch + delta_pitch).clamp(self.config.min_pitch, self.config.max_pitch);

        (self.yaw, self.pitch)
    }

    /// Update smooth FOV transition towards `target_fov` over delta time `dt`.
    ///
    /// Useful for smooth Aim-Down-Sights (ADS) or sniper zoom in/out.
    pub fn update_fov_transition(&mut self, dt: f32) -> f32 {
        if (self.current_fov - self.target_fov).abs() < 1e-4 {
            self.current_fov = self.target_fov;
            return self.current_fov;
        }

        // Exponential smoothing: 1.0 - exp(-speed * dt)
        let speed = self.config.fov_transition_speed.max(0.1);
        let factor = 1.0 - (-speed * dt.max(0.0)).exp();
        self.current_fov += (self.target_fov - self.current_fov) * factor;

        self.current_fov
    }

    /// Set a new target FOV for smooth transition (e.g., scoping or ADS).
    pub fn set_target_fov(&mut self, target_fov: f32) {
        self.target_fov = target_fov.clamp(1.0, 179.0);
    }

    /// Instantly set the current and target FOV without smoothing.
    pub fn set_instant_fov(&mut self, fov: f32) {
        let clamped = fov.clamp(1.0, 179.0);
        self.current_fov = clamped;
        self.target_fov = clamped;
    }

    /// Switch to Aim-Down-Sights (ADS) zoom FOV.
    pub fn set_aim_down_sights(&mut self, ads_fov: f32) {
        self.set_target_fov(ads_fov);
    }

    /// Reset FOV back to base hip-fire FOV.
    pub fn reset_to_hip_fire(&mut self) {
        self.set_target_fov(self.config.base_fov);
    }

    /// Compute unit forward vector based on current yaw and pitch.
    ///
    /// Uses standard 3D camera convention:
    ///  - Pitch rotates around local X (up/down).
    ///  - Yaw rotates around world Y or Z.
    ///
    /// In standard OpenGL/Vulkan/glam convention:
    /// +X = Right, +Y = Up, -Z = Forward.
    pub fn forward_vector(&self) -> Vec3 {
        let yaw_rad = self.yaw.to_radians();
        let pitch_rad = self.pitch.to_radians();

        let cos_p = pitch_rad.cos();
        let sin_p = pitch_rad.sin();
        let cos_y = yaw_rad.cos();
        let sin_y = yaw_rad.sin();

        // Forward direction: looking into screen
        Vec3::new(cos_p * sin_y, sin_p, -cos_p * cos_y).normalize_or_zero()
    }

    /// Compute unit right vector.
    pub fn right_vector(&self) -> Vec3 {
        let yaw_rad = self.yaw.to_radians();
        Vec3::new(yaw_rad.cos(), 0.0, yaw_rad.sin()).normalize_or_zero()
    }

    /// Compute unit up vector.
    pub fn up_vector(&self) -> Vec3 {
        self.right_vector().cross(self.forward_vector()).normalize_or_zero()
    }

    /// Compute Unreal Engine / Killing Floor coordinate forward vector:
    /// In UE2 / UE: +X is Forward, +Y is Right, +Z is Up.
    pub fn forward_vector_ue(&self) -> Vec3 {
        let yaw_rad = self.yaw.to_radians();
        let pitch_rad = self.pitch.to_radians();

        let cos_p = pitch_rad.cos();
        let sin_p = pitch_rad.sin();
        let cos_y = yaw_rad.cos();
        let sin_y = yaw_rad.sin();

        Vec3::new(cos_p * cos_y, cos_p * sin_y, sin_p).normalize_or_zero()
    }

    /// Compute orientation quaternion from Euler angles (yaw, pitch, roll).
    pub fn rotation_quat(&self) -> Quat {
        let yaw_rad = self.yaw.to_radians();
        let pitch_rad = self.pitch.to_radians();
        let roll_rad = self.roll.to_radians();

        Quat::from_euler(glam::EulerRot::YXZ, yaw_rad, pitch_rad, roll_rad)
    }

    /// Build a view matrix looking from `eye_position` in the current aim direction.
    pub fn view_matrix(&self, eye_position: Vec3) -> Mat4 {
        let target = eye_position + self.forward_vector();
        Mat4::look_at_rh(eye_position, target, Vec3::Y)
    }
}

/// Normalize yaw angle to [0.0, 360.0) range.
#[inline]
pub fn normalize_yaw(yaw: f32) -> f32 {
    let rem = yaw.rem_euclid(360.0);
    if rem < 0.0 {
        rem + 360.0
    } else {
        rem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_initialization() {
        let controller = CameraAimController::new();
        assert_eq!(controller.yaw, 0.0);
        assert_eq!(controller.pitch, 0.0);
        assert_eq!(controller.config.m_yaw, 0.022);
        assert_eq!(controller.config.m_pitch, 0.022);
        assert_eq!(controller.config.min_pitch, -89.0);
        assert_eq!(controller.config.max_pitch, 89.0);
        assert_eq!(controller.config.base_fov, 90.0);
        assert_eq!(controller.current_fov, 90.0);
        assert_eq!(controller.target_fov, 90.0);
    }

    #[test]
    fn test_apply_mouse_delta_basic() {
        let mut controller = CameraAimController::new();
        // At fov 90.0 and sensitivity 1.0, fov_scale = 1.0
        // dx = 100 counts -> 100 * 1.0 * 0.022 * 1.0 = 2.2 degrees
        // dy = 50 counts -> 50 * 1.0 * 0.022 * 1.0 = 1.1 degrees
        let (new_yaw, new_pitch) = controller.apply_mouse_delta(100.0, 50.0, 1.0, 90.0);

        assert!((new_yaw - 2.2).abs() < 1e-4, "Expected yaw 2.2, got {}", new_yaw);
        assert!((new_pitch - 1.1).abs() < 1e-4, "Expected pitch 1.1, got {}", new_pitch);
        assert_eq!(controller.angles(), (new_yaw, new_pitch));
    }

    #[test]
    fn test_sensitivity_multiplier() {
        let mut controller = CameraAimController::new();
        // Sensitivity 2.5: dx = 100 -> 100 * 2.5 * 0.022 = 5.5 degrees
        let (yaw, pitch) = controller.apply_mouse_delta(100.0, 100.0, 2.5, 90.0);
        assert!((yaw - 5.5).abs() < 1e-4);
        assert!((pitch - 5.5).abs() < 1e-4);
    }

    #[test]
    fn test_pitch_clamping_limits() {
        let mut controller = CameraAimController::new();

        // Push pitch way above 89.0 degrees
        controller.apply_mouse_delta(0.0, 10000.0, 1.0, 90.0);
        assert_eq!(controller.pitch, 89.0, "Pitch must clamp to +89.0");

        // Push pitch way below -89.0 degrees
        controller.apply_mouse_delta(0.0, -20000.0, 1.0, 90.0);
        assert_eq!(controller.pitch, -89.0, "Pitch must clamp to -89.0");
    }

    #[test]
    fn test_yaw_wrapping() {
        let mut controller = CameraAimController::new();

        // Large positive delta wraps past 360
        // 360 / 0.022 ~= 16363.636 counts
        controller.apply_mouse_delta(20000.0, 0.0, 1.0, 90.0);
        assert!(controller.yaw >= 0.0 && controller.yaw < 360.0);

        // Negative delta wraps around 0
        controller.yaw = 10.0;
        controller.apply_mouse_delta(-1000.0, 0.0, 1.0, 90.0); // -22 degrees
        // 10 - 22 = -12 -> 348.0
        assert!((controller.yaw - 348.0).abs() < 1e-4);
    }

    #[test]
    fn test_fov_scaling_linear() {
        let mut controller = CameraAimController::new();

        // Hip-fire FOV = 90.0: dx = 100 -> delta_yaw = 2.2
        let (yaw_hip, _) = controller.apply_mouse_delta(100.0, 0.0, 1.0, 90.0);
        assert!((yaw_hip - 2.2).abs() < 1e-4);

        controller.yaw = 0.0;
        // Iron sights FOV = 45.0 (half FOV): fov_scale = 0.5 -> delta_yaw = 1.1
        let (yaw_ads, _) = controller.apply_mouse_delta(100.0, 0.0, 1.0, 45.0);
        assert!((yaw_ads - 1.1).abs() < 1e-4, "Expected half sensitivity at half FOV");
    }

    #[test]
    fn test_fov_scaling_focal_length() {
        let mut config = CameraControllerConfig::default();
        config.fov_scaling_mode = FovScalingMode::FocalLength;
        let controller = CameraAimController::with_config(config);

        // At base_fov (90), tan(45) / tan(45) == 1.0
        let scale_90 = controller.calculate_fov_scale(90.0);
        assert!((scale_90 - 1.0).abs() < 1e-4);

        // At fov 45, tan(22.5 deg) / tan(45 deg) ~= 0.41421356
        let scale_45 = controller.calculate_fov_scale(45.0);
        let expected = (22.5f32.to_radians()).tan() / (45.0f32.to_radians()).tan();
        assert!((scale_45 - expected).abs() < 1e-4);
    }

    #[test]
    fn test_invert_pitch() {
        let mut config = CameraControllerConfig::default();
        config.invert_pitch = true;
        let mut controller = CameraAimController::with_config(config);

        let (_, pitch) = controller.apply_mouse_delta(0.0, 100.0, 1.0, 90.0);
        assert!((pitch - (-2.2)).abs() < 1e-4, "Inverted pitch should be negative for positive dy");
    }

    #[test]
    fn test_smooth_fov_transition() {
        let mut controller = CameraAimController::new();
        assert_eq!(controller.current_fov, 90.0);

        // Transition to scope FOV (30.0)
        controller.set_aim_down_sights(30.0);
        assert_eq!(controller.target_fov, 30.0);

        // Step 50ms
        let fov_step1 = controller.update_fov_transition(0.05);
        assert!(fov_step1 < 90.0 && fov_step1 > 30.0);

        // Step several times until convergence
        for _ in 0..100 {
            controller.update_fov_transition(0.05);
        }
        assert!((controller.current_fov - 30.0).abs() < 1e-3);

        // Reset to hipfire
        controller.reset_to_hip_fire();
        assert_eq!(controller.target_fov, 90.0);
        for _ in 0..100 {
            controller.update_fov_transition(0.05);
        }
        assert!((controller.current_fov - 90.0).abs() < 1e-3);
    }

    #[test]
    fn test_vectors_orthogonality_and_normalization() {
        let controller = CameraAimController::with_angles(45.0, 30.0);
        let fwd = controller.forward_vector();
        let right = controller.right_vector();
        let up = controller.up_vector();

        assert!((fwd.length() - 1.0).abs() < 1e-4, "Forward vector should be normalized");
        assert!((right.length() - 1.0).abs() < 1e-4, "Right vector should be normalized");
        assert!((up.length() - 1.0).abs() < 1e-4, "Up vector should be normalized");

        assert!(fwd.dot(right).abs() < 1e-4, "Forward and Right should be orthogonal");
        assert!(fwd.dot(up).abs() < 1e-4, "Forward and Up should be orthogonal");
        assert!(right.dot(up).abs() < 1e-4, "Right and Up should be orthogonal");
    }

    #[test]
    fn test_ue_forward_vector() {
        let controller = CameraAimController::with_angles(0.0, 0.0);
        let fwd_ue = controller.forward_vector_ue();
        // In UE2, Yaw 0 Pitch 0 looks down +X
        assert!((fwd_ue.x - 1.0).abs() < 1e-4);
        assert!(fwd_ue.y.abs() < 1e-4);
        assert!(fwd_ue.z.abs() < 1e-4);
    }
}
