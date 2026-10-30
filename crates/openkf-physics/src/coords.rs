//! Coordinate transformation and unit conversion subsystem.
//!
//! Unreal Engine 2.5 uses Unreal Units (UU) in a Left-Handed, Z-Up coordinate system:
//! - +X: Forward
//! - +Y: Right
//! - +Z: Up
//! - 1 UU = 0.02 m (50 UU = 1.0 m)
//!
//! Standard SI Metric (Jolt / Modern Physics / Graphics) uses Right-Handed coordinates:
//! - Forward: +X
//! - Up: +Y (or configurable Z-Up)
//! - Right: -Z (or Right-Handed rule)
//!
//! Exact position mapping specified for OpenKF:
//! `(x, y, z) -> (0.02 * x, 0.02 * z, -0.02 * y)`

use glam::{Mat3, Quat, Vec3};

/// Conversion factor from Unreal Units (UU) to Meters (SI).
pub const UU_TO_METERS: f32 = 0.02;

/// Conversion factor from Meters (SI) to Unreal Units (UU).
pub const METERS_TO_UU: f32 = 50.0;

/// Unreal Engine Rotator angle resolution: 65536 units = 360 degrees = 2*PI radians.
pub const UU_ROTATOR_TO_RAD: f32 = std::f32::consts::TAU / 65536.0;

/// Radian to Unreal Engine Rotator angle resolution.
pub const RAD_TO_UU_ROTATOR: f32 = 65536.0 / std::f32::consts::TAU;

/// Convert a position from Unreal Units (Z-Up, Left-Handed) to SI Metric (Meters, Right-Handed).
/// Formula: `(x, y, z) -> (0.02x, 0.02z, -0.02y)`
#[inline]
pub fn ue_to_metric_position(ue_pos: Vec3) -> Vec3 {
    Vec3::new(
        ue_pos.x * UU_TO_METERS,
        ue_pos.z * UU_TO_METERS,
        -ue_pos.y * UU_TO_METERS,
    )
}

/// Convert a position from SI Metric (Meters, Right-Handed) to Unreal Units (Z-Up, Left-Handed).
/// Formula: `(x, y, z) -> (50.0x, -50.0z, 50.0y)`
#[inline]
pub fn metric_to_ue_position(si_pos: Vec3) -> Vec3 {
    Vec3::new(
        si_pos.x * METERS_TO_UU,
        -si_pos.z * METERS_TO_UU,
        si_pos.y * METERS_TO_UU,
    )
}

/// Convert a linear velocity from Unreal Units/sec to SI Meters/sec.
#[inline]
pub fn ue_to_metric_velocity(ue_vel: Vec3) -> Vec3 {
    ue_to_metric_position(ue_vel)
}

/// Convert a linear velocity from SI Meters/sec to Unreal Units/sec.
#[inline]
pub fn metric_to_ue_velocity(si_vel: Vec3) -> Vec3 {
    metric_to_ue_position(si_vel)
}

/// Convert a direction vector or surface normal from UE (Z-Up LH) to SI (Y-Up RH).
/// Unit length is preserved: `(x, y, z) -> (x, z, -y)`.
#[inline]
pub fn ue_to_metric_direction(ue_dir: Vec3) -> Vec3 {
    Vec3::new(ue_dir.x, ue_dir.z, -ue_dir.y)
}

/// Convert a direction vector or surface normal from SI (Y-Up RH) to UE (Z-Up LH).
/// Unit length is preserved: `(x, y, z) -> (x, -z, y)`.
#[inline]
pub fn metric_to_ue_direction(si_dir: Vec3) -> Vec3 {
    Vec3::new(si_dir.x, -si_dir.z, si_dir.y)
}

/// Scale-only conversion from Unreal Units to Meters without basis change.
#[inline]
pub fn ue_to_si_scale(scale: Vec3) -> Vec3 {
    scale * UU_TO_METERS
}

/// Scale-only conversion from Meters to Unreal Units without basis change.
#[inline]
pub fn si_to_ue_scale(scale: Vec3) -> Vec3 {
    scale * METERS_TO_UU
}

/// Rotation matrix that maps UE coordinates to SI coordinates:
/// `v_si = BASIS_UE_TO_SI * v_ue` where `(x, y, z) -> (x, z, -y)`.
pub const BASIS_UE_TO_SI: Mat3 = Mat3::from_cols(
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 0.0, -1.0),
    Vec3::new(0.0, 1.0, 0.0),
);

/// Rotation matrix that maps SI coordinates to UE coordinates:
/// `v_ue = BASIS_SI_TO_UE * v_si` where `(x, y, z) -> (x, -z, y)`.
pub const BASIS_SI_TO_UE: Mat3 = Mat3::from_cols(
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 0.0, 1.0),
    Vec3::new(0.0, -1.0, 0.0),
);

/// Convert a quaternion from Unreal Engine (Left-Handed Z-Up) to SI Metric (Right-Handed Y-Up).
///
/// Accounts for:
/// 1. Handedness reversal (LH -> RH): inverts rotation direction (conjugates axis).
/// 2. Coordinate basis transform: `(x, y, z) -> (x, z, -y)`.
pub fn ue_to_metric_quat(q_ue: Quat) -> Quat {
    // In UE Left-Handed, rotation matrix R transforms v: v' = R v.
    // For RH, rotation angle is reversed, so R_rh = R^T.
    // In transformed SI basis: R_si = M * R_rh * M^T = M * R^T * M^T.
    let r_ue = Mat3::from_quat(q_ue);
    // Invert rotation sense for handedness flip:
    let r_rh = r_ue.transpose();
    let r_si = BASIS_UE_TO_SI * r_rh * BASIS_SI_TO_UE;
    Quat::from_mat3(&r_si).normalize()
}

/// Convert a quaternion from SI Metric (Right-Handed Y-Up) to Unreal Engine (Left-Handed Z-Up).
pub fn metric_to_ue_quat(q_si: Quat) -> Quat {
    let r_si = Mat3::from_quat(q_si);
    let r_rh = BASIS_SI_TO_UE * r_si * BASIS_UE_TO_SI;
    let r_ue = r_rh.transpose();
    Quat::from_mat3(&r_ue).normalize()
}

/// Direct component mapping for Z-up Jolt configurations as defined in recon_04:
/// `Q_Jolt = (Qx, -Qy, Qz, Qw)`
#[inline]
pub fn ue_to_jolt_zup_quat(q: Quat) -> Quat {
    Quat::from_xyzw(q.x, -q.y, q.z, q.w)
}

/// Direct component mapping from Z-up Jolt to Unreal:
/// `Q_UE = (Qx, -Qy, Qz, Qw)`
#[inline]
pub fn jolt_zup_to_ue_quat(q: Quat) -> Quat {
    Quat::from_xyzw(q.x, -q.y, q.z, q.w)
}

/// Unreal Engine integer rotator (16-bit per axis, 65536 = 360 deg).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UnrealRotator {
    /// Pitch: Rotation around Y (Looking up/down). Positive is pitch up/down depending on convention.
    pub pitch: i32,
    /// Yaw: Rotation around Z (Facing heading).
    pub yaw: i32,
    /// Roll: Rotation around X (Banking).
    pub roll: i32,
}

impl UnrealRotator {
    pub const fn new(pitch: i32, yaw: i32, roll: i32) -> Self {
        Self { pitch, yaw, roll }
    }

    /// Convert to radians (Pitch, Yaw, Roll).
    pub fn to_radians(&self) -> (f32, f32, f32) {
        (
            (self.pitch as f32) * UU_ROTATOR_TO_RAD,
            (self.yaw as f32) * UU_ROTATOR_TO_RAD,
            (self.roll as f32) * UU_ROTATOR_TO_RAD,
        )
    }

    /// Construct from radians (pitch, yaw, roll).
    pub fn from_radians(pitch: f32, yaw: f32, roll: f32) -> Self {
        Self {
            pitch: (pitch * RAD_TO_UU_ROTATOR).round() as i32,
            yaw: (yaw * RAD_TO_UU_ROTATOR).round() as i32,
            roll: (roll * RAD_TO_UU_ROTATOR).round() as i32,
        }
    }

    /// Convert Unreal rotator to unit Quaternion (Yaw around Z, Pitch around Y, Roll around X).
    pub fn to_quat(&self) -> Quat {
        let (p, y, r) = self.to_radians();
        let q_yaw = Quat::from_rotation_z(y);
        let q_pitch = Quat::from_rotation_y(p);
        let q_roll = Quat::from_rotation_x(r);
        (q_yaw * q_pitch * q_roll).normalize()
    }

    /// Construct from Quaternion.
    pub fn from_quat(q: Quat) -> Self {
        let (roll, pitch, yaw) = q.to_euler(glam::EulerRot::XYZ);
        Self::from_radians(pitch, yaw, roll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_conversion_roundtrip() {
        let original_ue = Vec3::new(1250.0, -840.0, 320.0);
        let metric = ue_to_metric_position(original_ue);
        assert_eq!(metric.x, 1250.0 * 0.02);
        assert_eq!(metric.y, 320.0 * 0.02);
        assert_eq!(metric.z, -(-840.0 * 0.02));

        let recovered_ue = metric_to_ue_position(metric);
        assert!((recovered_ue - original_ue).length() < 1e-4);
    }

    #[test]
    fn test_exact_uu_scaling() {
        let pawn_height_uu = 88.0;
        let pawn_height_m = pawn_height_uu * UU_TO_METERS;
        assert!((pawn_height_m - 1.76).abs() < 1e-5);
    }

    #[test]
    fn test_quaternion_conversion() {
        let q_identity = Quat::IDENTITY;
        let q_si = ue_to_metric_quat(q_identity);
        let q_back = metric_to_ue_quat(q_si);
        assert!((q_back.dot(q_identity)).abs() > 0.999);
    }
}
