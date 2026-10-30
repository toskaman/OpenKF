//! # OpenKF Audio Engine (`openkf-audio`)
//!
//! Spatial 3D Audio subsystem and USound RIFF WAVE asset processor for the OpenKF engine.
//!
//! This crate implements:
//! - RIFF WAVE PCM parser matching Unreal Engine 2 `.uax` package audio specifications (8/16/24/32-bit, mono/stereo, 22.05kHz/44.1kHz).
//! - OpenAL 1.1 compliant 3D spatial audio math:
//!   - Inverse distance clamped attenuation model: $Gain(d) = \text{Clamp}\left(\frac{R_{ref}}{R_{ref} + \alpha(d_{clamped} - R_{ref})}, min, max\right)$
//!   - Doppler frequency pitch shift: $f' = f \times \frac{c - D \cdot v_{ls}}{c - D \cdot v_{ss}}$
//!   - Equal-power stereo spatial panning.
//! - Multithreaded voice emitter pool supporting up to 256 concurrent voice channels with priority-based voice stealing.
//! - High-level `AudioEngine` coordinating listener poses, sound triggers, and real-time software audio frame mixing.

pub mod engine;
pub mod hrtf_mixer;
pub mod pool;
pub mod spatial;
pub mod voice;
pub mod wav;

pub use engine::{AudioEngine, AudioEngineError};
pub use hrtf_mixer::{HrtfBinauralCue, HrtfMixerBridge, HrtfMixerError, SoundMetadata};
pub use pool::{EmitterPool, PoolError, SharedEmitterPool, MAX_VOICE_CHANNELS};
pub use spatial::{
    calculate_distance_attenuation, calculate_doppler_shift, calculate_spatial_panning,
    AudioListener, DistanceModel, DEFAULT_SPEED_OF_SOUND_UU, UE2_SOUND_RADIUS_MULTIPLIER,
    UU_PER_METER,
};
pub use voice::{PlaySoundConfig, VoiceChannel, VoiceId, VoiceState};
pub use wav::{WavError, WavSound};
