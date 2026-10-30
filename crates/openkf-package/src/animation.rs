//! UE2 `UMeshAnimation` decoding + sampling (Killing Floor v1065).
//!
//! Layout (verified on KF_Freaks_Trip.Clot_Anim): props, `i32 Version`,
//! `TArray<FNamedBone {Name, u32 Flags, i32 Parent}> RefBones`,
//! `TArray<MotionChunk> Moves`, `TArray<FMeshAnimSeq> AnimSeqs`.
//! MotionChunk: `FVector RootSpeed3D, f32 TrackTime, i32 StartBone, u32 Flags,
//! TArray<i32> BoneIndices, TArray<AnalogTrack> AnimTracks, AnalogTrack RootTrack`.
//! AnalogTrack: `u32 Flags, TArray<FQuat> KeyQuat, TArray<FVector> KeyPos, TArray<f32> KeyTime`.
//! FMeshAnimSeq: `f32 ?, FName Name, TArray<FName> Groups, i32 StartFrame, i32 NumFrames,
//! TArray<{f32 Time, FName Function, UObject* Notify}> Notifys, f32 Rate`.

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::properties::read_properties;
use crate::reader::UnrealPackage;

#[derive(Debug, Clone, Default)]
pub struct AnalogTrack {
    pub flags: u32,
    pub key_quat: Vec<[f32; 4]>,
    pub key_pos: Vec<[f32; 3]>,
    pub key_time: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct MotionChunk {
    pub root_speed: [f32; 3],
    pub track_time: f32,
    pub start_bone: i32,
    pub flags: u32,
    /// Track index -> animation bone index.
    pub bone_indices: Vec<i32>,
    pub tracks: Vec<AnalogTrack>,
}

#[derive(Debug, Clone)]
pub struct AnimNotify {
    pub time: f32,
    pub function: String,
    pub object: i32,
}

#[derive(Debug, Clone)]
pub struct AnimSequence {
    pub name: String,
    pub groups: Vec<String>,
    pub start_frame: i32,
    pub num_frames: i32,
    /// Frames per second.
    pub rate: f32,
    pub notifies: Vec<AnimNotify>,
}

#[derive(Debug, Clone)]
pub struct MeshAnimation {
    pub name: String,
    pub bone_names: Vec<String>,
    pub bone_parents: Vec<i32>,
    pub moves: Vec<MotionChunk>,
    pub sequences: Vec<AnimSequence>,
}

struct Cur<'a> {
    d: &'a [u8],
    o: usize,
}
impl Cur<'_> {
    fn need(&self, n: usize) -> Result<(), PackageError> {
        if self.o + n > self.d.len() {
            Err(PackageError::UnexpectedEof { offset: self.o, requested: n, total: self.d.len() })
        } else {
            Ok(())
        }
    }
    fn u32(&mut self) -> Result<u32, PackageError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.d[self.o..self.o + 4].try_into().unwrap());
        self.o += 4;
        Ok(v)
    }
    fn i32(&mut self) -> Result<i32, PackageError> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, PackageError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn ci(&mut self) -> Result<i32, PackageError> {
        read_compact_index(self.d, &mut self.o)
    }
    fn count(&mut self, elem: usize) -> Result<usize, PackageError> {
        let n = self.ci()?;
        if n < 0 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: 0, total: self.d.len() });
        }
        let check_elem = if elem == 0 { 1 } else { elem };
        if elem == 0 && n as usize > 8192 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n as usize, total: 8192 });
        }
        let need = (n as usize).checked_mul(check_elem).ok_or(PackageError::UnexpectedEof { offset: self.o, requested: usize::MAX, total: self.d.len() })?;
        self.need(need)?;
        Ok(n as usize)
    }
    fn count_capped(&mut self, elem: usize, max: usize) -> Result<usize, PackageError> {
        let n = self.count(elem)?;
        if n > max {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n, total: max });
        }
        Ok(n)
    }
    fn track(&mut self) -> Result<AnalogTrack, PackageError> {
        let flags = self.u32()?;
        let n = self.count(16)?;
        let mut key_quat = Vec::with_capacity(n);
        for _ in 0..n {
            key_quat.push([self.f32()?, self.f32()?, self.f32()?, self.f32()?]);
        }
        let n = self.count(12)?;
        let mut key_pos = Vec::with_capacity(n);
        for _ in 0..n {
            key_pos.push([self.f32()?, self.f32()?, self.f32()?]);
        }
        let n = self.count(4)?;
        let mut key_time = Vec::with_capacity(n);
        for _ in 0..n {
            key_time.push(self.f32()?);
        }
        Ok(AnalogTrack { flags, key_quat, key_pos, key_time })
    }
}

pub fn read_mesh_animation(pkg: &UnrealPackage<'_>, export_index: usize) -> Result<MeshAnimation, PackageError> {
    let d = pkg.get_export_data(export_index)?;
    let props = read_properties(pkg, d, 0)?;
    let mut c = Cur { d, o: props.end_offset };
    let _version = c.i32()?;
    let nb = c.count_capped(9, 512)?;
    let (mut bone_names, mut bone_parents) = (Vec::with_capacity(nb), Vec::with_capacity(nb));
    for _ in 0..nb {
        let n = c.ci()?;
        bone_names.push(pkg.resolve_name(n).unwrap_or("?").to_string());
        c.u32()?;
        bone_parents.push(c.i32()?);
    }
    let nm = c.count_capped(24, 32_768)?;
    let mut moves = Vec::with_capacity(nm);
    for _ in 0..nm {
        let root_speed = [c.f32()?, c.f32()?, c.f32()?];
        let track_time = c.f32()?;
        let start_bone = c.i32()?;
        let flags = c.u32()?;
        let n = c.count(4)?;
        let mut bone_indices = Vec::with_capacity(n);
        for _ in 0..n {
            bone_indices.push(c.i32()?);
        }
        let n = c.count(4)?;
        let mut tracks = Vec::with_capacity(n);
        for _ in 0..n {
            tracks.push(c.track()?);
        }
        let _root = c.track()?;
        moves.push(MotionChunk { root_speed, track_time, start_bone, flags, bone_indices, tracks });
    }
    let ns = c.count_capped(4, 8192)?;
    let mut sequences = Vec::with_capacity(ns);
    for _ in 0..ns {
        c.f32()?;
        let name = pkg.resolve_name(c.ci()?).unwrap_or("?").to_string();
        let ng = c.count(1)?;
        let mut groups = Vec::with_capacity(ng);
        for _ in 0..ng {
            groups.push(pkg.resolve_name(c.ci()?).unwrap_or("?").to_string());
        }
        let start_frame = c.i32()?;
        let num_frames = c.i32()?;
        let nn = c.count(6)?;
        let mut notifies = Vec::with_capacity(nn);
        for _ in 0..nn {
            let time = c.f32()?;
            let function = pkg.resolve_name(c.ci()?).unwrap_or("None").to_string();
            let object = c.ci()?;
            notifies.push(AnimNotify { time, function, object });
        }
        let rate = c.f32()?;
        sequences.push(AnimSequence { name, groups, start_frame, num_frames, rate, notifies });
    }
    Ok(MeshAnimation { name: pkg.format_full_name(export_index as i32 + 1), bone_names, bone_parents, moves, sequences })
}

fn nlerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let s = if dot < 0.0 { -1.0 } else { 1.0 };
    let q = [
        a[0] + (b[0] * s - a[0]) * t,
        a[1] + (b[1] * s - a[1]) * t,
        a[2] + (b[2] * s - a[2]) * t,
        a[3] + (b[3] * s - a[3]) * t,
    ];
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-12);
    [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
}

impl AnalogTrack {
    /// Samples (rotation, position) at `frame` (track-local time in frames, looping over `length`).
    pub fn sample(&self, frame: f32, length: f32) -> Option<([f32; 4], [f32; 3])> {
        if self.key_quat.is_empty() {
            return None;
        }
        let n = self.key_time.len().max(1);
        // Find key interval.
        let (mut i0, mut i1, mut t) = (0usize, 0usize, 0.0f32);
        if self.key_time.len() > 1 {
            let mut k = n - 1;
            for (j, &kt) in self.key_time.iter().enumerate() {
                if kt > frame {
                    k = j.saturating_sub(1);
                    break;
                }
            }
            i0 = k;
            i1 = if k + 1 < n { k + 1 } else { 0 };
            let t0 = self.key_time[i0];
            let t1 = if i1 > i0 { self.key_time[i1] } else { length };
            t = if t1 > t0 { ((frame - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 };
        }
        let q = |i: usize| self.key_quat[i.min(self.key_quat.len() - 1)];
        let p = |i: usize| self.key_pos.get(i.min(self.key_pos.len().saturating_sub(1))).copied().unwrap_or([0.0; 3]);
        let rot = nlerp(q(i0), q(i1), t);
        let (a, b) = (p(i0), p(i1));
        let pos = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        Some((rot, pos))
    }
}

impl MeshAnimation {
    pub fn sequence(&self, name: &str) -> Option<usize> {
        self.sequences.iter().position(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Local pose (rotation, position) of every animation bone at `time` seconds
    /// into sequence `seq` (looping). Bones without a track get `None`.
    pub fn sample(&self, seq: usize, time: f32) -> Vec<Option<([f32; 4], [f32; 3])>> {
        let mut out = vec![None; self.bone_names.len()];
        let (Some(s), Some(m)) = (self.sequences.get(seq), self.moves.get(seq)) else { return out };
        let length = m.track_time.max(1.0);
        let frame = (time * s.rate.max(1.0)) % length;
        for (ti, tr) in m.tracks.iter().enumerate() {
            let Some(&b) = m.bone_indices.get(ti) else { continue };
            if let Some(slot) = out.get_mut(b as usize) {
                *slot = tr.sample(frame, length);
            }
        }
        out
    }
}
