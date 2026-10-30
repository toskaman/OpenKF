//! 3D Spatial Audio mathematics and acoustics modeling matching OpenAL 1.1 and Unreal Engine 2 `ALAudio.dll`.
//!
//! Provides:
//! - OpenAL 1.1 Distance Attenuation models (`InverseDistanceClamped`, `LinearDistanceClamped`, etc.)
//! - OpenAL 1.1 Doppler effect frequency shift calculation
//! - Listener orientation, position, and velocity tracking
//! - Equal-power stereo spatial panning

use glam::Vec3;
use std::f32::consts::{FRAC_PI_4, SQRT_2};

/// Unreal Units to Meters conversion factor in Killing Floor (50 UU = 1 meter).
pub const UU_PER_METER: f32 = 50.0;

/// Standard Speed of Sound in Unreal Units per second (343.0 m/s * 50 UU/m = 17,150.0 UU/s).
pub const DEFAULT_SPEED_OF_SOUND_UU: f32 = 17_150.0;

/// Default sound radius multiplier used in UE2 `ALAudio.dll` (SoundRadius * 40.0 = R_ref).
pub const UE2_SOUND_RADIUS_MULTIPLIER: f32 = 40.0;

/// OpenAL 1.1 distance attenuation models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DistanceModel {
    /// OpenAL 1.1 `AL_INVERSE_DISTANCE_CLAMPED` (Default used by Killing Floor / UE2).
    /// Gain(d) = Clamp(R_ref / (R_ref + Rolloff * (Clamp(d, R_ref, R_max) - R_ref)), min_gain, max_gain)
    #[default]
    InverseDistanceClamped,
    /// OpenAL 1.1 `AL_INVERSE_DISTANCE` (Unclamped at max distance).
    InverseDistance,
    /// OpenAL 1.1 `AL_LINEAR_DISTANCE_CLAMPED`.
    LinearDistanceClamped,
    /// OpenAL 1.1 `AL_EXPONENT_DISTANCE_CLAMPED`.
    ExponentDistanceClamped,
    /// No attenuation (full volume regardless of distance).
    None,
}

/// Computes the attenuation gain factor for a given distance according to OpenAL 1.1 distance models.
///
/// # Arguments
/// * `distance` - Euclidean distance between emitter and listener in Unreal Units.
/// * `ref_distance` - Reference distance where gain is 1.0 (SoundRadius * 40.0).
/// * `max_distance` - Distance threshold beyond which attenuation ceases or drops to zero.
/// * `rolloff` - Rolloff factor (typically 1.0).
/// * `min_gain` - Minimum clamp value for gain.
/// * `max_gain` - Maximum clamp value for gain.
/// * `model` - The distance attenuation model to evaluate.
pub fn calculate_distance_attenuation(
    distance: f32,
    ref_distance: f32,
    max_distance: f32,
    rolloff: f32,
    min_gain: f32,
    max_gain: f32,
    model: DistanceModel,
) -> f32 {
    let ref_dist = ref_distance.max(0.001);
    let max_dist = max_distance.max(ref_dist);

    let raw_gain = match model {
        DistanceModel::InverseDistanceClamped => {
            let clamped_dist = distance.clamp(ref_dist, max_dist);
            ref_dist / (ref_dist + rolloff * (clamped_dist - ref_dist))
        }
        DistanceModel::InverseDistance => {
            let effective_dist = distance.max(ref_dist);
            ref_dist / (ref_dist + rolloff * (effective_dist - ref_dist))
        }
        DistanceModel::LinearDistanceClamped => {
            let clamped_dist = distance.clamp(ref_dist, max_dist);
            if max_dist <= ref_dist {
                1.0
            } else {
                1.0 - rolloff * (clamped_dist - ref_dist) / (max_dist - ref_dist)
            }
        }
        DistanceModel::ExponentDistanceClamped => {
            let clamped_dist = distance.clamp(ref_dist, max_dist);
            (clamped_dist / ref_dist).powf(-rolloff)
        }
        DistanceModel::None => 1.0,
    };

    raw_gain.clamp(min_gain, max_gain)
}

/// Calculates the Doppler frequency shift multiplier matching OpenAL 1.1 specification:
///
/// ```text
/// SL = P_listener - P_source
/// v_ss = (SL . v_source) / |SL|
/// v_ls = (SL . v_listener) / |SL|
/// v_ss = min(v_ss, (c * 0.999) / D)
/// v_ls = min(v_ls, (c * 0.999) / D)
/// f' / f = (c - D * v_ls) / (c - D * v_ss)
/// ```
pub fn calculate_doppler_shift(
    listener_pos: Vec3,
    listener_vel: Vec3,
    emitter_pos: Vec3,
    emitter_vel: Vec3,
    doppler_factor: f32,
    speed_of_sound: f32,
) -> f32 {
    let sl = listener_pos - emitter_pos;
    let dist = sl.length();

    // If source and listener are virtually co-located, no Doppler shift occurs.
    if dist < 0.001 || doppler_factor <= 0.0 || speed_of_sound <= 0.0 {
        return 1.0;
    }

    let dir = sl / dist;
    let mut v_ss = emitter_vel.dot(dir);
    let mut v_ls = listener_vel.dot(dir);

    // Safety clamping according to OpenAL 1.1 to avoid division by zero or negative pitch
    let max_v = (speed_of_sound * 0.999) / doppler_factor;
    let min_v = -max_v * 5.0; // avoid absurd backwards pitch

    v_ss = v_ss.clamp(min_v, max_v);
    v_ls = v_ls.clamp(min_v, max_v);

    let numerator = speed_of_sound - doppler_factor * v_ls;
    let denominator = speed_of_sound - doppler_factor * v_ss;

    if denominator.abs() < 1e-4 {
        return 1.0;
    }

    let shift = numerator / denominator;
    // Clamp to realistic audible pitch limits [0.05, 10.0]
    shift.clamp(0.05, 10.0)
}

/// Represents the virtual ears of the player in 3D world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioListener {
    /// World position in Unreal Units.
    pub position: Vec3,
    /// Forward facing direction vector (normalized).
    pub forward: Vec3,
    /// Up direction vector (normalized).
    pub up: Vec3,
    /// Velocity vector in Unreal Units per second.
    pub velocity: Vec3,
}

impl Default for AudioListener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::Y, // UE2 coordinate standard: Y forward or X forward
            up: Vec3::Z,      // UE2 Z is vertical up
            velocity: Vec3::ZERO,
        }
    }
}

impl AudioListener {
    /// Constructs a new listener with explicit position and orientations.
    pub fn new(position: Vec3, forward: Vec3, up: Vec3, velocity: Vec3) -> Self {
        let f = forward.normalize_or_zero();
        let u = up.normalize_or_zero();
        Self {
            position,
            forward: if f.length_squared() > 0.0 { f } else { Vec3::Y },
            up: if u.length_squared() > 0.0 { u } else { Vec3::Z },
            velocity,
        }
    }

    /// Computes the listener's right vector (Forward x Up).
    #[inline]
    pub fn right(&self) -> Vec3 {
        self.forward.cross(self.up).normalize_or_zero()
    }
}

/// Calculates stereo equal-power spatial panning gains `(left_gain, right_gain)`.
///
/// Center sound produces `(1.0, 1.0)` unity gains.
/// Hard-left produces `(~1.414, 0.0)`.
/// Hard-right produces `(0.0, ~1.414)`.
pub fn calculate_spatial_panning(listener: &AudioListener, emitter_pos: Vec3) -> (f32, f32) {
    let to_emitter = emitter_pos - listener.position;
    let dist = to_emitter.length();

    if dist < 0.001 {
        return (1.0, 1.0);
    }

    let dir = to_emitter / dist;
    let right_vec = listener.right();

    // Panning azimuth: dot product with listener's right axis [-1.0 .. 1.0]
    let pan = dir.dot(right_vec).clamp(-1.0, 1.0);

    // Equal-power sinusoidal panning: theta ranges from 0 (hard left) to pi/2 (hard right)
    let theta = (pan + 1.0) * FRAC_PI_4;
    let left_gain = theta.cos() * SQRT_2;
    let right_gain = theta.sin() * SQRT_2;

    (left_gain.max(0.0), right_gain.max(0.0))
}
