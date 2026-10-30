//! Spectator replay of an `openkf-netcap` capture: decodes every server→client
//! actor bunch with `openkf-network` and keeps a time-stamped track per Pawn.

use openkf_network::package_map::UsesLine;
use openkf_network::props::Value;
use openkf_network::schema::GameSchema;
use openkf_network::session::{ActorSession, SessionEvent};
use openkf_network::wire::*;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub ms: u64,
    pub loc: [f32; 3],
    pub yaw: i32,
    pub speed: f32,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub class: String,
    pub mesh: usize,
    #[allow(dead_code)]
    pub channel: u32,
    pub open_ms: u64,
    pub close_ms: u64,
    pub samples: Vec<Sample>,
    pub is_player: bool,
}

pub struct Replay {
    pub tracks: Vec<Track>,
    /// (package, mesh, max simultaneous instances)
    pub pool: Vec<(String, String, usize)>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub map: Option<String>,
}

/// Class chain → (package, skeletal mesh).
pub fn mesh_for(chain: &[String]) -> Option<(&'static str, &'static str)> {
    let has = |k: &str| chain.iter().any(|c| c.to_ascii_lowercase().starts_with(k));
    let z = |m| Some(("KF_Freaks_Trip", m));
    if has("zombieclot") { return z("CLOT_Freak"); }
    if has("zombiebloat") { return z("Bloat_Freak"); }
    if has("zombiegorefast") { return z("GoreFast_Freak"); }
    if has("zombiecrawler") { return z("Crawler_Freak"); }
    if has("zombiestalker") { return z("Stalker_Freak"); }
    if has("zombiesiren") { return z("Siren_Freak"); }
    if has("zombiescrake") { return z("Scrake_Freak"); }
    if has("zombiefleshpound") { return z("FleshPound_Freak"); }
    if has("zombieboss") { return z("Patriarch_Freak"); }
    if has("kfhumanpawn") { return Some(("KF_Soldier_Trip", "British_Soldier1")); }
    None
}

pub fn load(capture: &Path, kf: &Path) -> Result<Replay, String> {
    let text = std::fs::read_to_string(capture).map_err(|e| e.to_string())?;
    let recs = parse_capture(&text);
    let mut uses = Vec::new();
    let mut bunches = Vec::new();
    let mut map = None;
    for r in recs.iter().filter(|r| !r.from_client) {
        let Ok(p) = Packet::decode(&r.bytes) else { continue };
        for it in p.items {
            let Item::Bunch(b) = it else { continue };
            if b.ch_index == 0 {
                if b.reliable {
                    for m in control_messages(&b).unwrap_or_default() {
                        if let Some(u) = UsesLine::parse(&m) {
                            uses.push(u);
                        } else if let Some(rest) = m.strip_prefix("WELCOME LEVEL=") {
                            map = rest.split(' ').next().map(str::to_string);
                        }
                    }
                }
            } else {
                bunches.push((r.ms, b));
            }
        }
    }
    let mut session = ActorSession::new(GameSchema::load(kf, uses));
    let mut meshes: Vec<(String, String)> = Vec::new();
    let mut tracks: Vec<Track> = Vec::new();
    let mut live: HashMap<u32, usize> = HashMap::new();
    for (ms, b) in &bunches {
        let ev = session.handle(b);
        match ev {
            Some(SessionEvent::Opened(ch)) => {
                if let Some(a) = session.actors.get(&ch) {
                    if let Some((pkg, mesh)) = mesh_for(&a.class.chain) {
                        let key = (pkg.to_string(), mesh.to_string());
                        let mi = meshes.iter().position(|m| *m == key).unwrap_or_else(|| {
                            meshes.push(key);
                            meshes.len() - 1
                        });
                        live.insert(ch, tracks.len());
                        tracks.push(Track {
                            class: a.class.name.clone(),
                            mesh: mi,
                            channel: ch,
                            open_ms: *ms,
                            close_ms: u64::MAX,
                            samples: Vec::new(),
                            is_player: a.class.chain.iter().any(|c| c == "KFHumanPawn"),
                        });
                    }
                }
            }
            Some(SessionEvent::Updated(_, ref ups)) => {
                // The owning client gets its own pawn's movement through
                // PlayerController.ClientAdjustPosition, not PawnPosition.
                for u in ups {
                    let openkf_network::props::Update::Rpc { name, params } = u else { continue };
                    if name != "ClientAdjustPosition" {
                        continue;
                    }
                    let f = |k: &str| params.iter().find(|p| p.0 == k).and_then(|p| if let Value::Float(v) = p.1 { Some(v) } else { None });
                    let (Some(x), Some(y), Some(z)) = (f("NewLocX"), f("NewLocY"), f("NewLocZ")) else { continue };
                    let (vx, vy) = (f("NewVelX").unwrap_or(0.0), f("NewVelY").unwrap_or(0.0));
                    let speed = (vx * vx + vy * vy).sqrt();
                    if let Some(tr) = tracks.iter_mut().rev().find(|t| t.is_player && t.close_ms == u64::MAX) {
                        let last_yaw = tr.samples.last().map_or(0, |s| s.yaw);
                        let yaw = if speed > 10.0 { (vy.atan2(vx) / std::f32::consts::TAU * 65536.0) as i32 & 0xffff } else { last_yaw };
                        tr.samples.push(Sample { ms: *ms, loc: [x, y, z], yaw, speed });
                    }
                }
            }
            Some(SessionEvent::Closed(ch)) => {
                if let Some(t) = live.remove(&ch) {
                    tracks[t].close_ms = *ms;
                }
                continue;
            }
            _ => {}
        }
        if let (Some(&t), Some(a)) = (live.get(&b.ch_index), session.actors.get(&b.ch_index)) {
            let dead = matches!(a.props.get("Health"), Some(Value::Int(h)) if *h <= 0);
            if dead && tracks[t].close_ms == u64::MAX {
                tracks[t].close_ms = *ms + 1500;
            }
            let v = a.velocity;
            let s = Sample { ms: *ms, loc: a.location, yaw: a.rotation[1], speed: (v[0] * v[0] + v[1] * v[1]).sqrt() };
            let tr = &mut tracks[t];
            if tr.is_player && tr.samples.len() > 1 {
                continue;
            }
            if tr.samples.last().is_none_or(|p| p.loc != s.loc || p.yaw != s.yaw) {
                tr.samples.push(s);
            }
        }
    }
    let end_ms = bunches.last().map(|b| b.0).unwrap_or(0);
    let start_ms = tracks.iter().map(|t| t.open_ms).min().unwrap_or(0);
    // Max simultaneous instances per mesh.
    let mut pool = Vec::new();
    for (mi, (pkg, mesh)) in meshes.iter().enumerate() {
        let mut ev: Vec<(u64, i32)> = Vec::new();
        for t in tracks.iter().filter(|t| t.mesh == mi) {
            ev.push((t.open_ms, 1));
            ev.push((t.close_ms.min(end_ms + 1), -1));
        }
        ev.sort();
        let (mut cur, mut max) = (0, 0);
        for (_, d) in ev {
            cur += d;
            max = max.max(cur);
        }
        pool.push((pkg.clone(), mesh.clone(), max.max(1) as usize));
    }
    let st = session.stats;
    println!(
        "[Replay] {} pawn tracks, {:.1} s, bunches ok {} / failed {} | pool {:?}",
        tracks.len(),
        (end_ms - start_ms) as f32 / 1000.0,
        st.bunches_ok,
        st.bunches_failed,
        pool.iter().map(|p| format!("{}x{}", p.2, p.1)).collect::<Vec<_>>()
    );
    Ok(Replay { tracks, pool, start_ms, end_ms, map })
}

impl Track {
    /// Interpolated state at `ms`, None when the actor does not exist then.
    pub fn at(&self, ms: u64) -> Option<Sample> {
        if ms < self.open_ms || ms >= self.close_ms || self.samples.is_empty() {
            return None;
        }
        let i = self.samples.partition_point(|s| s.ms <= ms);
        if i == 0 {
            return Some(self.samples[0]);
        }
        let a = self.samples[i - 1];
        let Some(b) = self.samples.get(i) else { return Some(a) };
        let f = ((ms - a.ms) as f32 / (b.ms - a.ms).max(1) as f32).clamp(0.0, 1.0);
        // Don't interpolate across long gaps (teleports / dormant updates).
        if b.ms - a.ms > 1000 {
            return Some(a);
        }
        let lerp = |x: f32, y: f32| x + (y - x) * f;
        let dy = (b.yaw.wrapping_sub(a.yaw) as i16) as f32;
        Some(Sample {
            ms,
            loc: [lerp(a.loc[0], b.loc[0]), lerp(a.loc[1], b.loc[1]), lerp(a.loc[2], b.loc[2])],
            yaw: a.yaw.wrapping_add((dy * f) as i32),
            speed: lerp(a.speed, b.speed),
        })
    }
}
