//! Comprehensive verification test suite for `openkf-audio`.

use glam::Vec3;
use openkf_audio::{
    calculate_distance_attenuation, calculate_doppler_shift, calculate_spatial_panning,
    AudioEngine, AudioListener, DistanceModel, EmitterPool, PlaySoundConfig, PoolError,
    SharedEmitterPool, WavError, WavSound, DEFAULT_SPEED_OF_SOUND_UU,
    MAX_VOICE_CHANNELS, UE2_SOUND_RADIUS_MULTIPLIER,
};
use std::sync::Arc;
use std::thread;

// =========================================================================
// 1. RIFF WAV PCM Parser Tests
// =========================================================================

#[test]
fn test_wav_roundtrip_mono_22050() {
    // Generate a 1-second 22.05 kHz mono sine wave
    let sample_rate = 22050;
    let freq = 440.0;
    let mut samples = Vec::new();
    for i in 0..sample_rate {
        let t = i as f32 / sample_rate as f32;
        let s = (t * freq * 2.0 * std::f32::consts::PI).sin() * 0.8;
        samples.push(s);
    }

    let sound = WavSound::from_samples(1, sample_rate, samples.clone());
    assert_eq!(sound.channels, 1);
    assert_eq!(sound.sample_rate, 22050);
    assert_eq!(sound.frame_count, 22050);
    assert!((sound.duration_seconds - 1.0).abs() < 1e-4);

    // Encode to RIFF WAV bytes
    let wav_bytes = sound.to_wav_bytes();
    assert!(wav_bytes.len() > 44);
    assert_eq!(&wav_bytes[0..4], b"RIFF");
    assert_eq!(&wav_bytes[8..12], b"WAVE");

    // Parse bytes back
    let parsed = WavSound::parse(&wav_bytes).expect("Failed to parse encoded mono WAV");
    assert_eq!(parsed.channels, 1);
    assert_eq!(parsed.sample_rate, 22050);
    assert_eq!(parsed.frame_count, 22050);
    assert_eq!(parsed.bits_per_sample, 16);

    // Check sample accuracy (16-bit quantization error < 1/32768)
    for (orig, dec) in samples.iter().zip(parsed.samples.iter()) {
        assert!((orig - dec).abs() < 0.001);
    }
}

#[test]
fn test_wav_roundtrip_stereo_44100() {
    let sample_rate = 44100;
    let frames = 1000;
    let mut samples = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let l = (i as f32 / 1000.0) * 0.5;
        let r = -(i as f32 / 1000.0) * 0.5;
        samples.push(l);
        samples.push(r);
    }

    let sound = WavSound::from_samples(2, sample_rate, samples.clone());
    let bytes = sound.to_wav_bytes();
    let parsed = WavSound::parse(&bytes).expect("Failed to parse stereo WAV");

    assert_eq!(parsed.channels, 2);
    assert_eq!(parsed.sample_rate, 44100);
    assert_eq!(parsed.frame_count, 1000);
    assert_eq!(parsed.samples.len(), 2000);

    let frame_0 = parsed.get_frame(0).unwrap();
    assert!((frame_0[0] - samples[0]).abs() < 0.001);
    assert!((frame_0[1] - samples[1]).abs() < 0.001);
}

#[test]
fn test_wav_fractional_interpolation() {
    // 2-frame sound: Frame 0 = [0.0, 0.0], Frame 1 = [1.0, 1.0]
    let samples = vec![0.0, 0.0, 1.0, 1.0];
    let sound = WavSound::from_samples(2, 44100, samples);

    let mid = sound.sample_at_fractional(0.5);
    assert!((mid[0] - 0.5).abs() < 1e-4);
    assert!((mid[1] - 0.5).abs() < 1e-4);

    let quarter = sound.sample_at_fractional(0.25);
    assert!((quarter[0] - 0.25).abs() < 1e-4);
    assert!((quarter[1] - 0.25).abs() < 1e-4);
}

#[test]
fn test_wav_malformed_headers() {
    // Too short
    assert_eq!(WavSound::parse(&[0u8; 8]), Err(WavError::DataTooShort));

    // Not RIFF
    let mut bad_magic = vec![0u8; 44];
    bad_magic[0..4].copy_from_slice(b"NOPE");
    bad_magic[8..12].copy_from_slice(b"WAVE");
    assert_eq!(
        WavSound::parse(&bad_magic),
        Err(WavError::InvalidRiffHeader)
    );

    // Missing fmt
    let mut missing_fmt = vec![0u8; 44];
    missing_fmt[0..4].copy_from_slice(b"RIFF");
    missing_fmt[8..12].copy_from_slice(b"WAVE");
    assert_eq!(
        WavSound::parse(&missing_fmt),
        Err(WavError::MissingFmtChunk)
    );
}

// =========================================================================
// 2. OpenAL 1.1 Distance Attenuation Tests
// =========================================================================

#[test]
fn test_distance_attenuation_inverse_clamped() {
    let ref_dist = 64.0 * UE2_SOUND_RADIUS_MULTIPLIER; // 2560.0 UU
    let max_dist = 10000.0;
    let rolloff = 1.0;

    // At or within ref_distance, gain is 1.0
    let gain_close = calculate_distance_attenuation(
        0.0,
        ref_dist,
        max_dist,
        rolloff,
        0.0,
        1.0,
        DistanceModel::InverseDistanceClamped,
    );
    assert_eq!(gain_close, 1.0);

    let gain_ref = calculate_distance_attenuation(
        ref_dist,
        ref_dist,
        max_dist,
        rolloff,
        0.0,
        1.0,
        DistanceModel::InverseDistanceClamped,
    );
    assert_eq!(gain_ref, 1.0);

    // At 2x ref_distance: Gain = 2560 / (2560 + 1 * (5120 - 2560)) = 0.5
    let gain_double = calculate_distance_attenuation(
        ref_dist * 2.0,
        ref_dist,
        max_dist,
        rolloff,
        0.0,
        1.0,
        DistanceModel::InverseDistanceClamped,
    );
    assert!((gain_double - 0.5).abs() < 1e-4);

    // At 3x ref_distance: Gain = 1/3 ~ 0.3333
    let gain_triple = calculate_distance_attenuation(
        ref_dist * 3.0,
        ref_dist,
        max_dist,
        rolloff,
        0.0,
        1.0,
        DistanceModel::InverseDistanceClamped,
    );
    assert!((gain_triple - (1.0 / 3.0)).abs() < 1e-4);

    // Beyond max_distance: clamped to value at max_distance
    let gain_max = calculate_distance_attenuation(
        max_dist,
        ref_dist,
        max_dist,
        rolloff,
        0.0,
        1.0,
        DistanceModel::InverseDistanceClamped,
    );
    let gain_way_beyond = calculate_distance_attenuation(
        max_dist * 5.0,
        ref_dist,
        max_dist,
        rolloff,
        0.0,
        1.0,
        DistanceModel::InverseDistanceClamped,
    );
    assert_eq!(gain_max, gain_way_beyond);
}

// =========================================================================
// 3. OpenAL 1.1 Doppler Shift Tests
// =========================================================================

#[test]
fn test_doppler_shift_stationary() {
    let listener_pos = Vec3::new(0.0, 0.0, 0.0);
    let listener_vel = Vec3::ZERO;
    let source_pos = Vec3::new(1000.0, 0.0, 0.0);
    let source_vel = Vec3::ZERO;

    let shift = calculate_doppler_shift(
        listener_pos,
        listener_vel,
        source_pos,
        source_vel,
        1.0,
        DEFAULT_SPEED_OF_SOUND_UU,
    );
    assert_eq!(shift, 1.0);
}

#[test]
fn test_doppler_shift_source_approaching_and_receding() {
    let listener_pos = Vec3::new(1000.0, 0.0, 0.0);
    let listener_vel = Vec3::ZERO;
    let source_pos = Vec3::new(0.0, 0.0, 0.0);

    // Source moving towards listener (+X direction) at 1000 UU/s
    let source_approaching_vel = Vec3::new(1000.0, 0.0, 0.0);
    let shift_approaching = calculate_doppler_shift(
        listener_pos,
        listener_vel,
        source_pos,
        source_approaching_vel,
        1.0,
        DEFAULT_SPEED_OF_SOUND_UU,
    );
    // Pitch should increase (f' > f)
    assert!(shift_approaching > 1.0);
    let expected = DEFAULT_SPEED_OF_SOUND_UU / (DEFAULT_SPEED_OF_SOUND_UU - 1000.0);
    assert!((shift_approaching - expected).abs() < 1e-3);

    // Source moving away from listener (-X direction) at 1000 UU/s
    let source_receding_vel = Vec3::new(-1000.0, 0.0, 0.0);
    let shift_receding = calculate_doppler_shift(
        listener_pos,
        listener_vel,
        source_pos,
        source_receding_vel,
        1.0,
        DEFAULT_SPEED_OF_SOUND_UU,
    );
    // Pitch should decrease (f' < f)
    assert!(shift_receding < 1.0);
    let expected_receding = DEFAULT_SPEED_OF_SOUND_UU / (DEFAULT_SPEED_OF_SOUND_UU + 1000.0);
    assert!((shift_receding - expected_receding).abs() < 1e-3);
}

#[test]
fn test_doppler_shift_supersonic_safety() {
    let listener_pos = Vec3::new(1000.0, 0.0, 0.0);
    let listener_vel = Vec3::ZERO;
    let source_pos = Vec3::new(0.0, 0.0, 0.0);

    // Mach 3 approaching velocity (> 50,000 UU/s)
    let supersonic_vel = Vec3::new(60000.0, 0.0, 0.0);
    let shift = calculate_doppler_shift(
        listener_pos,
        listener_vel,
        source_pos,
        supersonic_vel,
        1.0,
        DEFAULT_SPEED_OF_SOUND_UU,
    );

    // Must not panic, NaN, infinity, or negative
    assert!(shift.is_finite());
    assert!(shift > 0.0);
    assert!(shift <= 10.0);
}

// =========================================================================
// 4. Spatial Panning Tests
// =========================================================================

#[test]
fn test_spatial_panning_left_right() {
    // Listener at origin facing +Y, Up is +Z -> Right is (+Y x +Z) = +X
    let listener = AudioListener::new(Vec3::ZERO, Vec3::Y, Vec3::Z, Vec3::ZERO);

    // Sound dead center ahead (+Y)
    let (center_l, center_r) = calculate_spatial_panning(&listener, Vec3::new(0.0, 500.0, 0.0));
    assert!((center_l - 1.0).abs() < 1e-3);
    assert!((center_r - 1.0).abs() < 1e-3);

    // Sound to the right (+X)
    let (right_l, right_r) = calculate_spatial_panning(&listener, Vec3::new(500.0, 0.0, 0.0));
    assert!(right_l < 0.05);
    assert!(right_r > 1.35);

    // Sound to the left (-X)
    let (left_l, left_r) = calculate_spatial_panning(&listener, Vec3::new(-500.0, 0.0, 0.0));
    assert!(left_l > 1.35);
    assert!(left_r < 0.05);
}

// =========================================================================
// 5. 256-Channel Voice Pool and Voice Stealing Tests
// =========================================================================

#[test]
fn test_pool_pause_resume() {
    let mut pool = EmitterPool::new();
    let dummy_sound = Arc::new(WavSound::from_samples(1, 22050, vec![0.1; 22050]));
    let config = PlaySoundConfig {
        priority: 50,
        volume: 0.8,
        ..Default::default()
    };
    let voice_id = pool.play(dummy_sound, &config).unwrap();
    assert!(pool.is_playing(voice_id));

    // Pause
    assert!(pool.pause(voice_id));
    assert!(!pool.is_playing(voice_id));

    // Resume
    assert!(pool.resume(voice_id));
    assert!(pool.is_playing(voice_id));

    // Stop
    assert!(pool.stop(voice_id));
    assert!(!pool.is_playing(voice_id));
}

#[test]
fn test_pool_capacity_and_voice_stealing() {
    let mut pool = EmitterPool::new();
    assert_eq!(pool.capacity(), MAX_VOICE_CHANNELS);
    assert_eq!(pool.active_count(), 0);

    let dummy_sound = Arc::new(WavSound::from_samples(1, 22050, vec![0.1; 22050]));

    // Fill all 256 channels with priority 50 sounds
    let mut ids = Vec::new();
    for i in 0..MAX_VOICE_CHANNELS {
        let config = PlaySoundConfig {
            priority: 50,
            volume: 0.5 + (i as f32 * 0.001),
            ..Default::default()
        };
        let id = pool.play(dummy_sound.clone(), &config).unwrap();
        ids.push(id);
    }

    assert_eq!(pool.active_count(), MAX_VOICE_CHANNELS);

    // Attempting to play a sound with lower priority (10) should fail
    let low_config = PlaySoundConfig {
        priority: 10,
        ..Default::default()
    };
    let low_res = pool.play(dummy_sound.clone(), &low_config);
    assert_eq!(low_res, Err(PoolError::PoolFullHigherPriority));

    // Attempting to play a sound with higher priority (100) should steal a channel
    let high_config = PlaySoundConfig {
        priority: 100,
        ..Default::default()
    };
    let high_id = pool
        .play(dummy_sound.clone(), &high_config)
        .expect("Higher priority voice should steal slot");

    assert_eq!(pool.active_count(), MAX_VOICE_CHANNELS);
    assert!(pool.is_playing(high_id));

    // Stop all
    pool.stop_all();
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn test_multithreaded_shared_pool_concurrency() {
    let shared_pool = SharedEmitterPool::new();
    let dummy_sound = Arc::new(WavSound::from_samples(1, 22050, vec![0.05; 1000]));

    let mut handles = Vec::new();

    // Spawn 8 worker threads playing sounds concurrently
    for t in 0..8 {
        let pool_clone = shared_pool.clone();
        let sound_clone = dummy_sound.clone();
        handles.push(thread::spawn(move || {
            for i in 0..30 {
                let config = PlaySoundConfig {
                    priority: ((t * 10) + (i % 20)) as u8,
                    volume: 0.8,
                    ..Default::default()
                };
                let _ = pool_clone.play(sound_clone.clone(), &config);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert!(shared_pool.active_count() <= MAX_VOICE_CHANNELS);
    assert!(shared_pool.active_count() > 0);

    // Audio mixer thread rendering frames while threads were active
    let mut mix_buf = vec![0.0f32; 1024 * 2];
    shared_pool.mix_interleaved(1024, &mut mix_buf);

    shared_pool.stop_all();
    assert_eq!(shared_pool.active_count(), 0);
}

// =========================================================================
// 6. High-level AudioEngine Integration Tests
// =========================================================================

#[test]
fn test_audio_engine_workflow() {
    let mut engine = AudioEngine::new();
    let sound = Arc::new(WavSound::from_samples(1, 44100, vec![0.2; 44100]));

    engine.update_listener(
        Vec3::new(0.0, 0.0, 100.0),
        Vec3::Y,
        Vec3::Z,
        Vec3::ZERO,
    );

    // Play 3D sound
    let voice_3d = engine
        .play_sound_3d(sound.clone(), Vec3::new(500.0, 500.0, 100.0), 64.0, 1.0, 1.0, 128)
        .expect("Failed to play 3D sound");

    // Play UI sound
    let voice_ui = engine
        .play_sound_ui(sound.clone(), 1.0, 1.0)
        .expect("Failed to play UI sound");

    assert!(engine.pool.is_playing(voice_3d));
    assert!(engine.pool.is_playing(voice_ui));
    assert_eq!(engine.pool.active_count(), 2);

    // Render audio
    let mut buffer = vec![0.0f32; 512 * 2];
    engine.render_audio_frames(512, &mut buffer);

    // Verify non-zero output rendered
    let has_signal = buffer.iter().any(|&sample| sample.abs() > 1e-4);
    assert!(has_signal, "Rendered output should contain mixed audio signal");

    engine.stop(voice_3d);
    assert_eq!(engine.pool.active_count(), 1);

    engine.stop_all();
    assert_eq!(engine.pool.active_count(), 0);
}
