//! OpenKF map viewer.
//!
//! Usage:
//!   openkf-viewer <map.rom> [--kf <KillingFloor dir>]
//!   openkf-viewer <map.rom> --screenshot out.png [--size 1600x900] [--cam x,y,z,yawdeg,pitchdeg]
//!
//!   openkf-viewer --connect 127.0.0.1:7717 [--play] [--auth steam|captures/capture_X.txt] [--name OpenKF] [--no-ready]
//!   (--play: ZQSD/WASD move, mouse aim, left click fire, R reload; first-person camera)
//!   openkf-viewer --replay captures/capture_X.txt [--speed 1.0]   (map taken from the capture)
//!
//! Controls: WASD / ZQSD move, mouse look (click to capture), Shift = fast,
//! Space/C up/down, Esc = release mouse / quit.
//! Replay: P pause, R restart, F follow the player's pawn, +/- speed.

mod assets;
mod browser;
mod camera;
mod particles;
mod ragdoll;
mod render;
mod replay;
mod live_view;
mod props;
mod hud;
mod audio;
mod scene;
mod skin;
mod zeds;

use camera::Camera;
use glam::Vec3;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

struct Args {
    map: PathBuf,
    kf: PathBuf,
    screenshot: Option<PathBuf>,
    size: (u32, u32),
    cam: Option<[f32; 5]>,
    zeds: bool,
    time: f32,
    replay: Option<PathBuf>,
    speed: f32,
    connect: Option<String>,
    auth: PathBuf,
    name: String,
    ready: bool,
    play: bool,
    browse: bool,
    bench: usize,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        map: PathBuf::new(),
        kf: PathBuf::from(r"E:\SteamLibrary\steamapps\common\KillingFloor"),
        screenshot: None,
        size: (1600, 900),
        cam: None,
        zeds: true,
        time: 0.0,
        replay: None,
        speed: 1.0,
        connect: None,
        auth: PathBuf::from("steam"),
        name: "OpenKF".into(),
        ready: true,
        play: false,
        browse: false,
        bench: 0,
    };
    while let Some(x) = it.next() {
        match x.as_str() {
            "--kf" => a.kf = PathBuf::from(it.next().expect("--kf <dir>")),
            "--screenshot" => a.screenshot = Some(PathBuf::from(it.next().expect("--screenshot <png>"))),
            "--size" => {
                let s = it.next().expect("--size WxH");
                let (w, h) = s.split_once('x').expect("WxH");
                a.size = (w.parse().unwrap(), h.parse().unwrap());
            }
            "--cam" => {
                let v: Vec<f32> = it.next().expect("--cam x,y,z,yaw,pitch").split(',').map(|s| s.parse().unwrap()).collect();
                a.cam = Some([v[0], v[1], v[2], v[3], v[4]]);
            }
            "--no-zeds" => a.zeds = false,
            "--replay" => a.replay = Some(PathBuf::from(it.next().expect("--replay <capture.txt>"))),
            "--connect" => a.connect = Some(it.next().expect("--connect host:port")),
            "--browse" => a.browse = true,
            "--bench" => a.bench = it.next().expect("--bench <frames>").parse().unwrap(),
            "--auth" => a.auth = PathBuf::from(it.next().expect("--auth <capture>")),
            "--name" => a.name = it.next().expect("--name <player>"),
            "--no-ready" => a.ready = false,
            "--play" => a.play = true,
            "--speed" => a.speed = it.next().expect("--speed <x>").parse().unwrap(),
            "--time" => a.time = it.next().expect("--time <seconds>").parse().unwrap(),
            _ => a.map = PathBuf::from(x),
        }
    }
    a
}

fn resolve_map(mut a: Args, replay_map: Option<&str>) -> Args {
    if a.map.as_os_str().is_empty() {
        if let Some(m) = replay_map {
            a.map = PathBuf::from(m);
        }
    }
    if a.map.as_os_str().is_empty() {
        eprintln!("usage: openkf-viewer <map.rom> [--kf <dir>] [--screenshot out.png]");
        std::process::exit(1);
    }
    // Accept a bare map name: KF-WestLondon
    if !a.map.exists() {
        let fname = a.map.with_extension("rom");
        let fname = fname.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        if let Some(p) = openkf_network::download::find_package_file(&a.kf, &fname) {
            a.map = p;
        }
    }
    a
}

/// First-person weapon: eye (pawn + 50) + view-rotated PlayerViewOffset, oriented like the view.
fn place_view_weapon(a: &mut zeds::SkelActor, me: [f32; 3], yaw: f32, pitch: f32, off: [f32; 3], display_fov: f32, anim: Option<&(String, f32, f32)>) {
    let (cy, sy, cp, sp) = (yaw.cos(), yaw.sin(), pitch.cos(), pitch.sin());
    let fwd = [cp * cy, cp * sy, sp];
    let right = [-sy, cy, 0.0];
    let up = [-sp * cy, -sp * sy, cp];
    let eye = [me[0], me[1], me[2] + 44.0]; // KFHumanPawn.EyeHeight
    a.location = [
        eye[0] + fwd[0] * off[0] + right[0] * off[1] + up[0] * off[2],
        eye[1] + fwd[1] * off[0] + right[1] * off[1] + up[1] * off[2],
        eye[2] + fwd[2] * off[0] + right[2] * off[1] + up[2] * off[2],
    ];
    a.yaw = ((yaw / std::f32::consts::TAU) * 65536.0) as i32;
    a.pitch = ((pitch / std::f32::consts::TAU) * 65536.0) as i32;
    a.seq = a.idle_seq;
    a.anim_time = None;
    // One-shot fire / reload: play it once (sequence time = elapsed x rate), then back to idle.
    if let (Some((name, t, rate)), Some(an)) = (anim, a.rig.anim.as_ref()) {
        if let Some(i) = an.sequences.iter().position(|s| s.name.eq_ignore_ascii_case(name)) {
            let sq = &an.sequences[i];
            let dur = sq.num_frames as f32 / sq.rate.max(1.0);
            let st = t * rate.max(0.1);
            if st < dur {
                a.seq = i;
                a.anim_time = Some(st);
            }
        }
    }
    a.attach_to = None;
    // Drawn with DisplayFOV instead of the 90-degree view FOV: lateral magnification.
    let lat = 1.0 / (display_fov.to_radians() * 0.5).tan();
    a.view_shrink = Some(zeds::ViewShrink { eye, fwd, k: 0.2, lat });
}

/// The map could not be opened (not installed and not downloadable): clean exit.
fn fatal_map(map: &std::path::Path, e: &str) -> ! {
    eprintln!("[Scene] cannot load map {}: {e}", map.display());
    eprintln!("[Scene] the map is neither installed nor downloadable from the server (FastDL)");
    std::process::exit(2);
}

fn initial_camera(scene: &scene::Scene, args: &Args) -> Camera {
    match args.cam {
        // UE coordinates + degrees for convenience.
        Some(c) => Camera {
            pos: Vec3::new(c[0], c[2], c[1]),
            yaw: c[3].to_radians(),
            pitch: c[4].to_radians(),
            fov_y: 75f32.to_radians(),
        },
        None => Camera { pos: Vec3::from(scene.spawn_pos), yaw: scene.spawn_yaw, pitch: 0.0, fov_y: 75f32.to_radians() },
    }
}

fn instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12 | wgpu::Backends::METAL,
        ..Default::default()
    })
}

fn device_for(adapter: &wgpu::Adapter) -> (wgpu::Device, wgpu::Queue) {
    let info = adapter.get_info();
    println!("[GPU] {} ({:?}, {:?})", info.name, info.backend, info.device_type);
    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("openkf"),
            required_features: render::required_features(adapter),
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("request_device")
}

// ------------------------------------------------------------------ headless
fn screenshot(args: &Args, rp: Option<&replay::Replay>, live: Option<&std::sync::Arc<std::sync::Mutex<live_view::Shared>>>, live_pool: Vec<(String, String, usize)>) {
    let inst = instance();
    let adapter = pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("no GPU adapter");
    let (device, queue) = device_for(&adapter);
    let bc = device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
    let t = Instant::now();
    let mut scene = scene::load_scene(&args.kf, &args.map, bc, args.zeds, rp.map(|r| r.pool.as_slice()).or(live.map(|_| live_pool.as_slice()))).unwrap_or_else(|e| fatal_map(&args.map, &e));
    println!("[Scene] {}", scene.stats);
    println!("[SceneLoad] {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let mut cam_override = None;
    if let Some(rp) = rp {
        // --time = seconds since replay start; camera above the player's pawn.
        let t = rp.start_ms + (args.time * 1000.0) as u64;
        let mut base = Vec::new();
        let mut acc = 0;
        for p in &rp.pool { base.push(acc); acc += p.2; }
        let mut used = vec![0usize; rp.pool.len()];
        for tr in &rp.tracks {
            let Some(s) = tr.at(t) else { continue };
            if used[tr.mesh] >= rp.pool[tr.mesh].2 { continue; }
            let a = &mut scene.actors[base[tr.mesh] + used[tr.mesh]];
            used[tr.mesh] += 1;
            a.location = s.loc;
            a.yaw = s.yaw;
            a.seq = if s.speed > 40.0 { a.move_seq } else { a.idle_seq };
            println!("[Replay] t={t} {} at {:?} yaw {} speed {:.0}", tr.class, s.loc.map(|x| x as i32), s.yaw, s.speed);
            if tr.is_player && args.cam.is_none() {
                let yaw = s.yaw as f32 * std::f32::consts::TAU / 65536.0;
                cam_override = Some(Camera { pos: Vec3::new(s.loc[0] - yaw.cos() * 300.0, s.loc[2] + 160.0, s.loc[1] - yaw.sin() * 300.0), yaw, pitch: -0.3, fov_y: 75f32.to_radians() });
            }
        }
    }
    if let Some(sh) = live {
        // --time = seconds to stay connected before the shot.
        let t = Instant::now();
        let mut step = 0;
        while t.elapsed().as_secs_f32() < args.time {
            std::thread::sleep(Duration::from_millis(500));
            if args.play && std::env::var("OPENKF_NO_SCRIPT").is_err() {
                // Scripted actions (no keyboard in screenshot mode).
                let el = t.elapsed().as_secs_f32();
                let mut g = sh.lock().unwrap();
                if let Some(i) = g.input.as_mut() {
                    let plan: [(f32, u8); 4] = [(8.0, 1), (10.0, 2), (12.0, 3), (13.0, 4)];
                    while step < plan.len() && el >= plan[step].0 {
                        match plan[step].1 {
                            1 => i.slot = Some(1),
                            2 => i.slot = Some(2),
                            3 => i.grenade = true,
                            _ => i.buy_clip = true,
                        }
                        println!("[Script] {el:.1}s action {}", plan[step].1);
                        step += 1;
                    }
                }
            }
            if let (true, Ok(v)) = (args.play, std::env::var("OPENKF_SHOT_ACTION")) {
                // Debug: hold reload / fire during the last second before the shot.
                if t.elapsed().as_secs_f32() > args.time - 1.0 {
                    if let Some(i) = sh.lock().unwrap().input.as_mut() {
                        if v == "reload" { i.reload = true } else { i.fire = true }
                    }
                }
            }
            let s = sh.lock().unwrap();
            println!("[Live] {:5.1}s {} | {}", t.elapsed().as_secs_f32(), s.hud, s.status);
        }
        let s = sh.lock().unwrap();
        let mut base = Vec::new();
        let mut acc = 0;
        for p in &live_pool { base.push(acc); acc += p.2; }
        let mut used = vec![0usize; live_pool.len()];
        let mut sum = [0.0f32; 3];
        let mut n = 0.0;
        let mut slot_of = vec![usize::MAX; s.pawns.len()];
        for (pi, p) in s.pawns.iter().enumerate() {
            if used[p.mesh] >= live_pool[p.mesh].2 { continue; }
            let idx = base[p.mesh] + used[p.mesh];
            slot_of[pi] = idx;
            let a = &mut scene.actors[idx];
            used[p.mesh] += 1;
            a.location = p.loc;
            a.yaw = p.yaw;
            a.pitch = p.pitch;
            a.roll = p.roll;
            a.apply_pawn_anim(p);
            println!("[Live] pawn {} at {:?} speed {:.0} anim {} action {:?} dying {:?}", live_pool[p.mesh].1, p.loc.map(|x| x as i32), p.speed, a.rig.anim.as_ref().and_then(|x| x.sequences.get(a.seq)).map(|q| q.name.clone()).unwrap_or_default(), p.action, p.dying);
            for (k, v) in sum.iter_mut().enumerate() { *v += p.loc[k]; }
            n += 1.0;
        }
        for at in &s.attachments {
            let parent = slot_of.get(at.parent).copied().unwrap_or(usize::MAX);
            if parent == usize::MAX || used[at.mesh] >= live_pool[at.mesh].2 { continue; }
            let a = &mut scene.actors[base[at.mesh] + used[at.mesh]];
            used[at.mesh] += 1;
            a.attach_to = Some((parent, at.bone.clone()));
            a.location = [0.0; 3];
            println!("[Live] attachment {} on {} bone {}", live_pool[at.mesh].1, scene.actors[parent].name, at.bone);
        }
        props::apply_live(&mut scene.props, &s.props, live_view::prop_pool());
        for p in scene.props.iter().filter(|p| p.visible) {
            println!("[Live] prop {} at {:?} rot {:?} scale {:?}", p.name, p.location.map(|x| x as i32), p.rotation, p.scale);
        }
        if let (true, Some(me)) = (args.play, s.me) {
            // First-person shot: eye + held weapon.
            let yaw = s.input.map(|i| i.yaw).unwrap_or(0) as f32 * std::f32::consts::TAU / 65536.0;
            if let Some(vw) = s.view_weapon.clone() {
                if used[vw.mesh] < live_pool[vw.mesh].2 {
                    let a = &mut scene.actors[base[vw.mesh] + used[vw.mesh]];
                    used[vw.mesh] += 1;
                    place_view_weapon(a, me, yaw, 0.0, vw.offset, vw.display_fov, vw.anim.as_ref());
                    println!("[Live] view weapon {} offset {:?} anim {:?} seq {:?}", live_pool[vw.mesh].1, vw.offset, vw.anim, a.rig.anim.as_ref().and_then(|x| x.sequences.get(a.seq)).map(|q| q.name.clone()));
                }
            }
            // UE FOV is horizontal (KF default 90) -> vertical for 16:9.
            cam_override = Some(Camera { pos: Vec3::new(me[0], me[2] + 44.0, me[1]), yaw, pitch: 0.0, fov_y: 2.0 * (9.0f32 / 16.0).atan() });
            if std::env::var("OPENKF_FP_SIDE").is_ok() {
                // Debug: look at the view weapon from the left side (UE -Y of the view).
                let (cy, sy) = (yaw.cos(), yaw.sin());
                let p = [me[0] + cy * 40.0 + sy * 120.0, me[1] + sy * 40.0 - cy * 120.0, me[2] + 30.0];
                cam_override = Some(Camera { pos: Vec3::new(p[0], p[2], p[1]), yaw: yaw + std::f32::consts::FRAC_PI_2, pitch: -0.1, fov_y: 1.0 });
            }
        } else         if let (Ok(_), Some(me)) = (std::env::var("OPENKF_SHOT_CLOSEUP"), s.pawns.iter().find(|p| live_pool[p.mesh].1 == "British_Soldier1")) {
            // Close-up on our soldier, from his front-right, looking at the torso.
            let yaw = me.yaw as f32 * std::f32::consts::TAU / 65536.0 + 2.4;
            let eye = [me.loc[0] + yaw.cos() * 170.0, me.loc[1] + yaw.sin() * 170.0, me.loc[2] + 30.0];
            let look = (me.loc[1] - eye[1]).atan2(me.loc[0] - eye[0]);
            cam_override = Some(Camera { pos: Vec3::new(eye[0], eye[2], eye[1]), yaw: look, pitch: -0.12, fov_y: 60f32.to_radians() });
        } else if n > 0.0 && args.cam.is_none() {
            let c = [sum[0] / n, sum[1] / n, sum[2] / n];
            // Behind our soldier (if any), looking at the other pawns.
            let me = s.pawns.iter().find(|p| live_pool[p.mesh].1 == "British_Soldier1").map(|p| p.loc);
            let others: Vec<[f32; 3]> = s.pawns.iter().filter(|p| live_pool[p.mesh].1 != "British_Soldier1").map(|p| p.loc).collect();
            let target = if others.is_empty() { c } else {
                let k = others.len() as f32;
                [others.iter().map(|o| o[0]).sum::<f32>() / k, others.iter().map(|o| o[1]).sum::<f32>() / k, others.iter().map(|o| o[2]).sum::<f32>() / k]
            };
            let eye_base = me.unwrap_or(c);
            let (dx, dy) = (target[0] - eye_base[0], target[1] - eye_base[1]);
            let yaw = if dx.hypot(dy) > 1.0 { dy.atan2(dx) } else { 0.0 };
            // High plunging view over the middle of the scene (walls don't block it).
            let mid = [(eye_base[0] + target[0]) / 2.0, (eye_base[1] + target[1]) / 2.0, eye_base[2].max(target[2])];
            let span = dx.hypot(dy).max(600.0);
            let back = span * 0.35;
            let pos = [mid[0] - yaw.cos() * back, mid[1] - yaw.sin() * back, mid[2] + span * 0.9 + 300.0];
            cam_override = Some(Camera { pos: Vec3::new(pos[0], pos[2], pos[1]), yaw, pitch: -1.1, fov_y: 80f32.to_radians() });
        }
    }
    let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut r = render::Renderer::new(&device, &queue, fmt, &scene);
    println!("[Load] total {:.0} ms (BC passthrough: {bc})", t.elapsed().as_secs_f64() * 1e3);
    let (w, h) = args.size;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shot"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: fmt,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let cam = cam_override.unwrap_or_else(|| initial_camera(&scene, args));
    let t = Instant::now();
    let verts = zeds::skin_all(&mut scene.actors, args.time);
    let skin_ms = t.elapsed().as_secs_f64() * 1e3;
    r.update_actors(&queue, &verts);
    if !scene.props.is_empty() {
        let pv = props::transform_all(&scene.props);
        r.update_props(&queue, &pv);
        println!("[Props] {}/{} pool slots visible", scene.props.iter().filter(|p| p.visible).count(), scene.props.len());
    }
    for (a, v) in scene.actors.iter().zip(&verts) {
        let mut mn = [f32::MAX; 3];
        let mut mx = [f32::MIN; 3];
        let mut nan = 0;
        for x in v {
            if x.pos.iter().any(|c| !c.is_finite()) { nan += 1; continue; }
            for k in 0..3 { mn[k] = mn[k].min(x.pos[k]); mx[k] = mx[k].max(x.pos[k]); }
        }
        let mapped = a.rig.bone_map.iter().filter(|b| b.is_some()).count();
        println!("[Actor] {} playing {} | {} verts, {nan} NaN, bones mapped {mapped}/{}, extent {:?}..{:?}", a.name, a.seq_name, v.len(), a.rig.mesh.bones.len(), mn.map(|x| x as i32), mx.map(|x| x as i32));
    }
    println!("[Skin] {} actors skinned in {skin_ms:.2} ms (parallel)", scene.actors.len());
    // Benchmark: orbit the camera 360 degrees over N frames, timing each.
    if args.bench > 0 {
        // Warm up (pipeline + caches) before measuring.
        r.render(&device, &queue, &view, w, h, &cam);
        device.poll(wgpu::Maintain::Wait);
        let mut dts: Vec<f64> = Vec::with_capacity(args.bench);
        let total = Instant::now();
        for i in 0..args.bench {
            let mut c = Camera { pos: cam.pos, yaw: cam.yaw + i as f32 * std::f32::consts::TAU / args.bench as f32, pitch: cam.pitch, fov_y: cam.fov_y };
            let f0 = Instant::now();
            let vv = zeds::skin_all(&mut scene.actors, args.time + i as f32 / 60.0);
            r.update_actors(&queue, &vv);
            if !scene.props.is_empty() {
                let pv = props::transform_all(&scene.props);
                r.update_props(&queue, &pv);
            }
            r.render(&device, &queue, &view, w, h, &c);
            device.poll(wgpu::Maintain::Wait);
            dts.push(f0.elapsed().as_secs_f64() * 1e3);
            let _ = &mut c;
        }
        dts.sort_by(|a, b| a.total_cmp(b));
        let sum: f64 = dts.iter().sum();
        let avg = sum / dts.len() as f64;
        let p50 = dts[dts.len() / 2];
        let p95 = dts[(dts.len() as f64 * 0.95) as usize];
        println!(
            "[Bench] {} frames in {:.1} s: {:.0} FPS avg | frame ms avg {avg:.2} p50 {p50:.2} p95 {p95:.2} | {} draws {} tris",
            args.bench,
            total.elapsed().as_secs_f64(),
            1000.0 / avg,
            r.draw_calls(),
            r.triangles
        );
    }
    r.render(&device, &queue, &view, w, h, &cam);
    if let Some(sh) = live {
        let s = sh.lock().unwrap();
        if !s.particles.is_empty() {
            if let Some(depth) = r.depth_texture() {
                let pr = particles::ParticleRenderer::new(&device, fmt, 2048);
                pr.draw(&device, &queue, &view, depth, &cam, w as f32 / h.max(1) as f32, &s.particles.parts);
                println!("[Particles] {} live", s.particles.len());
            }
        }
        let mut hud = hud::Hud::new(&device, &queue, fmt);
        hud.draw(&device, &queue, &view, w, h, &[s.hud.clone(), format!("OPENKF LIVE  {}", args.map.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())], args.play);
    }
    let row = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::ImageCopyBuffer { buffer: &buf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    println!("[Frame] {:.1} ms, {} draw calls, {} triangles", t.elapsed().as_secs_f64() * 1e3, r.draw_calls(), r.triangles);
    let Some(path) = args.screenshot.as_ref() else { return };
    let data = buf.slice(..).get_mapped_range();
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        px.extend_from_slice(&data[(y * row) as usize..(y * row + w * 4) as usize]);
    }
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&px).unwrap();
    println!("[Shot] {}", path.display());
}

// ------------------------------------------------------------------ windowed
struct Gfx {
    actors: Vec<zeds::SkelActor>,
    props: Vec<props::PropActor>,
    start: Instant,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: render::Renderer,
    hud: hud::Hud,
    particles: particles::ParticleRenderer,
}

struct App {
    args: Args,
    live: Option<std::sync::Arc<std::sync::Mutex<live_view::Shared>>>,
    live_pool: Vec<(String, String, usize)>,
    replay: Option<replay::Replay>,
    replay_ms: f64,
    paused: bool,
    follow: bool,
    gfx: Option<Gfx>,
    cam: Option<Camera>,
    keys: HashSet<KeyCode>,
    fire_held: bool,
    /// Keys pressed since last frame (edge-triggered actions).
    pressed: HashSet<KeyCode>,
    captured: bool,
    last: Instant,
    fps_t: Instant,
    frames: u32,
    title: String,
    title_extra: String,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(Window::default_attributes().with_title("OpenKF Viewer").with_inner_size(winit::dpi::PhysicalSize::new(self.args.size.0, self.args.size.1)))
                .unwrap(),
        );
        let inst = instance();
        let surface = inst.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no GPU adapter");
        let (device, queue) = device_for(&adapter);
        let bc = device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let t = Instant::now();
        let scene = scene::load_scene(&self.args.kf, &self.args.map, bc, self.args.zeds, self.replay.as_ref().map(|r| r.pool.as_slice()).or(self.live.as_ref().map(|_| self.live_pool.as_slice()))).unwrap_or_else(|e| fatal_map(&self.args.map, &e));
        println!("[Scene] {}", scene.stats);
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        // Uncapped: Immediate if available (Mailbox otherwise), like the game's presentInterval=0.
        let present_mode = [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox]
            .into_iter()
            .find(|m| caps.present_modes.contains(m))
            .unwrap_or(wgpu::PresentMode::Fifo);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let renderer = render::Renderer::new(&device, &queue, format, &scene);
        println!("[Load] total {:.0} ms, present mode {:?}", t.elapsed().as_secs_f64() * 1e3, present_mode);
        self.title = scene.name.clone();
        self.cam = Some(initial_camera(&scene, &self.args));
        if let Some(rp) = &self.replay {
            self.replay_ms = rp.start_ms as f64;
            if let Some(s) = rp.tracks.iter().find(|t| t.is_player).and_then(|t| t.samples.first()) {
                let c = self.cam.as_mut().unwrap();
                c.pos = Vec3::new(s.loc[0], s.loc[2] + 400.0, s.loc[1]);
                c.pitch = -0.6;
            }
        }
        let mut scene = scene;
        let actors = std::mem::take(&mut scene.actors);
        let props = std::mem::take(&mut scene.props);
        let hud = hud::Hud::new(&device, &queue, format);
        let parts = particles::ParticleRenderer::new(&device, format, 2048);
        self.gfx = Some(Gfx { actors, props, start: Instant::now(), window, surface, device, queue, config, renderer, hud, particles: parts });
        self.last = Instant::now();
        self.fps_t = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(g) = self.gfx.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                g.config.width = s.width.max(1);
                g.config.height = s.height.max(1);
                g.surface.configure(&g.device, &g.config);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } if self.captured && self.args.play => {
                self.fire_held = state == ElementState::Pressed;
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let ok = g.window.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| g.window.set_cursor_grab(CursorGrabMode::Confined)).is_ok();
                g.window.set_cursor_visible(!ok);
                self.captured = ok;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(k) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        match k {
                            KeyCode::KeyP => self.paused = !self.paused,
                            KeyCode::KeyR => self.replay_ms = self.replay.as_ref().map_or(0.0, |r| r.start_ms as f64),
                            KeyCode::KeyF => self.follow = !self.follow,
                            KeyCode::NumpadAdd | KeyCode::Equal => self.args.speed = (self.args.speed * 2.0).min(16.0),
                            KeyCode::NumpadSubtract | KeyCode::Minus => self.args.speed = (self.args.speed / 2.0).max(0.125),
                            _ => {}
                        }
                        if k == KeyCode::Escape {
                            if self.captured {
                                let _ = g.window.set_cursor_grab(CursorGrabMode::None);
                                g.window.set_cursor_visible(true);
                                self.captured = false;
                            } else {
                                el.exit();
                            }
                        }
                        if !self.keys.contains(&k) {
                            self.pressed.insert(k);
                        }
                        self.keys.insert(k);
                    } else {
                        self.keys.remove(&k);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32().min(0.1);
                self.last = now;
                let cam = self.cam.as_mut().unwrap();
                if let (true, Some(sh)) = (self.args.play, &self.live) {
                    let k = |c: KeyCode| self.keys.contains(&c);
                    let (f, r) = ([cam.yaw.cos(), cam.yaw.sin()], [-cam.yaw.sin(), cam.yaw.cos()]);
                    let mut w = [0.0f32; 2];
                    let mut add = |v: [f32; 2], s: f32| { w[0] += v[0] * s; w[1] += v[1] * s; };
                    if k(KeyCode::KeyW) || k(KeyCode::ArrowUp) { add(f, 1.0); }
                    if k(KeyCode::KeyS) || k(KeyCode::ArrowDown) { add(f, -1.0); }
                    if k(KeyCode::KeyD) || k(KeyCode::ArrowRight) { add(r, 1.0); }
                    if k(KeyCode::KeyA) || k(KeyCode::ArrowLeft) { add(r, -1.0); }
                    let l = w[0].hypot(w[1]);
                    if l > 1.0 { w = [w[0] / l, w[1] / l]; }
                    let to_ue = |a: f32| ((a / std::f32::consts::TAU) * 65536.0) as i32 & 0xffff;
                    let mut s = sh.lock().unwrap();
                    let prev = s.input.unwrap_or_default();
                    let mut slot = prev.slot;
                    for (i, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5].iter().enumerate() {
                        if self.pressed.remove(key) { slot = Some(i as u8 + 1); }
                    }
                    s.input = Some(live_view::Input {
                        wish: w, yaw: to_ue(cam.yaw), pitch: to_ue(cam.pitch), fire: self.fire_held, reload: k(KeyCode::KeyR),
                        slot,
                        grenade: prev.grenade || self.pressed.remove(&KeyCode::KeyG),
                        buy_clip: prev.buy_clip || self.pressed.remove(&KeyCode::KeyB),
                        buy_shotgun: prev.buy_shotgun || self.pressed.remove(&KeyCode::KeyN),
                    });
                    if let Some(me) = s.me {
                        // Eye height above the pawn's collision center.
                        cam.pos = Vec3::new(me[0], me[2] + 44.0, me[1]); // KFHumanPawn.EyeHeight
                    }
                }
                let speed = if self.args.play { 0.0 } else if self.keys.contains(&KeyCode::ShiftLeft) { 3000.0 } else { 900.0 } * dt;
                let (f, r) = (cam.forward(), cam.right());
                let k = |c: KeyCode| self.keys.contains(&c);
                // Physical keys: KeyW is the key at the W position (Z on AZERTY).
                if k(KeyCode::KeyW) || k(KeyCode::ArrowUp) { cam.pos += f * speed; }
                if k(KeyCode::KeyS) || k(KeyCode::ArrowDown) { cam.pos -= f * speed; }
                if k(KeyCode::KeyD) || k(KeyCode::ArrowRight) { cam.pos += r * speed; }
                if k(KeyCode::KeyA) || k(KeyCode::ArrowLeft) { cam.pos -= r * speed; }
                if k(KeyCode::Space) { cam.pos.y += speed; }
                if k(KeyCode::KeyC) || k(KeyCode::ControlLeft) { cam.pos.y -= speed; }

                let frame = match g.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(_) => {
                        g.surface.configure(&g.device, &g.config);
                        return;
                    }
                };
                let view = frame.texture.create_view(&Default::default());
                if let Some(rp) = &self.replay {
                    if !self.paused {
                        self.replay_ms += dt as f64 * 1000.0 * self.args.speed as f64;
                        if self.replay_ms > rp.end_ms as f64 {
                            self.replay_ms = rp.start_ms as f64;
                        }
                    }
                    let t = self.replay_ms as u64;
                    // Slots of the pool, grouped per mesh in pool order.
                    let mut base = Vec::new();
                    let mut acc = 0;
                    for p in &rp.pool {
                        base.push(acc);
                        acc += p.2;
                    }
                    let mut used = vec![0usize; rp.pool.len()];
                    for a in g.actors.iter_mut() {
                        a.location = [0.0, 0.0, -1.0e6];
                        a.view_shrink = None;
                        a.anim_time = None;
                    }
                    for tr in &rp.tracks {
                        let Some(s) = tr.at(t) else { continue };
                        if used[tr.mesh] >= rp.pool[tr.mesh].2 {
                            continue;
                        }
                        let a = &mut g.actors[base[tr.mesh] + used[tr.mesh]];
                        used[tr.mesh] += 1;
                        a.location = s.loc;
                        a.yaw = s.yaw;
                        a.seq = if s.speed > 40.0 { a.move_seq } else { a.idle_seq };
                        if tr.is_player && self.follow {
                            let yaw = s.yaw as f32 * std::f32::consts::TAU / 65536.0;
                            cam.yaw = yaw;
                            cam.pitch = -0.25;
                            cam.pos = Vec3::new(s.loc[0] - yaw.cos() * 260.0, s.loc[2] + 110.0, s.loc[1] - yaw.sin() * 260.0);
                        }
                    }
                    if self.frames == 0 {
                        self.title_extra = format!(" - replay {:.1}/{:.1} s x{}{}", (self.replay_ms - rp.start_ms as f64) / 1000.0, (rp.end_ms - rp.start_ms) as f64 / 1000.0, self.args.speed, if self.paused { " (pause)" } else { "" });
                    }
                }
                // Live map change (server travel + reconnect): reload the scene.
                let new_map = self.live.as_ref().and_then(|sh| sh.lock().unwrap().map.clone());
                if let Some(m) = new_map {
                    if !m.eq_ignore_ascii_case(&self.title) {
                        let rom = openkf_network::download::find_package_file(&self.args.kf, &format!("{m}.rom")).unwrap_or_else(|| self.args.kf.join("Maps").join(&m).with_extension("rom"));
                        println!("[Live] map change {} -> {m}, reloading", self.title);
                        let bc = g.device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
                        match scene::load_scene(&self.args.kf, &rom, bc, self.args.zeds, Some(self.live_pool.as_slice())) {
                            Ok(mut sc) => {
                                g.renderer = render::Renderer::new(&g.device, &g.queue, g.config.format, &sc);
                                g.actors = std::mem::take(&mut sc.actors);
                                g.props = std::mem::take(&mut sc.props);
                                *cam = initial_camera(&sc, &self.args);
                                self.title = sc.name.clone();
                            }
                            Err(e) => {
                                println!("[Live] cannot load {m}: {e}");
                                self.title = m;
                            }
                        }
                    }
                }
                if let Some(sh) = &self.live {
                    let s = sh.lock().unwrap();
                    let mut base = Vec::new();
                    let mut acc = 0;
                    for p in &self.live_pool {
                        base.push(acc);
                        acc += p.2;
                    }
                    let mut used = vec![0usize; self.live_pool.len()];
                    for a in g.actors.iter_mut() {
                        a.location = [0.0, 0.0, -1.0e6];
                        a.view_shrink = None;
                        a.anim_time = None;
                    }
                    for a in g.actors.iter_mut() {
                        a.attach_to = None;
                    }
                    let mut slot_of = vec![usize::MAX; s.pawns.len()];
                    for (pi, p) in s.pawns.iter().enumerate() {
                        if used[p.mesh] >= self.live_pool[p.mesh].2 {
                            continue;
                        }
                        let idx = base[p.mesh] + used[p.mesh];
                        let a = &mut g.actors[idx];
                        used[p.mesh] += 1;
                        a.location = p.loc;
                        a.yaw = p.yaw;
                        a.pitch = p.pitch;
                        a.roll = p.roll;
                        a.apply_pawn_anim(p);
                        slot_of[pi] = idx;
                    }
                    for at in &s.attachments {
                        let parent = slot_of.get(at.parent).copied().unwrap_or(usize::MAX);
                        if parent == usize::MAX || used[at.mesh] >= self.live_pool[at.mesh].2 {
                            continue;
                        }
                        let a = &mut g.actors[base[at.mesh] + used[at.mesh]];
                        used[at.mesh] += 1;
                        a.attach_to = Some((parent, at.bone.clone()));
                        a.location = [0.0; 3];
                    }
                    if let (true, Some(vw), Some(me)) = (self.args.play, s.view_weapon.clone(), s.me) {
                        if used[vw.mesh] < self.live_pool[vw.mesh].2 {
                            let a = &mut g.actors[base[vw.mesh] + used[vw.mesh]];
                            used[vw.mesh] += 1;
                            place_view_weapon(a, me, cam.yaw, cam.pitch, vw.offset, vw.display_fov, vw.anim.as_ref());
                        }
                    }
                    props::apply_live(&mut g.props, &s.props, live_view::prop_pool());
                    if self.frames == 0 {
                        self.title_extra = format!(" - LIVE {} - {}", s.hud, s.status);
                    }
                }
                if !g.actors.is_empty() {
                    let verts = zeds::skin_all(&mut g.actors, g.start.elapsed().as_secs_f32());
                    g.renderer.update_actors(&g.queue, &verts);
                }
                if !g.props.is_empty() {
                    let pv = props::transform_all(&g.props);
                    g.renderer.update_props(&g.queue, &pv);
                }
                #[allow(unused_mut)]
                let mut cam = cam;
                if self.args.play {
                    let aspect = g.config.width as f32 / g.config.height.max(1) as f32;
                    cam.fov_y = 2.0 * (1.0 / aspect).atan();
                }
                g.renderer.render(&g.device, &g.queue, &view, g.config.width, g.config.height, cam);
                if let Some(sh) = &self.live {
                    let parts = sh.lock().unwrap().particles.clone();
                    if !parts.is_empty() {
                        if let Some(depth) = g.renderer.depth_texture() {
                            let aspect = g.config.width as f32 / g.config.height.max(1) as f32;
                            g.particles.draw(&g.device, &g.queue, &view, depth, &cam, aspect, &parts.parts);
                        }
                    }
                    let line = sh.lock().unwrap().hud.clone();
                    let controls = if self.args.play { "WASD MOVE  CLICK FIRE  R RELOAD  1-5 WEAPONS  G GRENADE  B BUY CLIP" } else { "SPECTATOR" };
                    g.hud.draw(&g.device, &g.queue, &view, g.config.width, g.config.height, &[line, controls.to_string()], self.args.play);
                }
                frame.present();

                self.frames += 1;
                let el_s = self.fps_t.elapsed().as_secs_f32();
                if el_s >= 0.5 {
                    let fps = self.frames as f32 / el_s;
                    g.window.set_title(&format!(
                        "OpenKF Viewer - {}{} - {:.0} FPS ({:.2} ms) - {} draws, {:.1}M tris",
                        self.title, self.title_extra, fps, 1000.0 / fps, g.renderer.draw_calls(), g.renderer.triangles as f32 / 1e6
                    ));
                    self.frames = 0;
                    self.fps_t = Instant::now();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, event: DeviceEvent) {
        if let (DeviceEvent::MouseMotion { delta }, true) = (event, self.captured) {
            if let Some(cam) = self.cam.as_mut() {
                cam.yaw += delta.0 as f32 * 0.0025;
                cam.pitch = (cam.pitch - delta.1 as f32 * 0.0025).clamp(-1.55, 1.55);
            }
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

fn main() {
    let mut args = parse_args();
    // Server browser: pick from favorites / recents / typed address, then join it.
    if args.browse && args.connect.is_none() {
        match browser::pick() {
            Some(addr) => args.connect = Some(addr),
            None => {
                eprintln!("no server selected");
                std::process::exit(1);
            }
        }
    }
    if let Some(addr) = &args.connect {
        browser::push_recent(addr);
    }
    let replay = args.replay.as_ref().map(|p| replay::load(p, &args.kf).expect("replay"));
    let mut live = None;
    let mut live_map = None;
    let mut live_pool_global: Option<Vec<(String, String, usize)>> = None;
    if let Some(addr) = &args.connect {
        // --auth steam (default if Steam runs) or --auth <capture.txt>.
        let auth = if args.auth.as_os_str() == "steam" {
            None
        } else {
            let text = std::fs::read_to_string(&args.auth).expect("--auth capture");
            Some(openkf_network::live::AuthConfig::from_capture(&text).expect("no AUTH in capture"))
        };
        let addr = std::net::ToSocketAddrs::to_socket_addrs(addr.as_str()).expect("address").next().expect("address");
        let t_def = Instant::now();
        let defaults = std::sync::Arc::new(openkf_package::uc_defaults::ScriptDefaults::load(&args.kf));
        let pool = live_view::pool_from_defaults(&defaults);
        println!("[Live] {} class defaults in {:.1} s, pool {:?}", defaults.classes.len(), t_def.elapsed().as_secs_f32(), pool.iter().map(|p| format!("{}x{}", p.2, p.1)).collect::<Vec<_>>());
        live_pool_global = Some(pool.clone());
        let ppool = live_view::prop_pool_from_defaults(&defaults);
        println!("[Live] prop pool: {} static meshes ({} slots)", ppool.len(), ppool.iter().map(|p| p.2).sum::<usize>());
        live_view::set_prop_pool(ppool);
        // The automated test player (no --play) only runs on local / LAN test servers.
        // On public servers the client joins as a passive spectator unless --play is given.
        let local = match addr.ip() {
            std::net::IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local(),
            std::net::IpAddr::V6(v) => v.is_loopback(),
        };
        let ready = args.ready && (args.play || local);
        if args.ready && !ready {
            println!("[Live] public server: test bot disabled, joining as spectator (use --play to play)");
        }
        let sh = live_view::start(addr, args.kf.clone(), auth, args.name.clone(), ready, Some(defaults), pool);
        if args.play {
            // Screenshot mode can't read the keyboard: walk towards +X, aim level (path test).
            let fixed = live_view::Input { wish: if args.screenshot.is_some() || args.bench > 0 { [1.0, 0.0] } else { [0.0, 0.0] }, ..Default::default() };
            sh.lock().unwrap().input = Some(fixed);
        }
        let t = Instant::now();
        loop {
            {
                let s = sh.lock().unwrap();
                if s.failed {
                    eprintln!("[Live] connection failed: {}", s.status);
                    std::process::exit(1);
                }
                if s.map.is_some() {
                    live_map = s.map.clone();
                    break;
                }
            }
            // Generous: the server may be changing maps, and missing packages are
            // downloaded (FastDL) before the level is announced to the renderer.
            if t.elapsed() > Duration::from_secs(900) {
                eprintln!("[Live] no WELCOME from server after 900 s");
                std::process::exit(1);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        println!("[Live] joined, map {}", live_map.as_deref().unwrap_or("?"));
        live = Some(sh);
    }
    let args = resolve_map(args, replay.as_ref().and_then(|r| r.map.as_deref()).or(live_map.as_deref()));
    if args.screenshot.is_some() || args.bench > 0 {
        screenshot(&args, replay.as_ref(), live.as_ref(), live_pool_global.clone().unwrap_or_else(live_view::pool));
        return;
    }
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        args,
        replay,
        live,
        live_pool: live_pool_global.clone().unwrap_or_else(live_view::pool),
        replay_ms: 0.0,
        paused: false,
        follow: false,
        title_extra: String::new(),
        gfx: None,
        cam: None,
        keys: HashSet::new(),
        fire_held: false,
        pressed: HashSet::new(),
        captured: false,
        last: Instant::now(),
        fps_t: Instant::now(),
        frames: 0,
        title: String::new(),
    };
    el.run_app(&mut app).expect("run");
}
