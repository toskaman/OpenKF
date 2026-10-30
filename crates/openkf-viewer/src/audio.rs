//! Sound output for the live client: openkf-audio mixer + cpal device (Windows).
//! Sounds come from the server's `ClientHearSound` RPCs.

use glam::Vec3;
use openkf_audio::engine::AudioEngine;
use openkf_audio::wav::WavSound;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct Audio {
    engine: Arc<Mutex<AudioEngine>>,
    rate: u32,
    cache: HashMap<u32, Option<Arc<WavSound>>>,
    #[cfg(windows)]
    _stream: cpal::Stream,
    pub played: u64,
}

/// Linear resampling to the device rate (the mixer plays at the source rate).
fn resample(w: &WavSound, rate: u32) -> WavSound {
    if w.sample_rate == rate || w.sample_rate == 0 {
        return w.clone();
    }
    let ch = w.channels.max(1) as usize;
    let frames = w.frame_count;
    let out_frames = (frames as u64 * rate as u64 / w.sample_rate as u64) as usize;
    let mut out = Vec::with_capacity(out_frames * ch);
    for i in 0..out_frames {
        let src = i as f64 * w.sample_rate as f64 / rate as f64;
        let i0 = src.floor() as usize;
        let f = (src - i0 as f64) as f32;
        for c in 0..ch {
            let a = w.samples.get(i0 * ch + c).copied().unwrap_or(0.0);
            let b = w.samples.get((i0 + 1) * ch + c).copied().unwrap_or(a);
            out.push(a + (b - a) * f);
        }
    }
    WavSound::from_samples(w.channels, rate, out)
}

impl Audio {
    #[cfg(windows)]
    pub fn open() -> Result<Self, String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let dev = host.default_output_device().ok_or("no audio output device")?;
        let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
        let rate = cfg.sample_rate().0;
        let channels = cfg.channels() as usize;
        let engine = Arc::new(Mutex::new(AudioEngine::new()));
        let eng = engine.clone();
        let mut stereo: Vec<f32> = Vec::new();
        let stream = dev
            .build_output_stream(
                &cpal::StreamConfig { channels: cfg.channels(), sample_rate: cfg.sample_rate(), buffer_size: cpal::BufferSize::Default },
                move |out: &mut [f32], _| {
                    let frames = out.len() / channels.max(1);
                    stereo.resize(frames * 2, 0.0);
                    stereo.iter_mut().for_each(|x| *x = 0.0);
                    if let Ok(e) = eng.lock() {
                        e.render_audio_frames(frames, &mut stereo);
                    }
                    for f in 0..frames {
                        for c in 0..channels {
                            out[f * channels + c] = if c < 2 { stereo[f * 2 + c].clamp(-1.0, 1.0) } else { 0.0 };
                        }
                    }
                },
                |e| eprintln!("[Audio] stream error: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { engine, rate, cache: HashMap::new(), _stream: stream, played: 0 })
    }

    #[cfg(not(windows))]
    pub fn open() -> Result<Self, String> {
        // No device backend here: mix into the void (keeps the code path testable).
        Ok(Self { engine: Arc::new(Mutex::new(AudioEngine::new())), rate: 44100, cache: HashMap::new(), played: 0 })
    }

    pub fn set_listener(&self, pos: [f32; 3], yaw_ue: i32) {
        let yaw = yaw_ue as f32 * std::f32::consts::TAU / 65536.0;
        if let Ok(mut e) = self.engine.lock() {
            e.update_listener(Vec3::from(pos), Vec3::new(yaw.cos(), yaw.sin(), 0.0), Vec3::Z, Vec3::ZERO);
        }
    }

    /// Plays sound object `key` (bytes fetched lazily by `load`) at `pos` (UE units).
    pub fn play(&mut self, key: u32, load: impl FnOnce() -> Option<Vec<u8>>, pos: [f32; 3], radius: f32, volume: f32, pitch: f32) -> bool {
        let rate = self.rate;
        let snd = self
            .cache
            .entry(key)
            .or_insert_with(|| {
                let data = load()?;
                let wav = openkf_package::sound::embedded_wav(&data)?;
                let w = WavSound::parse(wav).ok()?;
                Some(Arc::new(resample(&w, rate)))
            })
            .clone();
        let Some(snd) = snd else { return false };
        let ok = self.engine.lock().map(|mut e| e.play_sound_3d(snd, Vec3::from(pos), radius.max(64.0), volume.clamp(0.0, 4.0), pitch.clamp(0.25, 4.0), 128).is_ok()).unwrap_or(false);
        if ok {
            self.played += 1;
        }
        ok
    }
}
