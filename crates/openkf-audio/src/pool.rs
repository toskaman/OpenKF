//! Multithreaded 256-channel voice emitter pool with priority-based voice stealing.

use std::sync::{Arc, RwLock};
use glam::Vec3;
use thiserror::Error;

use crate::spatial::AudioListener;
use crate::voice::{PlaySoundConfig, VoiceChannel, VoiceId, VoiceState};
use crate::wav::WavSound;

/// Hard ceiling for concurrent hardware/software voice channels (matching OpenAL ALAudio specs).
pub const MAX_VOICE_CHANNELS: usize = 256;

#[derive(Debug, Error, PartialEq)]
pub enum PoolError {
    #[error("All {MAX_VOICE_CHANNELS} channels are saturated with higher-priority audio")]
    PoolFullHigherPriority,
    #[error("Voice channel not found or generation expired")]
    InvalidVoiceId,
}

/// Central pool managing up to 256 simultaneous audio voices.
pub struct EmitterPool {
    channels: Box<[VoiceChannel; MAX_VOICE_CHANNELS]>,
    generations: [u32; MAX_VOICE_CHANNELS],
    active_count: usize,
}

impl Default for EmitterPool {
    fn default() -> Self {
        Self::new()
    }
}

impl EmitterPool {
    /// Creates a new pool with 256 unallocated channels.
    pub fn new() -> Self {
        // Allocate on heap to avoid 256 structs overflowing stack in constrained debug environments
        let mut channels_vec = Vec::with_capacity(MAX_VOICE_CHANNELS);
        for _ in 0..MAX_VOICE_CHANNELS {
            channels_vec.push(VoiceChannel::default());
        }
        let boxed_slice: Box<[VoiceChannel]> = channels_vec.into_boxed_slice();
        let channels: Box<[VoiceChannel; MAX_VOICE_CHANNELS]> = match boxed_slice.try_into() {
            Ok(arr) => arr,
            Err(_) => unreachable!(),
        };

        Self {
            channels,
            generations: [1; MAX_VOICE_CHANNELS],
            active_count: 0,
        }
    }

    /// Number of channels currently actively playing or paused.
    #[inline]
    pub fn active_count(&self) -> usize {
        self.active_count
    }

    /// Total capacity of this pool.
    #[inline]
    pub const fn capacity(&self) -> usize {
        MAX_VOICE_CHANNELS
    }

    /// Plays a sound in the pool.
    ///
    /// If all 256 channels are occupied, voice stealing kicks in:
    /// the voice with the lowest priority (and lowest loudness/oldest age)
    /// will be interrupted if its priority is <= the incoming sound's priority.
    pub fn play(
        &mut self,
        sound: Arc<WavSound>,
        config: &PlaySoundConfig,
    ) -> Result<VoiceId, PoolError> {
        let slot = match self.find_free_slot() {
            Some(s) => s,
            None => self.steal_voice_slot(config.priority)?,
        };

        let generation = self.generations[slot].wrapping_add(1);
        self.generations[slot] = generation;

        let voice_id = VoiceId {
            slot: slot as u16,
            generation,
        };

        self.channels[slot].initialize(voice_id, sound, config);
        self.recount_active();

        Ok(voice_id)
    }

    /// Stops an active voice channel by ID.
    pub fn stop(&mut self, id: VoiceId) -> bool {
        if self.is_valid_voice(id) {
            self.channels[id.slot as usize].reset();
            self.recount_active();
            true
        } else {
            false
        }
    }

    /// Pauses an active voice channel by ID.
    pub fn pause(&mut self, id: VoiceId) -> bool {
        if self.is_valid_voice(id) {
            let slot = id.slot as usize;
            if self.channels[slot].state == VoiceState::Playing {
                self.channels[slot].state = VoiceState::Paused;
                return true;
            }
        }
        false
    }

    /// Resumes a paused voice channel by ID.
    pub fn resume(&mut self, id: VoiceId) -> bool {
        if self.is_valid_voice(id) {
            let slot = id.slot as usize;
            if self.channels[slot].state == VoiceState::Paused {
                self.channels[slot].state = VoiceState::Playing;
                return true;
            }
        }
        false
    }

    /// Updates 3D world position and velocity of an active emitter.
    pub fn set_position(&mut self, id: VoiceId, pos: Vec3, vel: Vec3) -> bool {
        if self.is_valid_voice(id) {
            let ch = &mut self.channels[id.slot as usize];
            ch.emitter_pos = pos;
            ch.emitter_vel = vel;
            true
        } else {
            false
        }
    }

    /// Updates base volume of an active voice.
    pub fn set_volume(&mut self, id: VoiceId, volume: f32) -> bool {
        if self.is_valid_voice(id) {
            let ch = &mut self.channels[id.slot as usize];
            ch.base_volume = volume.max(0.0);
            true
        } else {
            false
        }
    }

    /// Updates base pitch of an active voice.
    pub fn set_pitch(&mut self, id: VoiceId, pitch: f32) -> bool {
        if self.is_valid_voice(id) {
            let ch = &mut self.channels[id.slot as usize];
            ch.base_pitch = pitch.max(0.01);
            true
        } else {
            false
        }
    }

    /// Checks if a voice is currently playing.
    pub fn is_playing(&self, id: VoiceId) -> bool {
        if self.is_valid_voice(id) {
            self.channels[id.slot as usize].state == VoiceState::Playing
        } else {
            false
        }
    }

    /// Stops all currently active voice channels.
    pub fn stop_all(&mut self) {
        for ch in self.channels.iter_mut() {
            ch.reset();
        }
        self.active_count = 0;
    }

    /// Updates 3D spatial properties (attenuation, panning, Doppler) for all active voices.
    pub fn update_spatial(
        &mut self,
        listener: &AudioListener,
        doppler_factor: f32,
        speed_of_sound: f32,
    ) {
        for ch in self.channels.iter_mut() {
            if ch.state == VoiceState::Playing {
                ch.update_spatial(listener, doppler_factor, speed_of_sound);
            }
        }
    }

    /// Mixes all active voices into stereo output buffers (`left_out`, `right_out`).
    pub fn mix_frames(&mut self, frames: usize, left_out: &mut [f32], right_out: &mut [f32]) {
        assert_eq!(left_out.len(), frames);
        assert_eq!(right_out.len(), frames);

        let mut finished_any = false;

        for ch in self.channels.iter_mut() {
            if ch.state == VoiceState::Playing {
                let still_playing = ch.render_frames(frames, left_out, right_out);
                if !still_playing {
                    finished_any = true;
                }
            }
        }

        if finished_any {
            self.recount_active();
        }
    }

    /// Mixes all active voices into an interleaved stereo output buffer `[L0, R0, L1, R1, ...]`.
    pub fn mix_interleaved(&mut self, frames: usize, output: &mut [f32]) {
        assert_eq!(output.len(), frames * 2);

        // Scratch buffers for separate L/R mixing
        let mut left_buf = vec![0.0f32; frames];
        let mut right_buf = vec![0.0f32; frames];

        self.mix_frames(frames, &mut left_buf, &mut right_buf);

        for i in 0..frames {
            output[i * 2] = left_buf[i];
            output[i * 2 + 1] = right_buf[i];
        }
    }

    // --- Private Helper Methods ---

    fn is_valid_voice(&self, id: VoiceId) -> bool {
        let slot = id.slot as usize;
        if slot >= MAX_VOICE_CHANNELS {
            return false;
        }
        self.generations[slot] == id.generation
            && (self.channels[slot].state == VoiceState::Playing
                || self.channels[slot].state == VoiceState::Paused)
    }

    fn find_free_slot(&self) -> Option<usize> {
        self.channels
            .iter()
            .position(|c| c.state == VoiceState::Free || c.state == VoiceState::Stopped)
    }

    /// Finds a candidate channel to steal based on lowest priority, lowest volume, and age.
    fn steal_voice_slot(&mut self, incoming_priority: u8) -> Result<usize, PoolError> {
        let mut victim_slot: Option<usize> = None;
        let mut victim_priority = u8::MAX;
        let mut victim_loudness = f32::MAX;
        let mut victim_age = 0u64;

        for (idx, ch) in self.channels.iter().enumerate() {
            let priority = ch.priority;
            let loudness = ch.computed_left_gain + ch.computed_right_gain;
            let age = ch.frames_played;

            let is_better_victim = match priority.cmp(&victim_priority) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => {
                    // Among identical priority, pick the quietest voice; if tied, pick the oldest
                    if (loudness - victim_loudness).abs() > 0.01 {
                        loudness < victim_loudness
                    } else {
                        age > victim_age
                    }
                }
                std::cmp::Ordering::Greater => false,
            };

            if is_better_victim {
                victim_priority = priority;
                victim_loudness = loudness;
                victim_age = age;
                victim_slot = Some(idx);
            }
        }

        if let Some(slot) = victim_slot {
            if victim_priority <= incoming_priority {
                // Steal this channel
                self.channels[slot].reset();
                return Ok(slot);
            }
        }

        Err(PoolError::PoolFullHigherPriority)
    }

    fn recount_active(&mut self) {
        self.active_count = self
            .channels
            .iter()
            .filter(|c| c.state == VoiceState::Playing || c.state == VoiceState::Paused)
            .count();
    }
}

/// Thread-safe wrapper around `EmitterPool` suitable for concurrent multithreaded systems.
#[derive(Clone)]
pub struct SharedEmitterPool {
    inner: Arc<RwLock<EmitterPool>>,
}

impl Default for SharedEmitterPool {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedEmitterPool {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(EmitterPool::new())),
        }
    }

    pub fn play(
        &self,
        sound: Arc<WavSound>,
        config: &PlaySoundConfig,
    ) -> Result<VoiceId, PoolError> {
        let mut pool = self.inner.write().unwrap();
        pool.play(sound, config)
    }

    pub fn stop(&self, id: VoiceId) -> bool {
        let mut pool = self.inner.write().unwrap();
        pool.stop(id)
    }

    pub fn pause(&self, id: VoiceId) -> bool {
        let mut pool = self.inner.write().unwrap();
        pool.pause(id)
    }

    pub fn resume(&self, id: VoiceId) -> bool {
        let mut pool = self.inner.write().unwrap();
        pool.resume(id)
    }

    pub fn set_position(&self, id: VoiceId, pos: Vec3, vel: Vec3) -> bool {
        let mut pool = self.inner.write().unwrap();
        pool.set_position(id, pos, vel)
    }

    pub fn is_playing(&self, id: VoiceId) -> bool {
        let pool = self.inner.read().unwrap();
        pool.is_playing(id)
    }

    pub fn active_count(&self) -> usize {
        let pool = self.inner.read().unwrap();
        pool.active_count()
    }

    pub fn stop_all(&self) {
        let mut pool = self.inner.write().unwrap();
        pool.stop_all();
    }

    pub fn update_spatial(
        &self,
        listener: &AudioListener,
        doppler_factor: f32,
        speed_of_sound: f32,
    ) {
        let mut pool = self.inner.write().unwrap();
        pool.update_spatial(listener, doppler_factor, speed_of_sound);
    }

    pub fn mix_interleaved(&self, frames: usize, output: &mut [f32]) {
        let mut pool = self.inner.write().unwrap();
        pool.mix_interleaved(frames, output);
    }
}
