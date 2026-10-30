//! Live spectator: runs `openkf_network::live::LiveClient` on a background
//! thread and publishes the current Pawns for the renderer.

use crate::replay::mesh_for;
use openkf_package::uc_defaults::ScriptDefaults;
use openkf_network::live::{AuthConfig, LiveClient, LiveState};
use openkf_network::props::Value;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Fixed pool (package, mesh, instances) since spawns are not known in advance.
pub const POOL: &[(&str, &str, usize)] = &[
    ("KF_Freaks_Trip", "CLOT_Freak", 16),
    ("KF_Freaks_Trip", "GoreFast_Freak", 8),
    ("KF_Freaks_Trip", "Crawler_Freak", 8),
    ("KF_Freaks_Trip", "Stalker_Freak", 8),
    ("KF_Freaks_Trip", "Bloat_Freak", 6),
    ("KF_Freaks_Trip", "Siren_Freak", 4),
    ("KF_Freaks_Trip", "Scrake_Freak", 3),
    ("KF_Freaks_Trip", "FleshPound_Freak", 3),
    ("KF_Freaks_Trip", "Patriarch_Freak", 1),
    ("KF_Soldier_Trip", "British_Soldier1", 6),
];

/// How long a dead pawn stays visible (death animation / fall-over), like the
/// retail corpse before it fades. Ragdoll physics is not simulated.
pub const CORPSE_TIME: Duration = Duration::from_secs(6);

pub fn pool() -> Vec<(String, String, usize)> {
    POOL.iter().map(|p| (p.0.to_string(), p.1.to_string(), p.2)).collect()
}

fn is_a(d: &ScriptDefaults, class: &str, base: &str) -> bool {
    let mut cur = class.to_ascii_lowercase();
    for _ in 0..24 {
        if cur == base {
            return true;
        }
        match d.classes.get(&cur).and_then(|c| c.parent.clone()) {
            Some(p) => cur = p.to_ascii_lowercase(),
            None => return false,
        }
    }
    false
}

/// Pool from the game's class defaults: every `*_STANDARD` zed + the player mesh.
pub fn pool_from_defaults(d: &ScriptDefaults) -> Vec<(String, String, usize)> {
    let mut out: Vec<(String, String, usize)> = Vec::new();
    let mut names: Vec<&String> = d.classes.keys().filter(|k| k.ends_with("_standard") && is_a(d, k, "kfmonster")).collect();
    names.sort();
    for k in names {
        if let Some((_, pkg, mesh)) = d.object_ref(k, "Mesh") {
            let n = if k.contains("boss") { 1 } else if k.contains("scrake") || k.contains("fleshpound") || k.contains("husk") { 3 } else { 8 };
            if !out.iter().any(|p| p.0.eq_ignore_ascii_case(&pkg) && p.1.eq_ignore_ascii_case(&mesh)) {
                out.push((pkg, mesh, n));
            }
        }
    }
    if let Some((_, pkg, mesh)) = d.object_ref("KFHumanPawn", "Mesh") {
        out.push((pkg, mesh, 6));
    }
    // First-person meshes of the starting inventory (one each).
    for w in VIEW_WEAPONS {
        if let Some((kind, pkg, mesh)) = d.object_ref(w, "Mesh") {
            if kind.eq_ignore_ascii_case("SkeletalMesh") && !out.iter().any(|p| p.0.eq_ignore_ascii_case(&pkg) && p.1.eq_ignore_ascii_case(&mesh)) {
                out.push((pkg, mesh, 1));
            }
        }
    }
    // Third-person weapon meshes (held weapons), 2 instances each.
    let mut att: Vec<&String> = d.classes.keys().filter(|k| is_a(d, k, "kfweaponattachment")).collect();
    att.sort();
    for k in att {
        if let Some((kind, pkg, mesh)) = d.object_ref(k, "Mesh") {
            if kind.eq_ignore_ascii_case("SkeletalMesh") && !out.iter().any(|p| p.0.eq_ignore_ascii_case(&pkg) && p.1.eq_ignore_ascii_case(&mesh)) {
                out.push((pkg, mesh, 2));
            }
        }
    }
    if out.is_empty() {
        return pool();
    }
    out
}

/// `FireModeClass(0)` -> `FireSound` of a weapon class, as "Pkg.Name".
fn fire_sound(d: &ScriptDefaults, weapon_class: &str) -> Option<String> {
    let fm = d.get(weapon_class, "FireModeClass(0)")?;
    let fm = fm.split('\'').nth(1)?.rsplit('.').next()?.to_string();
    let (_, pkg, name) = d.object_ref(&fm, "FireSound")?;
    Some(format!("{pkg}.{name}"))
}

/// Pool slot for a class: its default Mesh if pooled, else the built-in table.
fn mesh_slot(pool: &[(String, String, usize)], d: Option<&ScriptDefaults>, chain: &[String]) -> Option<usize> {
    if let Some(d) = d {
        if let Some((_, pkg, mesh)) = chain.first().and_then(|c| d.object_ref(c, "Mesh")) {
            if let Some(i) = pool.iter().position(|p| p.0.eq_ignore_ascii_case(&pkg) && p.1.eq_ignore_ascii_case(&mesh)) {
                return Some(i);
            }
        }
    }
    let (pkg, mesh) = mesh_for(chain)?;
    pool.iter().position(|p| p.0.eq_ignore_ascii_case(pkg) && p.1.eq_ignore_ascii_case(mesh))
}

#[derive(Debug, Clone)]
pub struct AttachView {
    pub mesh: usize,
    /// Index in `Shared::pawns` of the pawn carrying it.
    pub parent: usize,
    pub bone: String,
}

#[derive(Debug, Clone)]
pub struct PawnView {
    pub channel: u32,
    pub mesh: usize,
    pub loc: [f32; 3],
    pub yaw: i32,
    /// Physics-driven orientation for ragdoll corpses (UE units).
    pub pitch: i32,
    pub roll: i32,
    /// True while a Karma ragdoll drives this corpse (no death anim).
    pub ragdoll: bool,
    pub speed: f32,
    /// Locomotion sequence for the current velocity (`MovementAnims(0..3)` /
    /// `WalkAnims` / `CrouchAnims` of the class, picked by direction vs. yaw).
    pub move_anim: Option<String>,
    /// Idle sequence of the class (`IdleRestAnim`, `IdleCrouchAnim`, ...).
    pub idle_anim: Option<String>,
    /// One-shot `Pawn.AnimAction` (replicated): (sequence, seconds since it started).
    pub action: Option<(String, f32)>,
    /// Seconds since death (`Health <= 0` / `bTearOff`), corpse kept for a few seconds.
    pub dying: Option<f32>,
    /// Class `DeathAnim(0..3)` / `CollisionHeight` (corpse fall-over fallback).
    pub death_anims: Vec<String>,
    pub collision_height: f32,
}

/// A replicated movable static-mesh actor (pickup / projectile).
#[derive(Debug, Clone, Copy)]
pub struct PropView {
    /// Index into the prop pool (see `prop_pool_from_defaults`).
    pub mesh: usize,
    pub loc: [f32; 3],
    pub rot: [i32; 3],
    /// DrawScale * DrawScale3D.
    pub scale: [f32; 3],
    pub pre_pivot: [f32; 3],
}

/// Prop pool shared with the scene loader (set once in `main`).
static PROP_POOL: std::sync::OnceLock<Vec<(String, String, usize)>> = std::sync::OnceLock::new();

pub fn set_prop_pool(p: Vec<(String, String, usize)>) {
    let _ = PROP_POOL.set(p);
}

pub fn prop_pool() -> &'static [(String, String, usize)] {
    PROP_POOL.get().map(|v| v.as_slice()).unwrap_or(&[])
}

fn is_static_mesh_class(d: &ScriptDefaults, class: &str) -> bool {
    d.get(class, "DrawType").is_some_and(|v| v.trim().eq_ignore_ascii_case("DT_StaticMesh"))
        && d.object_ref(class, "StaticMesh").is_some_and(|r| r.0.eq_ignore_ascii_case("StaticMesh"))
}

/// Pool of static meshes for every pickup / projectile class drawn as a static mesh.
pub fn prop_pool_from_defaults(d: &ScriptDefaults) -> Vec<(String, String, usize)> {
    let mut names: Vec<&String> = d.classes.keys().collect();
    names.sort();
    let mut out: Vec<(String, String, usize)> = Vec::new();
    for k in names {
        let n = if is_a(d, k, "projectile") { 6 } else if is_a(d, k, "pickup") { 3 } else { continue };
        if !is_static_mesh_class(d, k) {
            continue;
        }
        let Some((_, pkg, mesh)) = d.object_ref(k, "StaticMesh") else { continue };
        match out.iter_mut().find(|p| p.0.eq_ignore_ascii_case(&pkg) && p.1.eq_ignore_ascii_case(&mesh)) {
            Some(e) => e.2 = e.2.max(n),
            None => out.push((pkg, mesh, n)),
        }
        if out.len() >= 192 {
            break;
        }
    }
    out
}

fn prop_slot(pool: &[(String, String, usize)], d: &ScriptDefaults, class: &str, replicated: Option<&str>) -> Option<usize> {
    // Replicated StaticMesh ("Pkg.Group.Name") wins over the class default.
    if let Some(full) = replicated {
        let mut parts = full.split('.');
        let pkg = parts.next().unwrap_or("");
        let name = full.rsplit('.').next().unwrap_or("");
        if let Some(i) = pool.iter().position(|p| p.0.eq_ignore_ascii_case(pkg) && p.1.eq_ignore_ascii_case(name)) {
            return Some(i);
        }
    }
    if !is_static_mesh_class(d, class) {
        return None;
    }
    let (_, pkg, mesh) = d.object_ref(class, "StaticMesh")?;
    pool.iter().position(|p| p.0.eq_ignore_ascii_case(&pkg) && p.1.eq_ignore_ascii_case(&mesh))
}

fn def_f32(d: &ScriptDefaults, class: &str, key: &str) -> Option<f32> {
    d.get(class, key).and_then(|v| v.trim().parse().ok())
}

fn unquote(v: &str) -> String {
    v.trim().trim_matches('"').trim_matches('\'').to_string()
}

/// Locomotion sequence of `class` for velocity `v` seen from `yaw` (UE2 Pawn: MovementAnims
/// 0=fwd 1=back 2=left 3=right; WalkAnims when walking, CrouchAnims when crouched).
pub fn movement_anim(d: &ScriptDefaults, class: &str, v: [f32; 3], yaw: i32, walking: bool, crouched: bool) -> Option<String> {
    let speed = v[0].hypot(v[1]);
    if speed < 1.0 {
        return None;
    }
    let y = yaw as f32 * std::f32::consts::TAU / 65536.0;
    let (fx, fy) = (y.cos(), y.sin());
    let fwd = (v[0] * fx + v[1] * fy) / speed;
    let right = (-v[0] * fy + v[1] * fx) / speed;
    let dir = if fwd.abs() >= right.abs() { if fwd >= 0.0 { 0 } else { 1 } } else if right >= 0.0 { 3 } else { 2 };
    let base = if crouched { "CrouchAnims" } else if walking { "WalkAnims" } else { "MovementAnims" };
    d.get(class, &format!("{base}({dir})")).or_else(|| d.get(class, &format!("MovementAnims({dir})"))).map(unquote).filter(|s| !s.is_empty())
}

pub fn idle_anim(d: &ScriptDefaults, class: &str, crouched: bool) -> Option<String> {
    let keys: &[&str] = if crouched { &["IdleCrouchAnim", "IdleRestAnim"] } else { &["IdleRestAnim", "IdleWeaponAnim", "IdleHeavyAnim"] };
    keys.iter().find_map(|k| d.get(class, k)).map(unquote).filter(|s| !s.is_empty())
}

/// Player input from the window (play mode). Angles in UE units.
#[derive(Debug, Default, Clone, Copy)]
pub struct Input {
    /// World-space wish direction (x, y), length <= 1.
    pub wish: [f32; 2],
    pub yaw: i32,
    pub pitch: i32,
    pub fire: bool,
    pub reload: bool,
    /// Weapon slot 1..5 requested (knife, Single, Frag, Syringe, Welder).
    pub slot: Option<u8>,
    /// Throw a grenade / buy a clip for the held weapon / buy a shotgun (one-shot flags).
    pub grenade: bool,
    pub buy_clip: bool,
    pub buy_shotgun: bool,
}

const SLOTS: [&str; 5] = ["knife", "Single", "Frag", "Syringe", "Welder"];
/// Weapons whose first-person mesh is pooled.
pub const VIEW_WEAPONS: [&str; 5] = ["Knife", "Single", "Frag", "Syringe", "Welder"];

/// First-person weapon to draw: pool slot + PlayerViewOffset (UE, view space).
#[derive(Debug, Clone)]
pub struct ViewWeapon {
    pub mesh: usize,
    pub offset: [f32; 3],
    /// Weapon.DisplayFOV (degrees, horizontal): the weapon is drawn with this FOV.
    pub display_fov: f32,
    /// One-shot animation: (sequence name, seconds since start, rate multiplier).
    pub anim: Option<(String, f32, f32)>,
}

fn parse_vec(v: &str) -> Option<[f32; 3]> {
    let v = v.trim().trim_start_matches('(').trim_end_matches(')');
    let mut out = [0.0f32; 3];
    for kv in v.split(',') {
        let (k, x) = kv.split_once('=')?;
        let x: f32 = x.trim().parse().ok()?;
        match k.trim().to_ascii_uppercase().as_str() { "X" => out[0] = x, "Y" => out[1] = x, "Z" => out[2] = x, _ => {} }
    }
    Some(out)
}

#[derive(Debug, Default)]
pub struct Shared {
    /// Some(..) in play mode: the bot is disabled and this input is sent instead.
    pub input: Option<Input>,
    pub me: Option<[f32; 3]>,
    /// One-line HUD: health, ammo, kills, wave.
    pub hud: String,
    pub map: Option<String>,
    pub pawns: Vec<PawnView>,
    pub attachments: Vec<AttachView>,
    pub props: Vec<PropView>,
    pub view_weapon: Option<ViewWeapon>,
    pub status: String,
    pub failed: bool,
    pub reconnects: u32,
    pub status_extra: String,
    /// Combat/ambient particles spawned by the live thread (UE coords).
    pub particles: crate::particles::ParticleSystem,
}

/// One-line download progress for the HUD (font-safe chars only).
pub fn download_hud_line(c: &LiveClient) -> Option<String> {
    let (src, file, rx, tot, q) = c.download_progress()?;
    let pct = if tot > 0 { (rx * 100 / tot).min(100) } else { 0 };
    let filled = (pct as usize / 10).min(10);
    let bar: String = "|".repeat(filled) + &"-".repeat(10 - filled);
    Some(if tot >= 10 * 1_048_576 {
        format!("DL {src} {file} {pct}% {}/{}MB {bar} {} LEFT", rx / 1_048_576, tot / 1_048_576, q)
    } else if tot > 0 {
        format!("DL {src} {file} {pct}% {}/{}KB {bar} {} LEFT", rx / 1024, tot / 1024, q)
    } else {
        format!("DL {src} {file} {}KB {bar} {} LEFT", rx / 1024, q)
    })
}

pub fn start(server: SocketAddr, kf: PathBuf, auth: Option<AuthConfig>, name: String, ready: bool, defaults: Option<Arc<ScriptDefaults>>, pool: Vec<(String, String, usize)>) -> Arc<Mutex<Shared>> {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let sh = shared.clone();
    std::thread::spawn(move || {
        let connect = |auth: &Option<AuthConfig>| match auth {
            Some(a) => LiveClient::connect(server, &kf, a.clone(), &name).map_err(|e| e.to_string()),
            None => LiveClient::connect_steam(server, &kf, &name),
        };
        let with_defaults = |r: Result<LiveClient, String>| r.map(|mut c| { c.defaults = defaults.clone(); c });
        let conn = with_defaults(connect(&auth));
        let mut c = match conn {
            Ok(c) => c,
            Err(e) => {
                let mut s = sh.lock().unwrap();
                s.failed = true;
                s.status = format!("connect: {e}");
                return;
            }
        };
        let mut shown = 0;
        let mut audio = match crate::audio::Audio::open() {
            Ok(a) => Some(a),
            Err(e) => {
                println!("[Audio] disabled: {e}");
                None
            }
        };
        let mut sounds_failed = 0u64;
        let mut local_fire = false;
        // First-person weapon animation: (sequence, start, rate).
        let mut view_anim: Option<(String, Instant, f32)> = None;
        let mut anim_reload_seen = Instant::now();
        let mut readied = false;
        let mut last_move = Instant::now();
        let mut flee_dir: Option<[f32; 2]> = None;
        let mut stuck_ref = ([0.0f32; 3], Instant::now());
        let mut turn = 0.0f32;
        let mut acked_pawn: Option<u32> = None;
        let mut fails_logged = 0u32;
        let mut last_buy = Instant::now();
        let mut buys = 0u32;
        let mut path_logged = 0usize;
        let mut nav: Option<(String, openkf_package::navigation::NavGraph)> = None;
        let mut nav_path: Vec<[f32; 3]> = Vec::new();
        let mut dbg_t = Instant::now();
        let mut path_sig = 0usize;
        let mut path_cursor = 0usize;
        let mut last_use = Instant::now();
        let mut nav_time = Instant::now() - Duration::from_secs(10);
        let mut armed_for: Option<u32> = None;
        let mut last_wcheck = Instant::now();
        let mut weapon: Option<u32> = None;
        let mut firing_since: Option<Instant> = None;
        let mut last_shot = Instant::now();
        let mut shots = 0u32;
        let mut kills = 0u32;
        let mut last_reload = Instant::now() - Duration::from_secs(10);
        let mut throw_at: Option<(u32, Instant)> = None;
        let mut seen_dead: std::collections::HashSet<u32> = Default::default();
        let mut anim_actions: std::collections::HashMap<u32, (String, Instant)> = Default::default();
        let mut died_at: std::collections::HashMap<u32, Instant> = Default::default();
        let mut corpses: Vec<(PawnView, Instant)> = Vec::new();
        let mut last_pawns: Vec<PawnView> = Vec::new();
        let mut last_dl_print = Instant::now() - Duration::from_secs(10);
        let mut emitter_tick = Instant::now();
        let mut part_tick = Instant::now();
        let mut ragdolls: std::collections::HashMap<u32, crate::ragdoll::RagdollState> = Default::default();
        let t0 = Instant::now();
        loop {
            let evs = c.poll();
            // Download progress bar (background FastDL / channel): console printout
            // here, HUD override in the shared update below.
            if c.downloading() {
                if last_dl_print.elapsed() > Duration::from_secs(2) {
                    if let Some(line) = download_hud_line(&c) {
                        println!("[Download] {line}");
                    }
                    last_dl_print = Instant::now();
                }
            }
            // Damage diagnostics: HitDamageType / TakeHitLocation updates on our pawn.
            if let Some(pc) = c.pawn_channel() {
                for e in &evs {
                    if let openkf_network::session::SessionEvent::Updated(ch, ups) = e {
                        if *ch == pc {
                            for u in ups {
                                if let openkf_network::props::Update::Property { name, value, .. } = u {
                                    if name == "HitDamageType" || name == "TakeHitLocation" || name == "Health" {
                                        let v = match value {
                                            Value::Object(i) => c.session.as_ref().map(|s| s.schema.object_name(*i)).unwrap_or_default(),
                                            v => format!("{v:?}"),
                                        };
                                        println!("[Live] {:5.1}s my pawn {name} = {v}", t0.elapsed().as_secs_f32());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Our own weapon sound (the server never sends us our own sounds).
            if local_fire {
                local_fire = false;
                view_anim = Some(("Fire".into(), Instant::now(), 0.0));
                // Muzzle flash at chest height (facing unknown: spherical flash).
                if let Some(me) = c.my_loc {
                    sh.lock().unwrap().particles.burst([me[0], me[1], me[2] + 50.0], 6, [1.0, 0.85, 0.45, 0.9], 120.0, 0.15, 14.0, 0.0);
                }
                if let (Some(au), Some(d), Some(w), Some(me)) = (audio.as_mut(), defaults.as_deref(), weapon, c.my_loc) {
                    let cls = c.session.as_ref().and_then(|s| s.actors.get(&w)).map(|a| a.class.name.clone()).unwrap_or_default();
                    if let Some(snd_name) = fire_sound(d, &cls) {
                        let sc = c.session.as_ref().map(|s| &s.schema);
                        if let Some(idx) = sc.and_then(|s| s.object_index_any(&snd_name)) {
                            let mut idx2 = idx;
                            if let Some(sc) = sc {
                                let m = sc.sound_group_members(idx);
                                if !m.is_empty() { idx2 = m[(t0.elapsed().as_nanos() as usize / 7) % m.len()]; }
                            }
                            let ok = au.play(idx2, || sc.and_then(|s| s.export_data(idx2)), me, 1000.0, 1.0, 1.0);
                            if !ok && sounds_failed < 3 { println!("[Audio] fire sound {snd_name} failed"); }
                        }
                    }
                }
            }
            // Sounds heard by the server for us: PlayerController.ClientHearSound.
            if let Some(au) = audio.as_mut() {
                if let Some(me) = c.my_loc {
                    au.set_listener(me, sh.lock().unwrap().input.map(|i| i.yaw).unwrap_or(0));
                }
                for e in &evs {
                    if let openkf_network::session::SessionEvent::Updated(_, ups) = e {
                        for u in ups {
                            let openkf_network::props::Update::Rpc { name, params } = u else { continue };
                            if name != "ClientHearSound" {
                                continue;
                            }
                            let get = |k: &str| params.iter().find(|p| p.0 == k).map(|p| p.1.clone());
                            let Some(Value::Object(snd)) = get("S") else { continue };
                            let loc = match get("SoundLocation") { Some(Value::Vector(v)) => [v[0] as f32, v[1] as f32, v[2] as f32], _ => c.my_loc.unwrap_or([0.0; 3]) };
                            let (vol, radius, pitch) = match get("Parameters") {
                                Some(Value::Vector(p)) => (p[0] as f32 / 100.0, p[1] as f32, if p[2] == 0 { 1.0 } else { p[2] as f32 / 100.0 }),
                                _ => (1.0, 300.0, 1.0),
                            };
                            let schema = c.session.as_ref().map(|s| &s.schema);
                            // SoundGroup -> one of its sounds at random.
                            let mut snd = snd;
                            if let Some(sc) = schema {
                                let members = sc.sound_group_members(snd);
                                if !members.is_empty() {
                                    snd = members[(t0.elapsed().as_nanos() as usize / 7) % members.len()];
                                }
                            }
                            let r = au.try_play(snd, || schema.and_then(|s| s.export_data(snd)), loc, radius, if vol > 0.0 { vol } else { 1.0 }, pitch);
                            if let Err(why) = r {
                                sounds_failed += 1;
                                if sounds_failed <= 12 {
                                    let what = c.session.as_ref().and_then(|s| s.schema.map.resolve(snd)).map(|o| format!("{}.{} ({})", o.package, o.name.unwrap_or("?"), o.class.unwrap_or("?")));
                                    println!("[Audio] cannot play object {snd}: {what:?} ({why:?})");
                                }
                            }
                        }
                    }
                }
            }
            // Pawn animation state: replicated AnimAction (one-shots), deaths, corpses.
            for e in &evs {
                match e {
                    openkf_network::session::SessionEvent::Updated(ch, ups) => {
                        for u in ups {
                            let openkf_network::props::Update::Property { name: pn, value, .. } = u else { continue };
                            match (pn.as_str(), value) {
                                ("AnimAction", Value::Name(i)) => {
                                    let n = c.session.as_ref().and_then(|s| s.schema.names.get(*i as usize).cloned()).unwrap_or_default();
                                    if n.is_empty() || n.eq_ignore_ascii_case("None") {
                                        anim_actions.remove(ch);
                                    } else {
                                        anim_actions.insert(*ch, (n, Instant::now()));
                                    }
                                }
                                ("Health", Value::Int(h)) if *h <= 0 => {
                                    if died_at.get(ch).is_none() {
                                        died_at.insert(*ch, Instant::now());
                                        // Blood burst + ground decal at the corpse.
                                        if let Some(a) = c.session.as_ref().and_then(|s| s.actors.get(ch)) {
                                            let mut s = sh.lock().unwrap();
                                            s.particles.burst(a.location, 24, [0.55, 0.03, 0.02, 0.95], 260.0, 0.9, 10.0, 1.0);
                                            s.particles.decal([a.location[0], a.location[1], a.location[2] - 30.0], [0.25, 0.01, 0.01, 0.85], 46.0, 12.0);
                                        }
                                    }
                                }
                                ("bTearOff", Value::Bool(true)) => {
                                    if died_at.get(ch).is_none() {
                                        died_at.insert(*ch, Instant::now());
                                        if let Some(a) = c.session.as_ref().and_then(|s| s.actors.get(ch)) {
                                            let mut s = sh.lock().unwrap();
                                            s.particles.burst(a.location, 24, [0.55, 0.03, 0.02, 0.95], 260.0, 0.9, 10.0, 1.0);
                                            s.particles.decal([a.location[0], a.location[1], a.location[2] - 30.0], [0.25, 0.01, 0.01, 0.85], 46.0, 12.0);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    openkf_network::session::SessionEvent::Closed(ch) => {
                        anim_actions.remove(ch);
                        // A pawn closed after dying (torn off): keep its corpse a few seconds.
                        if let (Some(t), Some(p)) = (died_at.remove(ch), last_pawns.iter().find(|p| p.channel == *ch)) {
                            corpses.push((p.clone(), t));
                        }
                    }
                    _ => {}
                }
            }
            corpses.retain(|(_, t)| t.elapsed() < CORPSE_TIME);
            for e in evs {
                if let openkf_network::session::SessionEvent::Failed(ch, err) = &e {
                    if fails_logged < 20 {
                        let cls = c.session.as_ref().and_then(|s| s.actors.get(ch)).map(|a| a.class.name.clone()).unwrap_or_default();
                        println!("[Live] decode fail ch{ch} {cls}: {err}");
                        fails_logged += 1;
                    }
                }
                if let openkf_network::session::SessionEvent::Opened(ch) = e {
                    if let Some(a) = c.session.as_ref().and_then(|s| s.actors.get(&ch)) {
                        if a.class.is_a("Projectile") || std::env::var("OPENKF_LOG_OPENS").is_ok() {
                            println!("[Live] open {} ch{ch} chain {:?} at {:?}", a.class.name, a.class.chain.get(..3), a.location.map(|x| x as i32));
                        }
                    }
                }
            }
            while shown < c.log.len() {
                println!("[Live {:6.2}] {}", t0.elapsed().as_secs_f32(), c.log[shown]);
                shown += 1;
            }
            if ready && !readied && c.state == LiveState::Joined {
                if let Some(ch) = c.controller_channel() {
                    let _ = c.call_rpc(ch, "ServerSetClassicTrans", &[("B", Value::Bool(false))]);
                    let _ = c.call_rpc(ch, "ServerSetReadyToStart", &[]);
                    // Like the official client: we want the trader path (RedWhisp) between waves.
                    let _ = c.call_rpc(ch, "ServerSetWantsTraderPath", &[("bNewWantsTraderPath", Value::Bool(true))]);
                    // Lobby "Ready" button (LobbyFooter.uc): PC.ServerRestartPlayer().
                    if let Err(e) = c.call_rpc(ch, "ServerRestartPlayer", &[]) {
                        println!("[Live] ServerRestartPlayer: {e}");
                    }
                    readied = true;
                }
            }
            // Possession: the server ignores our moves until we acknowledge the pawn
            // (UE2.5 AcknowledgedPawn), like the official client does.
            if readied {
                let pawn_ch = c.session.as_ref().and_then(|s| s.actors.values().filter(|a| a.class.is_a("KFHumanPawn")).map(|a| a.channel).min());
                if let (Some(pc), Some(ctl)) = (pawn_ch, c.controller_channel()) {
                    if acked_pawn != Some(pc) {
                        let r = c.call_rpc(ctl, "ServerAcknowledgePossession", &[("P", Value::ActorChannel(pc)), ("NewHand", Value::Float(1.0)), ("bNewAutoTaunt", Value::Bool(false))]);
                        println!("[Live] ServerAcknowledgePossession(ch{pc}) {r:?}");
                        acked_pawn = Some(pc);
                        c.my_loc = None;
                    }
                }
            }
            let play = sh.lock().unwrap().input;
            // Weapon: once per pawn, switch to the 9mm (class "Single") like the official client.
            // Then upgrade to the best owned gun as soon as one is bought / picked up.
            if acked_pawn.is_some() && (armed_for != acked_pawn || (play.is_none() && last_wcheck.elapsed() > Duration::from_secs(1))) {
                last_wcheck = Instant::now();
                let ws = c.weapons();
                const PREF: [&str; 4] = ["Shotgun", "Winchester", "Deagle", "Single"];
                let best = PREF.iter().find_map(|p| ws.iter().find(|w| w.1 == *p)).map(|w| w.0);
                if let Some(w) = best {
                    if weapon != Some(w) || armed_for != acked_pawn {
                        let old = if armed_for == acked_pawn { weapon } else { None };
                        let r = c.change_weapon(old, w);
                        println!("[Live] weapons {ws:?} -> ServerChangedWeapon(ch{w}) {r:?}");
                        weapon = Some(w);
                        armed_for = acked_pawn;
                    }
                }
            }
            // Kills: replicated by our PlayerReplicationInfo (PlayerName == our name).
            if let Some(s) = &c.session {
                for a in s.actors.values().filter(|a| a.class.is_a("PlayerReplicationInfo")) {
                    if matches!(a.props.get("PlayerName"), Some(Value::Str(n)) if *n == name) {
                        if let Some(Value::Int(k)) = a.props.get("Kills") {
                            kills = *k as u32;
                        }
                    }
                }
            }
            let _ = &mut seen_dead;
            // Reload when the magazine is empty (official client: Weapon.ReloadMeNow).
            if let Some(w) = weapon {
                let mag = match c.session.as_ref().and_then(|s| s.actors.get(&w)).and_then(|a| a.props.get("MagAmmoRemaining")) { Some(Value::Int(m)) => *m, _ => 15 };
                let me_l = c.my_loc.unwrap_or([0.0; 3]);
                let zed_near = c.session.as_ref().is_some_and(|s| s.actors.values().any(|a| a.class.chain.iter().any(|x| x == "KFMonster") && (a.location[0] - me_l[0]).hypot(a.location[1] - me_l[1]) < 800.0));
                let empty = mag == 0 || (mag <= 5 && !zed_near);
                if empty && last_reload.elapsed() > Duration::from_secs(3) {
                    let r = c.call_rpc(w, "ReloadMeNow", &[]);
                    println!("[Live] ReloadMeNow {r:?}");
                    last_reload = Instant::now();
                }
            }
                        if let (Some(_), true) = (play, c.state == LiveState::Joined) {
                if last_move.elapsed() >= Duration::from_millis(33) {
                    // Take one-shot actions only when we actually process them.
                    let inp = {
                        let mut g = sh.lock().unwrap();
                        let p = g.input.unwrap_or_default();
                        if let Some(i) = g.input.as_mut() {
                            i.slot = None;
                            i.grenade = false;
                            i.buy_clip = false;
                            i.buy_shotgun = false;
                        }
                        p
                    };
                    last_move = Instant::now();
                    let _ = c.send_move([inp.wish[0] * 3072.0, inp.wish[1] * 3072.0, 0.0], inp.pitch, inp.yaw);
                    if let Some(w) = weapon {
                        if inp.fire && firing_since.is_none() {
                            if c.fire(w, true, 0).is_ok() {
                                firing_since = Some(Instant::now());
                                shots += 1;
                                local_fire = true;
                            }
                        } else if !inp.fire && firing_since.is_some() {
                            let _ = c.fire(w, false, 0);
                            firing_since = None;
                        }
                        if let Some(slot) = inp.slot {
                            if let Some(nw) = c.weapon_by_class(SLOTS[(slot as usize - 1).min(4)]) {
                                if Some(nw) != weapon {
                                    let r = c.change_weapon(weapon, nw);
                                    println!("[Live] slot {slot} -> ch{nw} {r:?}");
                                    weapon = Some(nw);
                                }
                            }
                        }
                        if inp.grenade && throw_at.is_none() {
                            if let Some(f) = c.weapon_by_class("Frag") {
                                let a = c.call_rpc(f, "ServerStartThrow", &[]);
                                let ammo: Vec<String> = c.session.as_ref().map(|s| s.actors.values().filter(|x| x.class.is_a("Ammunition")).map(|x| format!("{}={:?}", x.class.name, x.props.get("AmmoAmount"))).collect()).unwrap_or_default();
                                println!("[Live] grenade start {a:?} ammo {ammo:?}");
                                throw_at = Some((f, Instant::now()));
                            }
                        }
                        // Official client: ServerThrow ~180 ms after ServerStartThrow.
                        if let Some((f, t)) = throw_at {
                            if t.elapsed() > Duration::from_millis(180) {
                                let b = c.call_rpc(f, "ServerThrow", &[]);
                                let frag = c.session.as_ref().and_then(|s| s.actors.values().find(|x| x.class.name == "FragAmmo")).and_then(|x| x.props.get("AmmoAmount").cloned());
                                println!("[Live] grenade throw {b:?} FragAmmo now {frag:?}");
                                throw_at = None;
                            }
                        }
                        if inp.buy_clip {
                            let cls = c.session.as_ref().and_then(|s| s.actors.get(&w)).map(|a| a.class.name.clone()).unwrap_or_default();
                            let r = c.buy_ammo(&format!("KFMod.{cls}Ammo"), true);
                            println!("[Live] buy clip {cls}Ammo {r:?}");
                        }
                        if inp.buy_shotgun {
                            let r = c.buy_weapon("KFMod.Shotgun", 8.0);
                            println!("[Live] buy Shotgun {r:?}");
                        }
                        if inp.reload && last_reload.elapsed() > Duration::from_secs(1) {
                            let _ = c.call_rpc(w, "ReloadMeNow", &[]);
                            last_reload = Instant::now();
                        }
                    }
                }
            }
            // Trader path: WillowWhisp.WayPoints (server pathfinding to CurrentShop), newest whisp.
            // Navigation graph of the current map (built once per map).
            if let (Some(m), Some(ss)) = (c.map.clone(), c.session.as_ref()) {
                if nav.as_ref().map(|n| n.0 != m).unwrap_or(true) {
                    if let Some(g) = ss.schema.with_level(openkf_package::navigation::read_nav_graph) {
                        println!("[Live] nav graph {m}: {} nodes", g.nodes.len());
                        nav = Some((m, g));
                    }
                }
            }
            // Trade time: A* to GRI.CurrentShop (ShopVolume in the level package).
            let trading = c.session.as_ref().and_then(|ss| ss.actors.values().find(|a| a.class.is_a("GameReplicationInfo")).map(|a| a.props.get("bWaveInProgress").cloned())).flatten() == Some(Value::Bool(false));
            if trading && nav_path.is_empty() && nav_time.elapsed() > Duration::from_secs(2) {
                nav_time = Instant::now();
                let shop = c.session.as_ref().and_then(|ss| {
                    let gri = ss.actors.values().find(|a| a.class.is_a("GameReplicationInfo"))?;
                    let Some(Value::Object(idx)) = gri.props.get("CurrentShop") else { return None };
                    let (slot, e) = ss.schema.locate(*idx)?;
                    if slot != 0 { return None; }
                    // Volume centre = Location - PrePivot (brush box is centred on its pivot).
                    ss.schema.with_level(|p| {
                        let pr = openkf_package::read_object_properties(p, e).ok()?;
                        let Some(openkf_package::PropValue::Vector(v)) = pr.get("Location") else { return None };
                        let pp = match pr.get("PrePivot") { Some(openkf_package::PropValue::Vector(q)) => *q, _ => [0.0; 3] };
                        Some([v[0] - pp[0], v[1] - pp[1], v[2] - pp[2]])
                    }).flatten()
                });
                if let (Some(shop), Some(me), Some((_, g))) = (shop, c.my_loc, nav.as_ref()) {
                    if let Some(mut p) = g.path(me, shop) {
                        p.push([shop[0], shop[1], shop[2]]);
                        if nav_path.is_empty() {
                            println!("[Live] nav path to shop {:?}: {} points", shop.map(|x| x as i32), p.len());
                        }
                        nav_path = p;
                    }
                }
            } else if !trading {
                nav_path.clear();
                buys = 0; // new trader period -> buy again
            }
            let mut path: Vec<[f32; 3]> = c
                .session
                .as_ref()
                .and_then(|s| s.actors.values().filter(|a| a.class.is_a("WillowWhisp")).max_by_key(|a| a.channel).map(|a| {
                    let n = match a.props.get("NumPoints") { Some(Value::Int(n)) => *n as usize, _ => 0 };
                    (0..n.min(16))
                        .filter_map(|i| match a.props.get(&if i == 0 { "WayPoints".to_string() } else { format!("WayPoints[{i}]") }) {
                            Some(Value::Vector(v)) => Some([v[0] as f32, v[1] as f32, v[2] as f32]),
                            _ => None,
                        })
                        .collect()
                }))
                .unwrap_or_default();
            if path.is_empty() && !nav_path.is_empty() {
                path = nav_path.clone();
            }
            if path.len() != path_logged && !path.is_empty() {
                println!("[Live] trader path {} points: {:?}", path.len(), path.iter().map(|p| p.map(|x| x as i32)).collect::<Vec<_>>());
                path_logged = path.len();
            }
            let dosh = c.session.as_ref().and_then(|s| s.actors.values().filter(|a| a.class.is_a("PlayerReplicationInfo")).find(|a| matches!(a.props.get("PlayerName"), Some(Value::Str(n)) if *n == name)).and_then(|a| a.props.get("Score").cloned()));
            // Movement: flee the nearest zed at ~30 Hz (test behaviour).
            if play.is_none() && c.state == LiveState::Joined && last_move.elapsed() >= Duration::from_millis(33) {
                last_move = Instant::now();
                let me0 = c.my_loc.or_else(|| c.session.as_ref().and_then(|s| s.actors.values().find(|a| a.class.is_a("KFHumanPawn")).map(|a| a.location)));
                if let (Some(me), Some(s)) = (me0, c.session.as_ref()) {
                    let zed = s
                        .actors
                        .values()
                        .filter(|a| a.class.chain.iter().any(|x| x == "KFMonster"))
                        .filter(|a| !matches!(a.props.get("Health"), Some(Value::Int(h)) if *h <= 0))
                        .map(|a| a.location)
                        .min_by(|a, b| {
                            let d = |p: &[f32; 3]| (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2);
                            d(a).partial_cmp(&d(b)).unwrap()
                        });
                    let zed_close = zed.is_some_and(|z| (z[0] - me[0]).hypot(z[1] - me[1]) < 700.0);
                    if let (false, Some(_)) = (zed_close, path.last()) {
                        // Follow the path with a persistent cursor (no oscillation between nodes).
                        if path_sig != path.len() * 7919 + path[path.len() - 1][0] as usize {
                            path_sig = path.len() * 7919 + path[path.len() - 1][0] as usize;
                            // start from the point after the closest one
                            let near = (0..path.len()).min_by(|&a, &b| {
                                let d = |p: &[f32; 3]| (p[0] - me[0]).hypot(p[1] - me[1]);
                                d(&path[a]).partial_cmp(&d(&path[b])).unwrap()
                            }).unwrap();
                            path_cursor = (near + 1).min(path.len() - 1);
                        }
                        while path_cursor + 1 < path.len() && (path[path_cursor][0] - me[0]).hypot(path[path_cursor][1] - me[1]) < 100.0 {
                            path_cursor += 1;
                        }
                        let near = path_cursor;
                        let goal = path_cursor;
                        let (dx, dy) = (path[goal][0] - me[0], path[goal][1] - me[1]);
                        let l = dx.hypot(dy).max(1.0);
                        flee_dir = Some([dx / l, dy / l]);
                        turn = 0.0;
                        if dbg_t.elapsed() > Duration::from_secs(2) {
                            dbg_t = Instant::now();
                            println!("[Nav] me {:?} near {near} goal {goal} {:?} dist {:.0}", me.map(|x| x as i32), path[goal].map(|x| x as i32), l);
                        }
                        // At the shop (last waypoint = CurrentShop location): buy a 9mm clip.
                        let shop = path[path.len() - 1];
                        if (shop[0] - me[0]).hypot(shop[1] - me[1]) < 120.0 && last_buy.elapsed() > Duration::from_secs(2) && buys < 5 {
                            // Shotgun first if affordable, then refill its ammo, then the 9mm's.
                            let has_sg = c.weapons().iter().any(|w| w.1 == "Shotgun");
                            let cash = match dosh { Some(Value::Float(f)) => f, _ => 0.0 };
                            let r = if !has_sg && cash >= 500.0 {
                                c.buy_weapon("KFMod.Shotgun", 8.0)
                            } else if has_sg && buys.is_multiple_of(2) {
                                c.buy_ammo("KFMod.ShotgunAmmo", false)
                            } else {
                                c.buy_ammo("KFMod.SingleAmmo", false)
                            };
                            buys += 1;
                            last_buy = Instant::now();
                            println!("[Live] at shop (dist {:.0}), buy #{buys} {r:?}, dosh before {dosh:?}, weapons {:?}", (shop[0] - me[0]).hypot(shop[1] - me[1]), c.weapons().iter().map(|w| w.1.clone()).collect::<Vec<_>>());
                        }
                    } else if let Some(z) = zed {
                        let (dx, dy) = (me[0] - z[0], me[1] - z[1]);
                        let l = (dx * dx + dy * dy).sqrt().max(1.0);
                        flee_dir = Some([dx / l, dy / l]);
                    } else if flee_dir.is_none() {
                        flee_dir = Some([1.0, 0.0]);
                    }
                    // Stuck against a wall for 1 s: rotate the flee direction.
                    if stuck_ref.1.elapsed() > Duration::from_secs(1) {
                        let moved = ((me[0] - stuck_ref.0[0]).powi(2) + (me[1] - stuck_ref.0[1]).powi(2)).sqrt();
                        if moved < 40.0 && !path.is_empty() && last_use.elapsed() > Duration::from_secs(2) {
                            if let Some(ctl) = c.controller_channel() {
                                let r = c.call_rpc(ctl, "ServerUse", &[]);
                                c.jump();
                                println!("[Nav] stuck at {:?}: ServerUse {r:?} + jump", me.map(|x| x as i32));
                            }
                            last_use = Instant::now();
                        }
                        if moved < 40.0 {
                            // pseudo-random new heading (avoid oscillating in corners)
                            let r = (Instant::now().duration_since(t0).as_nanos() % 6283) as f32 / 1000.0;
                            turn += 1.2 + r * 0.6;
                        }
                        stuck_ref = (me, Instant::now());
                    }
                    // Aim at the nearest zed (eye height ~ +50), shoot when in range.
                    let mut aim = None;
                    if let Some(z) = zed {
                        let (dx, dy, dz) = (z[0] - me[0], z[1] - me[1], z[2] - (me[2] + 50.0));
                        let dist = dx.hypot(dy);
                        let ay = (dy.atan2(dx) / std::f32::consts::TAU * 65536.0) as i32 & 0xffff;
                        let ap = (dz.atan2(dist) / std::f32::consts::TAU * 65536.0) as i32 & 0xffff;
                        aim = Some((ap, ay, dist));
                    }
                    if let (Some(w), Some((_, _, dist))) = (weapon, aim) {
                        if dist < 1500.0 && firing_since.is_none() && last_shot.elapsed() > Duration::from_millis(350)
                            && c.fire(w, true, 0).is_ok() {
                                firing_since = Some(Instant::now());
                                shots += 1;
                                local_fire = true;
                            }
                    }
                    if let (Some(w), Some(t)) = (weapon, firing_since) {
                        if t.elapsed() > Duration::from_millis(90) {
                            let _ = c.fire(w, false, 0);
                            firing_since = None;
                            last_shot = Instant::now();
                        }
                    }
                    let d = flee_dir.unwrap();
                    let (sn, cs) = turn.sin_cos();
                    let dir = [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs];
                    let move_yaw = (dir[1].atan2(dir[0]) / std::f32::consts::TAU * 65536.0) as i32 & 0xffff;
                    // No zed in view: sweep the view around (UE2 relevancy depends on what we look at,
                    // looking only where we run hides zeds chasing us).
                    let _ = move_yaw;
                    let scan_yaw = ((t0.elapsed().as_secs_f32() * 0.35).fract() * 65536.0) as i32;
                    let (pitch, yaw) = aim.map(|a| (a.0, a.1)).unwrap_or((0, scan_yaw));
                    if let Err(e) = c.send_move([dir[0] * 3072.0, dir[1] * 3072.0, 0.0], pitch, yaw) {
                        if c.moves_sent == 0 && t0.elapsed().as_secs().is_multiple_of(5) {
                            let _ = e;
                        }
                    }
                }
            }
            // Ambient emitter smoke (throttled) + particle physics tick.
            if emitter_tick.elapsed() > Duration::from_millis(120) {
                emitter_tick = Instant::now();
                if let Some(s) = &c.session {
                    let mut n = 0;
                    for a in s.actors.values().filter(|a| a.class.is_a("Emitter")) {
                        if n >= 8 {
                            break;
                        }
                        sh.lock().unwrap().particles.smoke(a.location, [0.45, 0.44, 0.42, 0.5], 26.0, 2.2);
                        n += 1;
                    }
                }
            }
            {
                let dt = part_tick.elapsed().as_secs_f32();
                part_tick = Instant::now();
                sh.lock().unwrap().particles.update(dt);
                for r in ragdolls.values_mut().filter(|r| !r.settled()) {
                    r.step(dt);
                }
            }
            let mut pawns: Vec<PawnView> = Vec::new();
            if let Some(s) = &c.session {
                for a in s.actors.values() {
                    if !a.class.is_a("Pawn") {
                        continue;
                    }
                    let Some(mi) = mesh_slot(&pool, defaults.as_deref(), &a.class.chain) else { continue };
                    let dead = matches!(a.props.get("Health"), Some(Value::Int(h)) if *h <= 0) || matches!(a.props.get("bTearOff"), Some(Value::Bool(true)));
                    if dead {
                        died_at.entry(a.channel).or_insert_with(Instant::now);
                    }
                    let mine = a.class.is_a("KFHumanPawn") && c.my_loc.is_some();
                    if mine && play.is_some() {
                        continue; // first-person: don't draw ourselves
                    }
                    let loc = if mine { c.my_loc.unwrap() } else { a.location };
                    let v = if mine { c.my_vel } else { a.velocity };
                    let yaw = if mine && v[0].hypot(v[1]) > 10.0 { (v[1].atan2(v[0]) / std::f32::consts::TAU * 65536.0) as i32 } else { a.rotation[1] };
                    let flag = |k: &str| matches!(a.props.get(k), Some(Value::Bool(true)));
                    let (crouched, walking) = (flag("bIsCrouched"), flag("bIsWalking"));
                    let cls = a.class.name.as_str();
                    let d = defaults.as_deref();
                    pawns.push(PawnView {
                        channel: a.channel,
                        mesh: mi,
                        loc,
                        yaw,
                        pitch: 0,
                        roll: 0,
                        ragdoll: false,
                        speed: v[0].hypot(v[1]),
                        move_anim: d.and_then(|d| movement_anim(d, cls, v, yaw, walking, crouched)),
                        idle_anim: d.and_then(|d| idle_anim(d, cls, crouched)),
                        action: anim_actions.get(&a.channel).map(|(n, t)| (n.clone(), t.elapsed().as_secs_f32())),
                        dying: died_at.get(&a.channel).map(|t| t.elapsed().as_secs_f32()),
                        death_anims: d.map(|d| (0..4).filter_map(|i| d.get(cls, &format!("DeathAnim({i})")).map(unquote).filter(|x| !x.is_empty())).collect()).unwrap_or_default(),
                        collision_height: d.and_then(|d| def_f32(d, cls, "CollisionHeight")).unwrap_or(44.0),
                    });
                }
            }
            died_at.retain(|ch, t| t.elapsed() < CORPSE_TIME || pawns.iter().any(|p| p.channel == *ch));
            ragdolls.retain(|ch, _| died_at.contains_key(ch));
            last_pawns = pawns.clone();
            for (p, t) in &corpses {
                if !pawns.iter().any(|q| q.channel == p.channel) {
                    pawns.push(PawnView { dying: Some(t.elapsed().as_secs_f32()), action: None, speed: 0.0, ..p.clone() });
                }
            }
            // Karma ragdolls drive corpses whose class has no death animation.
            for p in pawns.iter_mut().filter(|p| p.dying.is_some() && p.death_anims.is_empty()) {
                let seed = (p.channel as u64).wrapping_mul(1_000_003).wrapping_add(t0.elapsed().as_micros() as u64);
                let r = ragdolls
                    .entry(p.channel)
                    .or_insert_with(|| crate::ragdoll::RagdollState::spawn(p.loc, p.yaw, p.collision_height, seed));
                let (loc, yaw, pitch, roll) = r.pose();
                p.loc = loc;
                p.yaw = yaw;
                p.pitch = pitch;
                p.roll = roll;
                p.ragdoll = true;
            }
            // Movable static meshes: pickups and projectiles.
            let mut props = Vec::new();
            let mut props_hidden = 0usize;
            if let (Some(ss), Some(d)) = (&c.session, defaults.as_deref()) {
                let ppool = prop_pool();
                for a in ss.actors.values() {
                    if a.class.is_a("Pawn") || !(a.class.is_a("Pickup") || a.class.is_a("Projectile")) {
                        continue;
                    }
                    if matches!(a.props.get("bHidden"), Some(Value::Bool(true))) {
                        props_hidden += 1;
                        continue;
                    }
                    let repl = match a.props.get("StaticMesh") {
                        Some(Value::Object(i)) if *i != 0 => Some(ss.schema.object_name(*i)),
                        _ => None,
                    };
                    let cls = a.class.name.as_str();
                    let Some(mi) = prop_slot(ppool, d, cls, repl.as_deref()) else { continue };
                    let ds = match a.props.get("DrawScale") { Some(Value::Float(f)) => *f, _ => def_f32(d, cls, "DrawScale").unwrap_or(1.0) };
                    let ds3 = d.get(cls, "DrawScale3D").and_then(parse_vec).unwrap_or([1.0; 3]);
                    let pre = d.get(cls, "PrePivot").and_then(parse_vec).unwrap_or([0.0; 3]);
                    // Projectiles face their velocity (bNetInitialRotation is often off).
                    let mut rot = a.rotation;
                    let v = a.velocity;
                    if a.class.is_a("Projectile") && (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) > 100.0 && rot == [0, 0, 0] {
                        let yaw = v[1].atan2(v[0]);
                        let pitch = v[2].atan2(v[0].hypot(v[1]));
                        rot = [(pitch / std::f32::consts::TAU * 65536.0) as i32, (yaw / std::f32::consts::TAU * 65536.0) as i32, 0];
                    }
                    props.push(PropView { mesh: mi, loc: a.location, rot, scale: [ds * ds3[0], ds * ds3[1], ds * ds3[2]], pre_pivot: pre });
                }
            }
            // Held weapons: WeaponAttachment actors based on a visible pawn, on AttachmentBone.
            let mut attachments = Vec::new();
            if let Some(ss) = &c.session {
                for a in ss.actors.values().filter(|a| a.class.is_a("WeaponAttachment")) {
                    let Some(Value::ActorChannel(base)) = a.props.get("Base") else { continue };
                    let Some(parent) = pawns.iter().position(|p| p.channel == *base) else { continue };
                    let bone = match a.props.get("AttachmentBone") {
                        Some(Value::Name(i)) => ss.schema.names.get(*i as usize).cloned().unwrap_or_default(),
                        _ => continue,
                    };
                    if let Some(mi) = mesh_slot(&pool, defaults.as_deref(), &a.class.chain) {
                        attachments.push(AttachView { mesh: mi, parent, bone });
                    }
                }
            }
            // First-person weapon: mesh + PlayerViewOffset of the held weapon's class.
            let view_weapon = (|| {
                let d = defaults.as_deref()?;
                let w = weapon?;
                let cls = c.session.as_ref()?.actors.get(&w)?.class.name.clone();
                let (_, pkg, mesh) = d.object_ref(&cls, "Mesh")?;
                let slot = pool.iter().position(|p| p.0.eq_ignore_ascii_case(&pkg) && p.1.eq_ignore_ascii_case(&mesh))?;
                let raw = d.get(&cls, "PlayerViewOffset").and_then(parse_vec).unwrap_or([20.0, 10.0, -10.0]);
                // Pawn.CalcDrawOffset: offset * (0.9 / DisplayFOV * 100), rotated by the view.
                let dfov = d.get(&cls, "DisplayFOV").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(90.0).max(1.0);
                let k = 0.9 / dfov * 100.0;
                let offset = [raw[0] * k, raw[1] * k, raw[2] * k];
                if last_reload != anim_reload_seen {
                    anim_reload_seen = last_reload;
                    view_anim = Some(("Reload".into(), last_reload, 0.0));
                }
                // Resolve names/rates from the class defaults (Weapon.ReloadAnim, WeaponFire.FireAnim).
                let anim = view_anim.as_ref().map(|(n, t, _)| {
                    let un = |v: &str| v.trim().trim_matches('"').to_string();
                    let fm = d.get(&cls, "FireModeClass(0)").and_then(|f| f.split('\'').nth(1).and_then(|x| x.rsplit('.').next()).map(str::to_string));
                    let (name, rate) = if n == "Reload" {
                        (d.get(&cls, "ReloadAnim").map(un).unwrap_or_else(|| "Reload".into()), d.get(&cls, "ReloadAnimRate").and_then(|r| r.trim().parse().ok()).unwrap_or(1.0))
                    } else {
                        let fm = fm.unwrap_or_default();
                        (d.get(&fm, "FireAnim").map(un).unwrap_or_else(|| "Fire".into()), d.get(&fm, "FireAnimRate").and_then(|r| r.trim().parse().ok()).unwrap_or(1.0))
                    };
                    (name, t.elapsed().as_secs_f32(), rate)
                });
                Some(ViewWeapon { mesh: slot, offset, display_fov: dfov, anim })
            })();
            {
                let mut s = sh.lock().unwrap();
                s.view_weapon = view_weapon;
                s.attachments = attachments;
                s.props = props.clone();
                s.map = c.map.clone();
                s.me = c.my_loc;
                let prop = |pred: &dyn Fn(&openkf_network::session::ActorState) -> bool, key: &str| -> Option<Value> {
                    c.session.as_ref()?.actors.values().filter(|a| pred(a)).find_map(|a| a.props.get(key).cloned())
                };
                let int = |v: Option<Value>| match v { Some(Value::Int(i)) => i.to_string(), Some(Value::Byte(b)) => b.to_string(), _ => "?".into() };
                // Values equal to class defaults are never replicated: Health 100, WaveNumber 0.
                let has_pawn = c.pawn_channel().is_some();
                let health = if has_pawn { int(prop(&|a| a.class.is_a("KFHumanPawn"), "Health").or(Some(Value::Int(100)))) } else { "-".into() };
                let wave = match prop(&|a| a.class.is_a("GameReplicationInfo"), "WaveNumber") {
                    Some(Value::Int(w)) => (w + 1).to_string(),
                    Some(Value::Byte(w)) => (w + 1).to_string(),
                    _ => "1".into(),
                };
                let ammo = int(weapon.and_then(|w| c.session.as_ref()?.actors.get(&w)?.props.get("MagAmmoRemaining").cloned()));
                let nades = int(prop(&|a| a.class.name == "FragAmmo", "AmmoAmount"));
                let near_zed = c.session.as_ref().and_then(|ss| {
                    let me = c.my_loc?;
                    ss.actors.values().filter(|a| a.class.chain.iter().any(|x| x == "KFMonster")).map(|a| ((a.location[0] - me[0]).hypot(a.location[1] - me[1]) as i32, a.class.name.clone(), a.props.get("Health").cloned())).min_by_key(|x| x.0)
                });
                let zeds_total = c.session.as_ref().map_or(0, |ss| ss.actors.values().filter(|a| a.class.chain.iter().any(|x| x == "KFMonster")).count());
                let gri = |k: &str| c.session.as_ref().and_then(|ss| ss.actors.values().find(|a| a.class.is_a("GameReplicationInfo")).and_then(|a| a.props.get(k).cloned()));
                let mypawn = |k: &str| c.session.as_ref().and_then(|ss| c.pawn_channel().and_then(|pc| ss.actors.get(&pc)).and_then(|a| a.props.get(k).cloned()));
                let audio_s = audio.as_ref().map(|a| format!("sons {} (echecs {sounds_failed})", a.played)).unwrap_or_default();
                s.status_extra = format!(
                    "{audio_s} | props {}/{} hidden | {}",
                    props.len(),
                    props_hidden,
                    format!(
                    "zeds {zeds_total} nearest {near_zed:?} | GRI wave_in_progress {:?} next {:?} | pawn phys {:?} tearoff {:?}",
                    gri("bWaveInProgress"), gri("TimeToNextWave"), mypawn("Physics"), mypawn("bTearOff")
                ));
                let dosh_s = match &dosh { Some(Value::Float(f)) => format!("{f:.0}"), _ => "?".into() };
                s.hud = format!("HP {health} | AMMO {ammo} | GRENADES {nades} | KILLS {kills} | WAVE {wave} | {dosh_s} POUNDS");
                // While downloads hold the handshake, the HUD shows progress instead.
                if c.downloading() {
                    if let Some(dl) = download_hud_line(&c) {
                        s.hud = dl;
                    }
                }
                s.pawns = pawns;
                let actors = c.session.as_ref().map_or(0, |s| s.actors.len());
                s.status = format!(
                    "{} | {:?} {} decode ok/fail {}/{}, {} actors, {} pawns, moves {} shots {} kills {} ammo {:?} me {:?}",
                    s.status_extra,
                    c.state,
                    c.map.clone().unwrap_or_default(),
                    c.session.as_ref().map_or(0, |s| s.stats.bunches_ok),
                    c.session.as_ref().map_or(0, |s| s.stats.bunches_failed),
                    actors,
                    s.pawns.len(),
                    c.moves_sent,
                    shots,
                    kills,
                    weapon.and_then(|w| c.session.as_ref()?.actors.get(&w)?.props.get("MagAmmoRemaining").cloned()),
                    c.my_loc.map(|l| l.map(|x| x as i32))
                );
                if c.state == LiveState::Failed && s.map.is_none() {
                    s.failed = true;
                    return;
                }
            }
            // Map change / silent server / failure after join: reconnect from scratch.
            if c.state == LiveState::Traveling || c.state == LiveState::Failed || (c.state == LiveState::Joined && c.silent_for(6.0)) {
                println!("[Live] reconnect (state {:?}, silent {}) ...", c.state, c.silent_for(6.0));
                std::thread::sleep(Duration::from_secs(3));
                match with_defaults(connect(&auth)) {
                    Ok(n) => {
                        c = n;
                        shown = 0;
                        readied = false;
                        acked_pawn = None;
                        armed_for = None;
                        weapon = None;
                        firing_since = None;
                        flee_dir = None;
                        let mut s = sh.lock().unwrap();
                        s.map = None;
                        s.pawns.clear();
                        s.reconnects += 1;
                    }
                    Err(e) => println!("[Live] reconnect failed: {e}"),
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    shared
}
