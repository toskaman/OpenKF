//! High-level Audio Engine subsystem matching Unreal Engine 2 `UALAudioSubsystem`.

use std::sync::Arc;
use glam::Vec3;
use thiserror::Error;

use crate::pool::{PoolError, SharedEmitterPool};
use crate::spatial::{AudioListener, DistanceModel, DEFAULT_SPEED_OF_SOUND_UU};
use crate::voice::{PlaySoundConfig, VoiceId};
use crate::wav::WavSound;

#[derive(Debug, Error)]
pub enum AudioEngineError {
    #[error("Voice pool error: {0}")]
    Pool(#[from] PoolError),
    #[error("Invalid sound asset provided")]
    InvalidSound,
}

/// Main entry point for the OpenKF Audio Subsystem.
pub struct AudioEngine {
    /// Virtual listener (player's head and ears in 3D space).
    pub listener: AudioListener,
    /// Thread-safe voice emitter pool.
    pub pool: SharedEmitterPool,
    /// Master volume scale [0.0 .. 1.0].
    pub master_volume: f32,
    /// Sound effects volume scale [0.0 .. 1.0].
    pub sfx_volume: f32,
    /// Music stream volume scale [0.0 .. 1.0].
    pub music_volume: f32,
    /// Doppler scale factor (default 1.0).
    pub doppler_factor: f32,
    /// Speed of sound in Unreal Units per second (default 17,150.0).
    pub speed_of_sound: f32,
    /// Distance attenuation model.
    pub distance_model: DistanceModel,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    /// Creates a newly initialized audio engine.
    pub fn new() -> Self {
        Self {
            listener: AudioListener::default(),
            pool: SharedEmitterPool::new(),
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 1.0,
            doppler_factor: 1.0,
            speed_of_sound: DEFAULT_SPEED_OF_SOUND_UU,
            distance_model: DistanceModel::InverseDistanceClamped,
        }
    }

    /// Updates the listener's 3D pose and linear velocity.
    pub fn update_listener(&mut self, pos: Vec3, forward: Vec3, up: Vec3, velocity: Vec3) {
        self.listener = AudioListener::new(pos, forward, up, velocity);
        self.pool
            .update_spatial(&self.listener, self.doppler_factor, self.speed_of_sound);
    }

    /// Plays a 3D spatialized sound effect at a given world coordinate.
    pub fn play_sound_3d(
        &mut self,
        sound: Arc<WavSound>,
        position: Vec3,
        sound_radius: f32,
        volume: f32,
        pitch: f32,
        priority: u8,
    ) -> Result<VoiceId, AudioEngineError> {
        let effective_volume = volume * self.master_volume * self.sfx_volume;
        let config = PlaySoundConfig {
            volume: effective_volume,
            pitch,
            priority,
            looping: false,
            is_spatial: true,
            emitter_pos: position,
            emitter_vel: Vec3::ZERO,
            sound_radius,
            max_distance: (sound_radius * 40.0 * 10.0).max(5000.0),
            rolloff: 1.0,
            model: self.distance_model,
        };

        let voice_id = self.pool.play(sound, &config)?;
        // Initial spatial calculation
        self.pool
            .update_spatial(&self.listener, self.doppler_factor, self.speed_of_sound);
        Ok(voice_id)
    }

    /// Plays a 2D non-spatialized UI or menu sound.
    pub fn play_sound_ui(
        &mut self,
        sound: Arc<WavSound>,
        volume: f32,
        pitch: f32,
    ) -> Result<VoiceId, AudioEngineError> {
        let effective_volume = volume * self.master_volume * self.sfx_volume;
        let config = PlaySoundConfig {
            volume: effective_volume,
            pitch,
            priority: 255, // UI sounds have highest priority
            looping: false,
            is_spatial: false,
            ..Default::default()
        };

        let voice_id = self.pool.play(sound, &config)?;
        Ok(voice_id)
    }

    /// Plays a looping 3D ambient sound attached to an emitter position.
    pub fn play_sound_ambient(
        &mut self,
        sound: Arc<WavSound>,
        position: Vec3,
        sound_radius: f32,
        volume: f32,
    ) -> Result<VoiceId, AudioEngineError> {
        let effective_volume = volume * self.master_volume * self.sfx_volume;
        let config = PlaySoundConfig {
            volume: effective_volume,
            pitch: 1.0,
            priority: 32, // Ambient sounds have lower priority
            looping: true,
            is_spatial: true,
            emitter_pos: position,
            emitter_vel: Vec3::ZERO,
            sound_radius,
            max_distance: (sound_radius * 40.0 * 10.0).max(5000.0),
            rolloff: 1.0,
            model: self.distance_model,
        };

        let voice_id = self.pool.play(sound, &config)?;
        self.pool
            .update_spatial(&self.listener, self.doppler_factor, self.speed_of_sound);
        Ok(voice_id)
    }

    /// Updates voice pool spatial state each frame or tick.
    pub fn update(&mut self, _dt: f32) {
        self.pool
            .update_spatial(&self.listener, self.doppler_factor, self.speed_of_sound);
    }

    /// Renders `frames` audio samples into an interleaved stereo output buffer `[L0, R0, L1, R1, ...]`.
    pub fn render_audio_frames(&self, frames: usize, output_interleaved: &mut [f32]) {
        self.pool.mix_interleaved(frames, output_interleaved);
    }

    /// Stops an active voice channel.
    pub fn stop(&mut self, id: VoiceId) -> bool {
        self.pool.stop(id)
    }

    /// Stops all currently active sounds.
    pub fn stop_all(&mut self) {
        self.pool.stop_all();
    }
}
