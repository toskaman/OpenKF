//! Individual audio voice channel representation and DSP mixing logic.

use std::sync::Arc;
use glam::Vec3;

use crate::spatial::{
    calculate_distance_attenuation, calculate_doppler_shift, calculate_spatial_panning,
    AudioListener, DistanceModel, DEFAULT_SPEED_OF_SOUND_UU, UE2_SOUND_RADIUS_MULTIPLIER,
};
use crate::wav::WavSound;

/// Generational unique identifier for an allocated voice channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VoiceId {
    /// Slot index within the emitter pool (0..255).
    pub slot: u16,
    /// Generation counter to prevent ABA handle reuse.
    pub generation: u32,
}

/// Operational state of a voice channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    #[default]
    Free,
    Playing,
    Paused,
    Stopped,
}

/// Configuration parameters for triggering a sound playback.
#[derive(Debug, Clone)]
pub struct PlaySoundConfig {
    pub volume: f32,
    pub pitch: f32,
    pub priority: u8,
    pub looping: bool,
    pub is_spatial: bool,
    pub emitter_pos: Vec3,
    pub emitter_vel: Vec3,
    pub sound_radius: f32,
    pub max_distance: f32,
    pub rolloff: f32,
    pub model: DistanceModel,
}

impl Default for PlaySoundConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pitch: 1.0,
            priority: 128,
            looping: false,
            is_spatial: false,
            emitter_pos: Vec3::ZERO,
            emitter_vel: Vec3::ZERO,
            sound_radius: 64.0, // Default UE2 SoundRadius
            max_distance: 10_000.0,
            rolloff: 1.0,
            model: DistanceModel::InverseDistanceClamped,
        }
    }
}

/// A single voice channel slot capable of playing and resampling a `WavSound`.
#[derive(Debug, Clone)]
pub struct VoiceChannel {
    pub id: Option<VoiceId>,
    pub state: VoiceState,
    pub sound: Option<Arc<WavSound>>,
    /// Fractional playback position (frame index).
    pub playhead: f64,
    pub base_volume: f32,
    pub base_pitch: f32,
    pub priority: u8,
    pub looping: bool,
    pub is_spatial: bool,

    // Spatial physics parameters
    pub emitter_pos: Vec3,
    pub emitter_vel: Vec3,
    pub ref_distance: f32,
    pub max_distance: f32,
    pub rolloff: f32,
    pub model: DistanceModel,

    // Real-time computed DSP gains & Doppler pitch
    pub computed_left_gain: f32,
    pub computed_right_gain: f32,
    pub computed_pitch: f32,
    pub frames_played: u64,
}

impl Default for VoiceChannel {
    fn default() -> Self {
        Self {
            id: None,
            state: VoiceState::Free,
            sound: None,
            playhead: 0.0,
            base_volume: 1.0,
            base_pitch: 1.0,
            priority: 0,
            looping: false,
            is_spatial: false,
            emitter_pos: Vec3::ZERO,
            emitter_vel: Vec3::ZERO,
            ref_distance: 64.0 * UE2_SOUND_RADIUS_MULTIPLIER,
            max_distance: 10_000.0,
            rolloff: 1.0,
            model: DistanceModel::InverseDistanceClamped,
            computed_left_gain: 0.0,
            computed_right_gain: 0.0,
            computed_pitch: 1.0,
            frames_played: 0,
        }
    }
}

impl VoiceChannel {
    /// Resets the channel to an idle, free state.
    pub fn reset(&mut self) {
        self.id = None;
        self.state = VoiceState::Free;
        self.sound = None;
        self.playhead = 0.0;
        self.base_volume = 1.0;
        self.base_pitch = 1.0;
        self.priority = 0;
        self.looping = false;
        self.is_spatial = false;
        self.emitter_pos = Vec3::ZERO;
        self.emitter_vel = Vec3::ZERO;
        self.computed_left_gain = 0.0;
        self.computed_right_gain = 0.0;
        self.computed_pitch = 1.0;
        self.frames_played = 0;
    }

    /// Initializes and activates the voice channel with given sound and configuration.
    pub fn initialize(&mut self, id: VoiceId, sound: Arc<WavSound>, config: &PlaySoundConfig) {
        let ref_dist = (config.sound_radius * UE2_SOUND_RADIUS_MULTIPLIER).max(1.0);
        self.id = Some(id);
        self.state = VoiceState::Playing;
        self.sound = Some(sound);
        self.playhead = 0.0;
        self.base_volume = config.volume.max(0.0);
        self.base_pitch = config.pitch.max(0.01);
        self.priority = config.priority;
        self.looping = config.looping;
        self.is_spatial = config.is_spatial;
        self.emitter_pos = config.emitter_pos;
        self.emitter_vel = config.emitter_vel;
        self.ref_distance = ref_dist;
        self.max_distance = config.max_distance.max(ref_dist);
        self.rolloff = config.rolloff;
        self.model = config.model;
        self.computed_left_gain = self.base_volume;
        self.computed_right_gain = self.base_volume;
        self.computed_pitch = self.base_pitch;
        self.frames_played = 0;
    }

    /// Recalculates spatial attenuation, panning, and Doppler pitch shift relative to the listener.
    pub fn update_spatial(
        &mut self,
        listener: &AudioListener,
        doppler_factor: f32,
        speed_of_sound: f32,
    ) {
        if self.state != VoiceState::Playing {
            return;
        }

        if !self.is_spatial {
            self.computed_left_gain = self.base_volume;
            self.computed_right_gain = self.base_volume;
            self.computed_pitch = self.base_pitch;
            return;
        }

        let dist = (self.emitter_pos - listener.position).length();
        let atten = calculate_distance_attenuation(
            dist,
            self.ref_distance,
            self.max_distance,
            self.rolloff,
            0.0,
            1.0,
            self.model,
        );

        let (pan_l, pan_r) = calculate_spatial_panning(listener, self.emitter_pos);
        self.computed_left_gain = self.base_volume * atten * pan_l;
        self.computed_right_gain = self.base_volume * atten * pan_r;

        let doppler_mult = calculate_doppler_shift(
            listener.position,
            listener.velocity,
            self.emitter_pos,
            self.emitter_vel,
            doppler_factor,
            if speed_of_sound > 0.0 {
                speed_of_sound
            } else {
                DEFAULT_SPEED_OF_SOUND_UU
            },
        );

        self.computed_pitch = (self.base_pitch * doppler_mult).clamp(0.05, 10.0);
    }

    /// Renders audio frames into separated left and right accumulation buffers.
    /// Returns `true` if the voice is still actively playing, or `false` if it finished.
    pub fn render_frames(
        &mut self,
        frames: usize,
        left_buf: &mut [f32],
        right_buf: &mut [f32],
    ) -> bool {
        if self.state != VoiceState::Playing {
            return false;
        }

        let sound = match &self.sound {
            Some(s) => s,
            None => {
                self.state = VoiceState::Stopped;
                return false;
            }
        };

        if sound.frame_count == 0 {
            self.state = VoiceState::Stopped;
            return false;
        }

        let pitch_step = self.computed_pitch as f64;
        let total_frames = sound.frame_count as f64;

        for i in 0..frames {
            if self.playhead >= total_frames {
                if self.looping {
                    self.playhead %= total_frames;
                } else {
                    self.state = VoiceState::Stopped;
                    return false;
                }
            }

            let stereo_sample = sound.sample_at_fractional(self.playhead);
            left_buf[i] += stereo_sample[0] * self.computed_left_gain;
            right_buf[i] += stereo_sample[1] * self.computed_right_gain;

            self.playhead += pitch_step;
            self.frames_played += 1;
        }

        true
    }
}
