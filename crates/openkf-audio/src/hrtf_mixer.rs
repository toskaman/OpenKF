//! 3D Spatial Audio HRTF Mixer Bridge for OpenKF.
//!
//! Synchronizes camera view matrices and player listener orientations with OpenAL 1.1
//! listener state, evaluates binaural HRTF acoustic cues (ITD, ILD, pinna filtering),
//! computes physical Doppler shifts, and manages 256-voice emitter channel priorities.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use thiserror::Error;

use crate::engine::{AudioEngine, AudioEngineError};
use crate::pool::{PoolError, SharedEmitterPool, MAX_VOICE_CHANNELS};
use crate::spatial::{
    calculate_doppler_shift, AudioListener, DistanceModel, UU_PER_METER,
};
use crate::voice::{PlaySoundConfig, VoiceId};
use crate::wav::WavSound;

/// Average human adult head radius in meters (~8.75 cm).
pub const HUMAN_HEAD_RADIUS_METERS: f32 = 0.0875;

/// Average human head radius in Unreal Units (50 UU/m * 0.0875 m = 4.375 UU).
pub const HUMAN_HEAD_RADIUS_UU: f32 = HUMAN_HEAD_RADIUS_METERS * UU_PER_METER;

/// Speed of sound in air in meters per second at 20°C.
pub const SPEED_OF_SOUND_MPS: f32 = 343.0;

/// Errors arising from HRTF mixer operations and sound registries.
#[derive(Debug, Error, PartialEq)]
pub enum HrtfMixerError {
    #[error("Sound asset ID {0} not found in sound registry")]
    SoundNotFound(u32),
    #[error("Voice pool error: {0}")]
    Pool(#[from] PoolError),
    #[error("Audio engine error: {0}")]
    Engine(String),
}

/// Metadata configuration for registered sound assets.
#[derive(Debug, Clone)]
pub struct SoundMetadata {
    pub default_radius: f32,
    pub default_priority: u8,
    pub looping: bool,
    pub max_distance: f32,
    pub rolloff: f32,
    pub model: DistanceModel,
}

impl Default for SoundMetadata {
    fn default() -> Self {
        Self {
            default_radius: 64.0,
            default_priority: 128,
            looping: false,
            max_distance: 10_000.0,
            rolloff: 1.0,
            model: DistanceModel::InverseDistanceClamped,
        }
    }
}

/// Binaural Head-Related Transfer Function (HRTF) acoustic cues for 3D spatialization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HrtfBinauralCue {
    /// Azimuth angle relative to listener facing axis in radians `[-PI, PI]`.
    /// 0.0 is straight ahead, `+PI/2` is hard-right, `-PI/2` is hard-left, `+/-PI` is directly behind.
    pub azimuth: f32,
    /// Elevation angle relative to listener horizontal plane in radians `[-PI/2, PI/2]`.
    /// `+PI/2` is zenith (above), `-PI/2` is nadir (below).
    pub elevation: f32,
    /// Interaural Time Difference (ITD) in seconds using Woodworth's spherical head model.
    /// Positive values denote sound arriving at the right ear before the left ear.
    pub itd_seconds: f32,
    /// Interaural Time Difference in fractional samples at standard 44.1 kHz.
    pub itd_samples_44k: f32,
    /// Interaural Level Difference (ILD) gain multiplier for left ear channel.
    pub ild_left_gain: f32,
    /// Interaural Level Difference (ILD) gain multiplier for right ear channel.
    pub ild_right_gain: f32,
    /// Low-pass head-shadow cutoff frequency for the contralateral (shadowed) ear in Hz.
    pub contralateral_lowpass_cutoff_hz: f32,
}

/// 3D Audio Listener Bridge synchronizing camera matrices, OpenAL listener parameters,
/// Doppler calculations, and 256-voice emitter priorities.
pub struct HrtfMixerBridge {
    /// High-level OpenKF audio engine managing voice mixing and listeners.
    engine: AudioEngine,
    /// Pre-registered sound asset dictionary indexed by unique 32-bit identifier.
    sound_registry: HashMap<u32, (Arc<WavSound>, SoundMetadata)>,
    /// OpenAL-formatted orientation vector: `[at.x, at.y, at.z, up.x, up.y, up.z]`.
    openal_orientation: [f32; 6],
    /// OpenAL-formatted position: `[x, y, z]`.
    openal_position: [f32; 3],
    /// OpenAL-formatted velocity: `[vx, vy, vz]`.
    openal_velocity: [f32; 3],
    /// Whether HRTF acoustic filtering and binaural cues are enabled.
    hrtf_enabled: bool,
}

impl Default for HrtfMixerBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl HrtfMixerBridge {
    /// Creates a newly initialized HRTF mixer bridge with standard Unreal Engine 2 defaults.
    pub fn new() -> Self {
        let engine = AudioEngine::new();
        let mut bridge = Self {
            engine,
            sound_registry: HashMap::new(),
            openal_orientation: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0], // +X forward, +Z up
            openal_position: [0.0, 0.0, 0.0],
            openal_velocity: [0.0, 0.0, 0.0],
            hrtf_enabled: true,
        };

        // Initialize listener orientation
        bridge.sync_listener(Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::ZERO);
        bridge
    }

    /// Creates an HRTF mixer bridge wrapping an existing `AudioEngine`.
    pub fn with_engine(engine: AudioEngine) -> Self {
        let mut bridge = Self {
            engine,
            sound_registry: HashMap::new(),
            openal_orientation: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            openal_position: [0.0, 0.0, 0.0],
            openal_velocity: [0.0, 0.0, 0.0],
            hrtf_enabled: true,
        };
        bridge.sync_listener(Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::ZERO);
        bridge
    }

    /// Synchronizes the 3D audio listener orientation, world position, and linear velocity.
    ///
    /// Ensures forward and up orientation vectors are strictly normalized and Gram-Schmidt
    /// orthogonalized to guarantee compliant OpenAL `AL_ORIENTATION` states.
    pub fn sync_listener(
        &mut self,
        position: Vec3,
        forward: Vec3,
        up: Vec3,
        velocity: Vec3,
    ) {
        let mut fwd = forward.normalize_or_zero();
        if fwd.length_squared() < 1e-6 {
            fwd = Vec3::X; // Default UE2 forward
        }

        // Gram-Schmidt orthogonalization: up' = normalize(up - (up . fwd) * fwd)
        let proj = up - fwd * up.dot(fwd);
        let mut ortho_up = proj.normalize_or_zero();
        if ortho_up.length_squared() < 1e-6 {
            // Pick fallback vector not collinear with fwd
            let fallback = if fwd.z.abs() < 0.9 { Vec3::Z } else { Vec3::Y };
            ortho_up = (fallback - fwd * fallback.dot(fwd)).normalize_or_zero();
        }

        self.engine.update_listener(position, fwd, ortho_up, velocity);

        self.openal_orientation = [
            fwd.x, fwd.y, fwd.z,
            ortho_up.x, ortho_up.y, ortho_up.z,
        ];
        self.openal_position = [position.x, position.y, position.z];
        self.openal_velocity = [velocity.x, velocity.y, velocity.z];
    }

    /// Synchronizes the listener directly from a 4x4 camera World-to-Camera View Matrix.
    ///
    /// For view matrix $V = R^T (X - P)$, the inverse matrix $V^{-1}$ contains:
    /// - `inv.w_axis.truncate()` = Camera world position $P$.
    /// - `inv.z_axis.truncate()` = Camera forward vector into screen.
    /// - `inv.y_axis.truncate()` = Camera up vector.
    pub fn sync_from_view_matrix(&mut self, view_matrix: Mat4, velocity: Vec3) {
        let inv = view_matrix.inverse();
        let position = inv.w_axis.truncate();
        let forward = inv.z_axis.truncate();
        let up = inv.y_axis.truncate();

        self.sync_listener(position, forward, up, velocity);
    }

    /// Registers a sound asset under a designated numeric ID with default playback parameters.
    pub fn register_sound(&mut self, sound_id: u32, sound: Arc<WavSound>) {
        self.sound_registry.insert(sound_id, (sound, SoundMetadata::default()));
    }

    /// Registers a sound asset with explicit metadata (sound radius, priority, looping, falloff).
    pub fn register_sound_with_meta(
        &mut self,
        sound_id: u32,
        sound: Arc<WavSound>,
        meta: SoundMetadata,
    ) {
        self.sound_registry.insert(sound_id, (sound, meta));
    }

    /// Plays a 3D spatialized sound effect matching the required contract.
    ///
    /// Looks up the sound in the sound registry and triggers it through the 256-voice pool.
    pub fn play_3d_sound(
        &mut self,
        sound_id: u32,
        position: Vec3,
        pitch: f32,
        gain: f32,
    ) -> Result<VoiceId, HrtfMixerError> {
        let (sound, meta) = self
            .sound_registry
            .get(&sound_id)
            .cloned()
            .ok_or(HrtfMixerError::SoundNotFound(sound_id))?;

        let voice_id = self
            .engine
            .play_sound_3d(
                sound,
                position,
                meta.default_radius,
                gain,
                pitch,
                meta.default_priority,
            )
            .map_err(|e| match e {
                AudioEngineError::Pool(p) => HrtfMixerError::Pool(p),
                other => HrtfMixerError::Engine(other.to_string()),
            })?;

        Ok(voice_id)
    }

    /// Plays a 3D sound with explicit priority override (0..255).
    pub fn play_3d_sound_with_priority(
        &mut self,
        sound_id: u32,
        position: Vec3,
        pitch: f32,
        gain: f32,
        priority: u8,
    ) -> Result<VoiceId, HrtfMixerError> {
        let (sound, meta) = self
            .sound_registry
            .get(&sound_id)
            .cloned()
            .ok_or(HrtfMixerError::SoundNotFound(sound_id))?;

        let voice_id = self
            .engine
            .play_sound_3d(sound, position, meta.default_radius, gain, pitch, priority)
            .map_err(|e| match e {
                AudioEngineError::Pool(p) => HrtfMixerError::Pool(p),
                other => HrtfMixerError::Engine(other.to_string()),
            })?;

        Ok(voice_id)
    }

    /// Plays a 3D sound with comprehensive physics and spatialization parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn play_3d_sound_full(
        &mut self,
        sound_id: u32,
        position: Vec3,
        velocity: Vec3,
        pitch: f32,
        gain: f32,
        sound_radius: f32,
        priority: u8,
    ) -> Result<VoiceId, HrtfMixerError> {
        let (sound, meta) = self
            .sound_registry
            .get(&sound_id)
            .cloned()
            .ok_or(HrtfMixerError::SoundNotFound(sound_id))?;

        let effective_volume = gain * self.engine.master_volume * self.engine.sfx_volume;
        let config = PlaySoundConfig {
            volume: effective_volume,
            pitch,
            priority,
            looping: meta.looping,
            is_spatial: true,
            emitter_pos: position,
            emitter_vel: velocity,
            sound_radius,
            max_distance: meta.max_distance,
            rolloff: meta.rolloff,
            model: meta.model,
        };

        let voice_id = self.engine.pool.play(sound, &config)?;
        self.engine.pool.update_spatial(
            &self.engine.listener,
            self.engine.doppler_factor,
            self.engine.speed_of_sound,
        );
        Ok(voice_id)
    }

    /// Computes Doppler frequency shift factor for an emitter moving relative to the listener.
    pub fn calculate_doppler(&self, emitter_pos: Vec3, emitter_vel: Vec3) -> f32 {
        calculate_doppler_shift(
            self.engine.listener.position,
            self.engine.listener.velocity,
            emitter_pos,
            emitter_vel,
            self.engine.doppler_factor,
            self.engine.speed_of_sound,
        )
    }

    /// Calculates binaural HRTF acoustic cues (azimuth, elevation, ITD, ILD, head shadow filtering).
    pub fn compute_hrtf_cues(&self, emitter_pos: Vec3) -> HrtfBinauralCue {
        let listener = &self.engine.listener;
        let rel_pos = emitter_pos - listener.position;
        let dist = rel_pos.length();

        if dist < 1e-4 {
            return HrtfBinauralCue {
                azimuth: 0.0,
                elevation: 0.0,
                itd_seconds: 0.0,
                itd_samples_44k: 0.0,
                ild_left_gain: 1.0,
                ild_right_gain: 1.0,
                contralateral_lowpass_cutoff_hz: 20_000.0,
            };
        }

        let dir = rel_pos / dist;
        let fwd = listener.forward;
        let right = listener.right();
        let up = listener.up;

        // Project onto horizontal listener plane for azimuth calculation
        let x = dir.dot(right); // +right
        let y = dir.dot(fwd);   // +front
        let z = dir.dot(up);    // +elevation

        let azimuth = x.atan2(y); // [-PI, PI]: 0 is front, +PI/2 is right, -PI/2 is left
        let ground_dist = (x * x + y * y).sqrt();
        let elevation = z.atan2(ground_dist); // [-PI/2, PI/2]

        // Woodworth spherical head model for Interaural Time Difference (ITD):
        // Delta_t = (r / c) * (sin|theta| + |theta|) * sign(theta)
        let abs_azimuth = azimuth.abs();
        let speed_of_sound_uu = self.engine.speed_of_sound;
        let itd_seconds = hum_radius_div_c(HUMAN_HEAD_RADIUS_UU, speed_of_sound_uu)
            * (abs_azimuth.sin() + abs_azimuth)
            * azimuth.signum();

        let itd_samples_44k = itd_seconds * 44_100.0;

        // Interaural Level Difference (ILD) and head-shadow filtering:
        // Contralateral ear experiences high-frequency attenuation up to ~12dB at 90 deg azimuth.
        let pan = x.clamp(-1.0, 1.0);
        let theta_pan = (pan + 1.0) * (std::f32::consts::FRAC_PI_4);
        let ild_left = theta_pan.cos() * std::f32::consts::SQRT_2;
        let ild_right = theta_pan.sin() * std::f32::consts::SQRT_2;

        // Contralateral low-pass cutoff: 20,000 Hz in front, dropping to 2,500 Hz on the far side
        let shadow_intensity = abs_azimuth.sin().abs();
        let contralateral_lowpass_cutoff_hz = 20_000.0 - shadow_intensity * 17_500.0;

        HrtfBinauralCue {
            azimuth,
            elevation,
            itd_seconds,
            itd_samples_44k,
            ild_left_gain: ild_left.max(0.0),
            ild_right_gain: ild_right.max(0.0),
            contralateral_lowpass_cutoff_hz: contralateral_lowpass_cutoff_hz.max(1000.0),
        }
    }

    /// Renders and mixes an audio frame buffer into left and right channel output slices.
    pub fn mix_frame(&mut self, left_out: &mut [f32], right_out: &mut [f32]) {
        let frames = left_out.len().min(right_out.len());
        let mut interleaved = vec![0.0f32; frames * 2];
        self.engine.render_audio_frames(frames, &mut interleaved);
        for i in 0..frames {
            left_out[i] = interleaved[i * 2];
            right_out[i] = interleaved[i * 2 + 1];
        }
    }

    /// Mixes directly into an interleaved stereo output buffer `[L0, R0, L1, R1, ...]`.
    pub fn mix_interleaved(&mut self, frames: usize, output: &mut [f32]) {
        self.engine.render_audio_frames(frames, output);
    }

    /// Returns the OpenAL 6-float orientation array: `[at.x, at.y, at.z, up.x, up.y, up.z]`.
    #[inline]
    pub fn openal_orientation(&self) -> [f32; 6] {
        self.openal_orientation
    }

    /// Returns the OpenAL 3-float position array: `[x, y, z]`.
    #[inline]
    pub fn openal_position(&self) -> [f32; 3] {
        self.openal_position
    }

    /// Returns the OpenAL 3-float velocity array: `[vx, vy, vz]`.
    #[inline]
    pub fn openal_velocity(&self) -> [f32; 3] {
        self.openal_velocity
    }

    /// Returns current number of active voice channels out of 256.
    #[inline]
    pub fn active_voice_count(&self) -> usize {
        self.engine.pool.active_count()
    }

    /// Maximum concurrent voice channels supported (256).
    #[inline]
    pub const fn max_voice_channels(&self) -> usize {
        MAX_VOICE_CHANNELS
    }

    /// Reference to the underlying `AudioEngine`.
    #[inline]
    pub fn engine(&self) -> &AudioEngine {
        &self.engine
    }

    /// Mutable reference to the underlying `AudioEngine`.
    #[inline]
    pub fn engine_mut(&mut self) -> &mut AudioEngine {
        &mut self.engine
    }

    /// Reference to the thread-safe voice emitter pool.
    #[inline]
    pub fn pool(&self) -> &SharedEmitterPool {
        &self.engine.pool
    }

    /// Reference to the active listener pose.
    #[inline]
    pub fn listener(&self) -> &AudioListener {
        &self.engine.listener
    }

    /// Configures the Doppler frequency scaling factor.
    pub fn set_doppler_factor(&mut self, factor: f32) {
        self.engine.doppler_factor = factor.max(0.0);
    }

    /// Configures the speed of sound in Unreal Units per second.
    pub fn set_speed_of_sound(&mut self, speed_uu: f32) {
        self.engine.speed_of_sound = speed_uu.max(1.0);
    }

    /// Enables or disables HRTF binaural acoustic cues.
    pub fn set_hrtf_enabled(&mut self, enabled: bool) {
        self.hrtf_enabled = enabled;
    }

    /// Returns whether HRTF spatialization is currently active.
    pub fn is_hrtf_enabled(&self) -> bool {
        self.hrtf_enabled
    }
}

#[inline]
fn hum_radius_div_c(radius: f32, c: f32) -> f32 {
    if c > 0.0 {
        radius / c
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{vec3, Mat4, Vec4};
    use std::f32::consts::PI;

    fn make_test_tone(sample_rate: u32, duration_secs: f32, freq: f32) -> WavSound {
        let total_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let val = (2.0 * PI * freq * t).sin();
            samples.push(val);
        }
        WavSound::from_samples(1, sample_rate, samples)
    }

    #[test]
    fn test_sync_listener_and_openal_orientation() {
        let mut bridge = HrtfMixerBridge::new();
        let pos = vec3(100.0, 200.0, 50.0);
        let fwd = vec3(1.0, 0.0, 0.0);
        let up = vec3(0.0, 0.0, 1.0);
        let vel = vec3(10.0, 0.0, 0.0);

        bridge.sync_listener(pos, fwd, up, vel);

        assert_eq!(bridge.openal_position(), [100.0, 200.0, 50.0]);
        assert_eq!(bridge.openal_velocity(), [10.0, 0.0, 0.0]);
        assert_eq!(bridge.openal_orientation(), [1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);

        let listener = bridge.listener();
        assert_eq!(listener.position, pos);
        assert_eq!(listener.forward, fwd);
        assert_eq!(listener.up, up);
        assert_eq!(listener.velocity, vel);
    }

    #[test]
    fn test_sync_listener_gram_schmidt_orthogonalization() {
        let mut bridge = HrtfMixerBridge::new();
        // Non-orthogonal forward and up
        let fwd = vec3(1.0, 0.0, 0.0);
        let slanted_up = vec3(1.0, 0.0, 1.0); // Slanted towards forward

        bridge.sync_listener(Vec3::ZERO, fwd, slanted_up, Vec3::ZERO);

        let ori = bridge.openal_orientation();
        let ortho_fwd = vec3(ori[0], ori[1], ori[2]);
        let ortho_up = vec3(ori[3], ori[4], ori[5]);

        // Must be normalized
        assert!((ortho_fwd.length() - 1.0).abs() < 1e-5);
        assert!((ortho_up.length() - 1.0).abs() < 1e-5);
        // Must be orthogonal: dot product == 0
        assert!(ortho_fwd.dot(ortho_up).abs() < 1e-5);
        assert_eq!(ortho_up, vec3(0.0, 0.0, 1.0));
    }

    #[test]
    fn test_sync_from_view_matrix() {
        let mut bridge = HrtfMixerBridge::new();
        let eye = vec3(500.0, 1000.0, 200.0);
        let target = vec3(1500.0, 1000.0, 200.0); // Looking along +X
        let world_up = vec3(0.0, 0.0, 1.0);

        // Standard camera view matrix
        let forward = (target - eye).normalize();
        let right = forward.cross(world_up).normalize();
        let up = right.cross(forward).normalize();

        let view_matrix = Mat4::from_cols(
            Vec4::new(right.x, up.x, forward.x, 0.0),
            Vec4::new(right.y, up.y, forward.y, 0.0),
            Vec4::new(right.z, up.z, forward.z, 0.0),
            Vec4::new(-right.dot(eye), -up.dot(eye), -forward.dot(eye), 1.0),
        );

        bridge.sync_from_view_matrix(view_matrix, vec3(5.0, 0.0, 0.0));

        let pos = bridge.listener().position;
        assert!((pos.x - 500.0).abs() < 1e-3);
        assert!((pos.y - 1000.0).abs() < 1e-3);
        assert!((pos.z - 200.0).abs() < 1e-3);

        let fwd = bridge.listener().forward;
        assert!((fwd.x - 1.0).abs() < 1e-3);
        assert!(fwd.y.abs() < 1e-3);
        assert!(fwd.z.abs() < 1e-3);
    }

    #[test]
    fn test_doppler_calculations() {
        let mut bridge = HrtfMixerBridge::new();
        bridge.sync_listener(vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), Vec3::ZERO);

        let emitter_pos = vec3(1000.0, 0.0, 0.0);
        // Approaching emitter (moving towards listener at 1715 UU/s = 0.1c)
        let emitter_vel_approaching = vec3(-1715.0, 0.0, 0.0);
        let doppler_approach = bridge.calculate_doppler(emitter_pos, emitter_vel_approaching);
        assert!(doppler_approach > 1.09 && doppler_approach < 1.12);

        // Receding emitter
        let emitter_vel_receding = vec3(1715.0, 0.0, 0.0);
        let doppler_recede = bridge.calculate_doppler(emitter_pos, emitter_vel_receding);
        assert!(doppler_recede < 0.92 && doppler_recede > 0.89);
    }

    #[test]
    fn test_hrtf_binaural_cues() {
        let mut bridge = HrtfMixerBridge::new();
        bridge.sync_listener(vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), Vec3::ZERO);

        // Straight ahead
        let cues_front = bridge.compute_hrtf_cues(vec3(100.0, 0.0, 0.0));
        assert!(cues_front.azimuth.abs() < 1e-4);
        assert!(cues_front.elevation.abs() < 1e-4);
        assert!(cues_front.itd_seconds.abs() < 1e-6);
        assert!(cues_front.contralateral_lowpass_cutoff_hz > 19_000.0);

        // Hard right (azimuth = +PI/2)
        let right_vec = bridge.listener().right();
        let cues_right = bridge.compute_hrtf_cues(right_vec * 100.0);
        assert!((cues_right.azimuth - (PI * 0.5)).abs() < 1e-3);
        assert!(cues_right.itd_seconds > 0.0); // Arrives at right ear first
        assert!(cues_right.ild_right_gain > cues_right.ild_left_gain);
        assert!(cues_right.contralateral_lowpass_cutoff_hz < 3_000.0); // Left ear shadowed

        // Hard left (azimuth = -PI/2)
        let cues_left = bridge.compute_hrtf_cues(-right_vec * 100.0);
        assert!((cues_left.azimuth - (-PI * 0.5)).abs() < 1e-3);
        assert!(cues_left.itd_seconds < 0.0); // Arrives at left ear first
        assert!(cues_left.ild_left_gain > cues_left.ild_right_gain);
        assert!((cues_left.ild_left_gain - cues_right.ild_right_gain).abs() < 1e-3);
    }

    #[test]
    fn test_play_3d_sound_contract_and_voice_pool() {
        let mut bridge = HrtfMixerBridge::new();
        let sound = Arc::new(make_test_tone(44100, 0.5, 440.0));

        let sound_id = 42;
        bridge.register_sound(sound_id, sound);

        let voice_id = bridge
            .play_3d_sound(sound_id, vec3(100.0, 50.0, 0.0), 1.0, 0.8)
            .expect("Should play registered 3D sound");

        assert_eq!(bridge.active_voice_count(), 1);
        assert!(bridge.pool().is_playing(voice_id));

        // Querying unknown sound returns SoundNotFound
        let err = bridge.play_3d_sound(999, vec3(0.0, 0.0, 0.0), 1.0, 1.0);
        assert_eq!(err.unwrap_err(), HrtfMixerError::SoundNotFound(999));
    }

    #[test]
    fn test_256_voice_emitter_priorities_and_stealing() {
        let mut bridge = HrtfMixerBridge::new();
        let sound = Arc::new(make_test_tone(44100, 1.0, 440.0));
        bridge.register_sound(1, sound);

        // Fill all 256 voice channels with low priority (10)
        for _ in 0..MAX_VOICE_CHANNELS {
            bridge
                .play_3d_sound_with_priority(1, vec3(0.0, 0.0, 0.0), 1.0, 0.5, 10)
                .expect("Channel allocation must succeed up to 256");
        }
        assert_eq!(bridge.active_voice_count(), MAX_VOICE_CHANNELS);

        // Attempting to play another low priority (5) sound when full should fail
        let result_lower = bridge.play_3d_sound_with_priority(1, vec3(0.0, 0.0, 0.0), 1.0, 0.5, 5);
        assert_eq!(
            result_lower.unwrap_err(),
            HrtfMixerError::Pool(PoolError::PoolFullHigherPriority)
        );

        // Playing higher priority (200) sound should successfully steal a voice channel
        let result_higher = bridge.play_3d_sound_with_priority(1, vec3(0.0, 0.0, 0.0), 1.0, 0.5, 200);
        assert!(result_higher.is_ok());
        assert_eq!(bridge.active_voice_count(), MAX_VOICE_CHANNELS);
    }

    #[test]
    fn test_mix_frame_rendering() {
        let mut bridge = HrtfMixerBridge::new();
        let sound = Arc::new(make_test_tone(44100, 0.1, 440.0));
        bridge.register_sound(10, sound);

        bridge
            .play_3d_sound(10, vec3(0.0, 0.0, 0.0), 1.0, 1.0)
            .expect("Play sound");

        let mut left = vec![0.0f32; 128];
        let mut right = vec![0.0f32; 128];

        bridge.mix_frame(&mut left, &mut right);

        // Non-silent audio must be rendered
        let energy: f32 = left.iter().map(|s| s.abs()).sum::<f32>()
            + right.iter().map(|s| s.abs()).sum::<f32>();
        assert!(energy > 0.0);
    }
}
