use glam::{Quat, Vec3};
use std::f32::consts::PI;

/// Conversion constants matching Unreal Engine 2.5 / Engine.dll specifications.
pub const UNITS_PER_ROTATION: f32 = 65536.0;
pub const UNITS_TO_DEGREES: f32 = 360.0 / UNITS_PER_ROTATION;
pub const DEGREES_TO_UNITS: f32 = UNITS_PER_ROTATION / 360.0;
pub const UNITS_TO_RADIANS: f32 = (2.0 * PI) / UNITS_PER_ROTATION;
pub const RADIANS_TO_UNITS: f32 = UNITS_PER_ROTATION / (2.0 * PI);

/// Unreal Engine 2.5 integer Rotator representation.
///
/// In retail Killing Floor:
/// - `pitch`: Look up/down (rotation around +Y)
/// - `yaw`: Turn left/right (rotation around +Z)
/// - `roll`: Bank/tilt (rotation around +X)
///
/// Coordinate system is Left-Handed Cartesian Z-Up:
/// - +X: Forward
/// - +Y: Right
/// - +Z: Up
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rotator {
    pub pitch: i32,
    pub yaw: i32,
    pub roll: i32,
}

impl Rotator {
    pub const ZERO: Self = Self {
        pitch: 0,
        yaw: 0,
        roll: 0,
    };

    #[inline]
    pub const fn new(pitch: i32, yaw: i32, roll: i32) -> Self {
        Self { pitch, yaw, roll }
    }

    /// Construct from degrees.
    #[inline]
    pub fn from_degrees(pitch_deg: f32, yaw_deg: f32, roll_deg: f32) -> Self {
        Self {
            pitch: (pitch_deg * DEGREES_TO_UNITS).round() as i32,
            yaw: (yaw_deg * DEGREES_TO_UNITS).round() as i32,
            roll: (roll_deg * DEGREES_TO_UNITS).round() as i32,
        }
    }

    /// Construct from radians.
    #[inline]
    pub fn from_radians(pitch_rad: f32, yaw_rad: f32, roll_rad: f32) -> Self {
        Self {
            pitch: (pitch_rad * RADIANS_TO_UNITS).round() as i32,
            yaw: (yaw_rad * RADIANS_TO_UNITS).round() as i32,
            roll: (roll_rad * RADIANS_TO_UNITS).round() as i32,
        }
    }

    /// Convert to (pitch, yaw, roll) in degrees.
    #[inline]
    pub fn to_degrees(&self) -> (f32, f32, f32) {
        (
            self.pitch as f32 * UNITS_TO_DEGREES,
            self.yaw as f32 * UNITS_TO_DEGREES,
            self.roll as f32 * UNITS_TO_DEGREES,
        )
    }

    /// Convert to (pitch, yaw, roll) in radians.
    #[inline]
    pub fn to_radians(&self) -> (f32, f32, f32) {
        (
            self.pitch as f32 * UNITS_TO_RADIANS,
            self.yaw as f32 * UNITS_TO_RADIANS,
            self.roll as f32 * UNITS_TO_RADIANS,
        )
    }

    /// Normalize rotator angles to [0, 65535].
    #[inline]
    pub fn normalize(&self) -> Self {
        Self {
            pitch: self.pitch.rem_euclid(65536),
            yaw: self.yaw.rem_euclid(65536),
            roll: self.roll.rem_euclid(65536),
        }
    }

    /// Convert to glam::Quat in Left-Handed Z-Up coordinates.
    /// In UE2: Yaw (around Z), then Pitch (around Y), then Roll (around X).
    pub fn to_quat(&self) -> Quat {
        let (p, y, r) = self.to_radians();
        let q_yaw = Quat::from_axis_angle(Vec3::Z, y);
        let q_pitch = Quat::from_axis_angle(Vec3::Y, p);
        let q_roll = Quat::from_axis_angle(Vec3::X, r);
        (q_yaw * q_pitch * q_roll).normalize()
    }

    /// Convert from glam::Quat to UE2 Rotator.
    pub fn from_quat(quat: Quat) -> Self {
        let q = quat.normalize();
        // Extract forward direction (+X rotated by q)
        let forward = q * Vec3::X;
        let right = q * Vec3::Y;

        let yaw_rad = forward.y.atan2(forward.x);
        let pitch_rad = (-forward.z).atan2((forward.x * forward.x + forward.y * forward.y).sqrt());
        
        // Projected right vector for roll
        let sin_p = pitch_rad.sin();
        let cos_p = pitch_rad.cos();
        let sin_y = yaw_rad.sin();
        let cos_y = yaw_rad.cos();

        let unrotated_right = Vec3::new(-sin_y, cos_y, 0.0);
        let unrotated_up = Vec3::new(-sin_p * cos_y, -sin_p * sin_y, cos_p);

        let roll_rad = right.dot(unrotated_up).atan2(right.dot(unrotated_right));

        Self::from_radians(pitch_rad, yaw_rad, roll_rad).normalize()
    }

    /// Calculate the forward direction vector (+X in Left-Handed Z-Up).
    pub fn to_vector(&self) -> Vec3 {
        let (p, y, _) = self.to_radians();
        let cp = p.cos();
        let sp = p.sin();
        let cy = y.cos();
        let sy = y.sin();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    /// Calculate directional axes: (Forward, Right, Up).
    pub fn get_axes(&self) -> (Vec3, Vec3, Vec3) {
        let q = self.to_quat();
        let forward = q * Vec3::X;
        let right = q * Vec3::Y;
        let up = q * Vec3::Z;
        (forward, right, up)
    }
}
