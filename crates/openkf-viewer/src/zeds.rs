//! Animated skeletal actors (specimens) placed in the scene, skinned on the CPU
//! in parallel (one rayon task per actor) every frame.

use crate::assets::{AssetLibrary, Blend, Pkg};
use crate::scene::Vertex;
use crate::skin::SkinRig;
use openkf_package::*;
use rayon::prelude::*;
use std::sync::Arc;

pub struct SkelActor {
    pub name: String,
    pub rig: SkinRig,
    pub seq: usize,
    pub seq_name: String,
    /// UE world location / yaw (UE units).
    pub location: [f32; 3],
    pub yaw: i32,
    /// UE pitch (first-person weapon follows the view).
    pub pitch: i32,
    /// UE roll (corpse fall-over fallback when the mesh has no death sequence).
    pub roll: i32,
    /// Replay: sequence to play when idle / moving.
    pub idle_seq: usize,
    pub move_seq: usize,
    /// Attached to (pool index of parent actor, bone name) — weapons in hand.
    pub attach_to: Option<(usize, String)>,
    /// Filled by `skin_all`: parent bone matrix + parent mesh-to-world parameters.
    pub attach_pose: Option<(glam::Mat4, ParentXf)>,
    /// First-person weapon: uniform scale about the eye (same projection, no wall clipping).
    pub view_shrink: Option<ViewShrink>,
    /// One-shot animation time (seconds) instead of the global looping clock.
    pub anim_time: Option<f32>,
    /// Articulated ragdoll deltas (mesh bone order), post-multiplied in `vertices`.
    pub ragdoll_pose: Option<Vec<[f32; 4]>>,
    /// Per material slot: (texture index, blend) and triangle indices into wedge vertices.
    pub slots: Vec<(Option<(usize, Blend)>, Vec<u32>)>,
}

fn find_seq(anim: &Option<MeshAnimation>, keys: &[&str]) -> Option<usize> {
    let a = anim.as_ref()?;
    keys.iter().find_map(|k| a.sequences.iter().position(|s| s.name.to_ascii_lowercase().contains(k)))
}

fn build_actor(
    lib: &mut AssetLibrary,
    pkg: &'static Pkg,
    i: usize,
    texture_for: &mut impl FnMut(&mut AssetLibrary, &'static Pkg, i32) -> Option<(usize, Blend)>,
) -> Option<SkelActor> {
    let mesh = read_skeletal_mesh(pkg, i).ok()?;
    let anim = if mesh.animation > 0 { read_mesh_animation(pkg, (mesh.animation - 1) as usize).ok() } else { None };
    let idle_seq = find_seq(&anim, &["idle"]).unwrap_or(0);
    let move_seq = find_seq(&anim, &["walkf", "walk", "runf", "run"]).unwrap_or(idle_seq);
    if std::env::var_os("OPENKF_LIST_SEQS").is_some() {
        if let Some(a) = &anim {
            println!("[Anim] {} : {}", mesh.name, a.sequences.iter().map(|s| format!("{}({}f@{})", s.name, s.num_frames, s.rate)).collect::<Vec<_>>().join(" "));
        }
    }
    let seq_name = anim.as_ref().and_then(|a| a.sequences.get(idle_seq)).map(|s| s.name.clone()).unwrap_or_default();
    let mut slots: Vec<(Option<(usize, Blend)>, Vec<u32>)> = Vec::new();
    let mut slot_of: std::collections::HashMap<i32, usize> = Default::default();
    for t in &mesh.triangles {
        let mat = mesh.triangle_material(t);
        let si = *slot_of.entry(mat).or_insert_with(|| {
            slots.push((texture_for(lib, pkg, mat), Vec::new()));
            slots.len() - 1
        });
        slots[si].1.extend(t.wedges.iter().map(|&w| w as u32));
    }
    Some(SkelActor {
        name: mesh.name.clone(),
        rig: SkinRig::new(Arc::new(mesh), anim.map(Arc::new)),
        seq: idle_seq,
        seq_name,
        location: [0.0, 0.0, -1.0e6],
        yaw: 0,
        pitch: 0,
        roll: 0,
        idle_seq,
        move_seq,
        attach_to: None,
        attach_pose: None,
        view_shrink: None,
        anim_time: None,
        ragdoll_pose: None,
        slots,
    })
}

/// Loads every SkeletalMesh of `package` and lines them up in front of `origin`.
pub fn spawn_line(
    lib: &mut AssetLibrary,
    package: &str,
    origin_ue: [f32; 3],
    yaw_ue: i32,
    mut texture_for: impl FnMut(&mut AssetLibrary, &'static Pkg, i32) -> Option<(usize, Blend)>,
) -> Vec<SkelActor> {
    let Some(pkg) = lib.package(package) else { return Vec::new() };
    let meshes: Vec<usize> = (0..pkg.exports.len()).filter(|&i| export_class_name(pkg, i) == "SkeletalMesh").collect();
    let n = meshes.len();
    let yaw_rad = yaw_ue as f32 * std::f32::consts::TAU / 65536.0;
    let (fwd, right) = ([yaw_rad.cos(), yaw_rad.sin()], [-yaw_rad.sin(), yaw_rad.cos()]);
    let mut out = Vec::new();
    for (k, &i) in meshes.iter().enumerate() {
        let Some(mut a) = build_actor(lib, pkg, i, &mut texture_for) else { continue };
        let side = (k as f32 - (n as f32 - 1.0) / 2.0) * 85.0;
        a.location = [origin_ue[0] + fwd[0] * 520.0 + right[0] * side, origin_ue[1] + fwd[1] * 520.0 + right[1] * side, origin_ue[2]];
        a.yaw = yaw_ue.wrapping_add(32768);
        out.push(a);
    }
    out
}

/// Cheap pool duplicate: shares the parsed mesh + animation (Arc), clones only
/// the small per-slot state. The rig tables are rebuilt (O(bones), trivial).
fn duplicate_actor(t: &SkelActor) -> SkelActor {
    SkelActor {
        name: t.name.clone(),
        rig: SkinRig::new(Arc::clone(&t.rig.mesh), t.rig.anim.clone()),
        seq: t.seq,
        seq_name: t.seq_name.clone(),
        location: t.location,
        yaw: t.yaw,
        pitch: t.pitch,
        roll: t.roll,
        idle_seq: t.idle_seq,
        move_seq: t.move_seq,
        attach_to: t.attach_to.clone(),
        attach_pose: t.attach_pose,
        view_shrink: t.view_shrink,
        anim_time: t.anim_time,
        ragdoll_pose: None,
        slots: t.slots.clone(),
    }
}

/// Replay pool: `count` hidden instances of each (package, mesh name).
pub fn spawn_pool(
    lib: &mut AssetLibrary,
    pool: &[(String, String, usize)],
    mut texture_for: impl FnMut(&mut AssetLibrary, &'static Pkg, i32) -> Option<(usize, Blend)>,
) -> Vec<SkelActor> {
    let mut out = Vec::new();
    for (package, mesh, count) in pool {
        let Some(pkg) = lib.package(package) else { eprintln!("[Replay] package {package} missing"); continue };
        let Some(i) = (0..pkg.exports.len()).find(|&i| export_class_name(pkg, i) == "SkeletalMesh" && pkg.resolve_name(pkg.exports.get(i).unwrap().object_name).ok() == Some(mesh.as_str())) else {
            eprintln!("[Replay] mesh {package}.{mesh} missing");
            continue;
        };
        // Parse once: every slot shares the same mesh + animation (Arc).
        let Some(first) = build_actor(lib, pkg, i, &mut texture_for) else { continue };
        out.push(first);
        for _ in 1..*count {
            let d = duplicate_actor(out.last().unwrap());
            out.push(d);
        }
    }
    out
}

/// First-person draw: scale `k` about the eye, lateral axes x `lat` (DisplayFOV vs view FOV).
#[derive(Clone, Copy, Debug)]
pub struct ViewShrink {
    pub eye: [f32; 3],
    pub fwd: [f32; 3],
    pub k: f32,
    pub lat: f32,
}

/// Mesh-space -> world parameters of an actor (used for attachments).
#[derive(Clone, Copy, Debug)]
pub struct ParentXf {
    origin: [f32; 3],
    scale: [f32; 3],
    ro: [[f32; 3]; 3],
    ra: [[f32; 3]; 3],
    location: [f32; 3],
}

impl ParentXf {
    fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let v = [(p[0] - self.origin[0]) * self.scale[0], (p[1] - self.origin[1]) * self.scale[1], (p[2] - self.origin[2]) * self.scale[2]];
        let a = rot(&self.ra, rot(&self.ro, v));
        [a[0] + self.location[0], a[1] + self.location[1], a[2] + self.location[2]]
    }
}

#[inline]
fn conv(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], p[1]]
}

fn rot(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        v[0] * m[0][0] + v[1] * m[1][0] + v[2] * m[2][0],
        v[0] * m[0][1] + v[1] * m[1][1] + v[2] * m[2][1],
        v[0] * m[0][2] + v[1] * m[1][2] + v[2] * m[2][2],
    ]
}

impl SkelActor {
    fn xf(&self) -> ParentXf {
        let m = &self.rig.mesh;
        ParentXf { origin: m.mesh_origin, scale: m.mesh_scale, ro: rotation_matrix(m.rot_origin), ra: rotation_matrix([self.pitch, self.yaw, self.roll]), location: self.location }
    }

    /// World-space wedge vertices for the current pose.
    pub fn vertices(&self, time: f32) -> Vec<Vertex> {
        let m = &self.rig.mesh;
        if self.location[2] < -1.0e5 {
            // Hidden pool slot: collapse far below the map, no skinning.
            return m.wedges.iter().map(|w| Vertex { pos: [0.0, -1.0e6, 0.0], nrm: [0.0, 1.0, 0.0], uv: w.uv, light: [0; 4], lm_uv: [0.0; 2] }).collect();
        }
        let pts = self.rig.pose_points_with(self.seq, self.anim_time.unwrap_or(time), self.ragdoll_pose.as_deref());
        let own = self.xf();
        let world: Vec<[f32; 3]> = match &self.attach_pose {
            // Attached: own mesh space (origin/scale/rot origin, no location/yaw) -> parent bone -> parent world.
            Some((bone, parent)) => pts
                .iter()
                .map(|p| {
                    let local = ParentXf { location: [0.0; 3], ra: rotation_matrix([0, 0, 0]), ..own }.apply([p.x, p.y, p.z]);
                    let b = bone.transform_point3(glam::Vec3::from(local));
                    parent.apply([b.x, b.y, b.z])
                })
                .collect(),
            None => pts.iter().map(|p| own.apply([p.x, p.y, p.z])).collect(),
        };
        let world: Vec<[f32; 3]> = match self.view_shrink {
            Some(v) => world
                .iter()
                .map(|w| {
                    let d = [w[0] - v.eye[0], w[1] - v.eye[1], w[2] - v.eye[2]];
                    let f = d[0] * v.fwd[0] + d[1] * v.fwd[1] + d[2] * v.fwd[2];
                    let r = |i: usize| v.eye[i] + v.k * (f * v.fwd[i] + (d[i] - f * v.fwd[i]) * v.lat);
                    [r(0), r(1), r(2)]
                })
                .collect(),
            None => world,
        };
        // Smooth normals from the skinned triangles — hard guard: skip OOB wedges/points (retail GPF: USkeletalMeshInstance::Render SetMaterial null + FArray 0*2)
        let mut nrm = vec![[0f32; 3]; world.len()];
        for t in &m.triangles {
            let ids_opt = t.wedges.map(|w| m.wedges.get(w as usize).map(|x| x.point as usize));
            let Some(ids) = ids_opt[0].zip(ids_opt[1]).zip(ids_opt[2]).map(|((a,b),c)| [a,b,c]) else { continue; };
            if ids.iter().any(|&i| i >= world.len()) { continue; }
            let (a, b, c) = (world[ids[0]], world[ids[1]], world[ids[2]]);
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            for &i in &ids {
                for k in 0..3 {
                    nrm[i][k] += n[k];
                }
            }
        }
        m.wedges
            .iter()
            .map(|w| {
                let i = w.point as usize;
                if i >= nrm.len() || i >= world.len() { return Vertex { pos: [0.0, -1.0e6, 0.0], nrm: [0.0,1.0,0.0], uv: w.uv, light:[0;4], lm_uv:[0.0;2]}; }
                let n = nrm[i];
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                Vertex {
                    pos: conv(world[i]),
                    nrm: conv([n[0] / l, n[1] / l, n[2] / l]),
                    uv: w.uv,
                    light: [0; 4],
                    lm_uv: [0.0; 2],
                }
            })
            .collect()
    }
}

/// Skins every actor in parallel (multi-core).
pub fn skin_all(actors: &mut [SkelActor], time: f32) -> Vec<Vec<Vertex>> {
    // Phase 1: attachments take their parent's bone pose for this frame.
    let poses: Vec<Option<(glam::Mat4, ParentXf)>> = actors
        .iter()
        .map(|a| {
            let (pi, bone) = a.attach_to.as_ref()?;
            let p = actors.get(*pi)?;
            if p.location[2] < -1.0e5 {
                return None;
            }
            let bi = p.rig.bone_index(bone)?;
            let g = p.rig.pose_globals(p.seq, p.anim_time.unwrap_or(time));
            Some((*g.get(bi)?, p.xf()))
        })
        .collect();
    for (a, pose) in actors.iter_mut().zip(poses) {
        a.attach_pose = pose;
        if a.attach_to.is_some() && a.attach_pose.is_none() {
            a.location = [0.0, 0.0, -1.0e6]; // parent hidden/unknown bone: hide
        } else if a.attach_pose.is_some() {
            a.location = [0.0, 0.0, 0.0];
        }
    }
    actors.par_iter().map(|a| a.vertices(time)).collect()
}

impl SkelActor {
    /// Sequence index by exact (case-insensitive) name.
    pub fn seq_by_name(&self, name: &str) -> Option<usize> {
        self.rig.anim.as_ref()?.sequences.iter().position(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Duration of a sequence in seconds.
    pub fn seq_duration(&self, i: usize) -> f32 {
        self.rig.anim.as_ref().and_then(|a| a.sequences.get(i)).map(|s| s.num_frames.max(1) as f32 / s.rate.max(1.0)).unwrap_or(0.0)
    }

    /// Picks the sequence for a live pawn. Priority: death > one-shot AnimAction >
    /// locomotion (direction-aware MovementAnims) > idle. Returns false when the
    /// pawn should not be drawn (dead long enough with no way to show a corpse).
    pub fn apply_pawn_anim(&mut self, p: &crate::live_view::PawnView) -> bool {
        self.anim_time = None;
        self.roll = 0;
        self.pitch = 0;
        if let Some(t) = p.dying {
            let death = p
                .death_anims
                .iter()
                .find_map(|n| self.seq_by_name(n))
                .or_else(|| {
                    let a = self.rig.anim.as_ref()?;
                    ["death", "die_", "dead"].iter().find_map(|k| a.sequences.iter().position(|s| s.name.to_ascii_lowercase().starts_with(k)))
                });
            match death {
                Some(i) => {
                    // Play once, then hold the last frame (upright, in place).
                    self.seq = i;
                    self.anim_time = Some(t.min((self.seq_duration(i) - 0.01).max(0.0)));
                }
                None if p.ragdoll => {
                    // Karma ragdoll drives location + orientation (set by the caller
                    // from the live physics state); freeze the idle pose on the body.
                    self.seq = self.idle_seq;
                    self.anim_time = Some(0.0);
                    self.pitch = p.pitch;
                    self.roll = p.roll;
                }
                None => {
                    // Legacy fallback (no physics state): tip the body over onto the
                    // floor in 0.45 s, frozen in its idle pose.
                    let k = (t / 0.45).min(1.0);
                    let k = k * k;
                    self.seq = self.idle_seq;
                    self.anim_time = Some(0.0);
                    self.roll = (16384.0 * k) as i32;
                    self.location[2] -= p.collision_height * 0.75 * k;
                }
            }
            return true;
        }
        if let Some((name, t)) = &p.action {
            if let Some(i) = self.seq_by_name(name) {
                if *t < self.seq_duration(i) {
                    self.seq = i;
                    self.anim_time = Some(*t);
                    return true;
                }
            }
        }
        self.seq = if p.speed > 40.0 {
            p.move_anim.as_deref().and_then(|n| self.seq_by_name(n)).unwrap_or(self.move_seq)
        } else {
            p.idle_anim.as_deref().and_then(|n| self.seq_by_name(n)).unwrap_or(self.idle_seq)
        };
        true
    }
}
