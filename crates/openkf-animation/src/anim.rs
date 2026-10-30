use std::io::{Cursor, Read, Write};

use glam::{Quat, Vec3};

use crate::bone::{BoneTransform, Skeleton};
use crate::error::AnimationError;
use crate::slerp::{lerp_vec3, slerp};

pub const UANIM_MAGIC: [u8; 4] = *b"UANM";
pub const UANIM_VERSION: u32 = 1;

/// Keyframed animation track for a specific bone within an animation sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimBoneTrack {
    /// Identifier of the target bone (e.g. "Bip01_Spine")
    pub bone_name: String,
    /// Rotation keyframes (one per frame, or empty)
    pub rotations: Vec<Quat>,
    /// Translation keyframes (one per frame, or empty)
    pub positions: Vec<Vec3>,
}

impl AnimBoneTrack {
    pub fn new(bone_name: impl Into<String>) -> Self {
        Self {
            bone_name: bone_name.into(),
            rotations: Vec::new(),
            positions: Vec::new(),
        }
    }
}

/// Single animation sequence containing frame metadata and per-bone keyframe tracks.
/// Disassembled from `Engine.dll:0x104f3210` (`UAnimSequence`).
#[derive(Clone, Debug, PartialEq)]
pub struct UAnimSequence {
    /// Sequence identifier (e.g. "WalkF", "Fire", "Idle_Rest", "Run")
    pub name: String,
    /// Total number of discrete keyframes in the sequence
    pub num_frames: u32,
    /// Playback rate in frames per second (typically 30.0 or 25.0 in retail KF)
    pub rate: f32,
    /// Per-bone keyframe tracks
    pub tracks: Vec<AnimBoneTrack>,
}

impl UAnimSequence {
    pub fn new(name: impl Into<String>, num_frames: u32, rate: f32) -> Self {
        Self {
            name: name.into(),
            num_frames,
            rate,
            tracks: Vec::new(),
        }
    }

    /// Total duration of the animation sequence in seconds.
    pub fn duration(&self) -> f32 {
        if self.rate > 0.0 && self.num_frames > 1 {
            (self.num_frames - 1) as f32 / self.rate
        } else {
            0.0
        }
    }

    /// Evaluates the local transform for a specific track at continuous time `time_secs`.
    pub fn sample_track(&self, track: &AnimBoneTrack, time_secs: f32, looping: bool) -> BoneTransform {
        if self.num_frames <= 1 || (track.rotations.is_empty() && track.positions.is_empty()) {
            let rot = track.rotations.first().copied().unwrap_or(Quat::IDENTITY);
            let pos = track.positions.first().copied().unwrap_or(Vec3::ZERO);
            return BoneTransform::from_rotation_translation(rot, pos);
        }

        let dur = self.duration();
        let sample_time = if looping && dur > 0.0 {
            time_secs.rem_euclid(dur)
        } else {
            time_secs.clamp(0.0, dur)
        };

        let frame_float = sample_time * self.rate;
        let frame_idx = frame_float.floor() as usize;
        let alpha = frame_float - frame_float.floor();

        let max_frame = (self.num_frames - 1) as usize;
        let (f0, f1) = if looping {
            let f0 = frame_idx % self.num_frames as usize;
            let f1 = (f0 + 1) % self.num_frames as usize;
            (f0, f1)
        } else {
            let f0 = frame_idx.min(max_frame);
            let f1 = (f0 + 1).min(max_frame);
            (f0, f1)
        };

        // Interpolate rotation
        let rot = if !track.rotations.is_empty() {
            let r0 = track.rotations.get(f0).copied().unwrap_or(Quat::IDENTITY);
            let r1 = track.rotations.get(f1).copied().unwrap_or(r0);
            slerp(r0, r1, alpha)
        } else {
            Quat::IDENTITY
        };

        // Interpolate position
        let pos = if !track.positions.is_empty() {
            let p0 = track.positions.get(f0).copied().unwrap_or(Vec3::ZERO);
            let p1 = track.positions.get(f1).copied().unwrap_or(p0);
            lerp_vec3(p0, p1, alpha)
        } else {
            Vec3::ZERO
        };

        BoneTransform::from_rotation_translation(rot, pos)
    }

    /// Evaluates the complete local pose for all bones in the skeleton at time `time_secs`.
    /// Bones without tracks fall back to their rest-pose orientation and position.
    pub fn sample_pose(&self, skeleton: &Skeleton, time_secs: f32, looping: bool) -> Vec<BoneTransform> {
        let mut pose = Vec::with_capacity(skeleton.bones.len());

        for bone in &skeleton.bones {
            if let Some(track) = self.tracks.iter().find(|t| t.bone_name.eq_ignore_ascii_case(&bone.name)) {
                pose.push(self.sample_track(track, time_secs, looping));
            } else {
                pose.push(BoneTransform::from_rotation_translation(
                    bone.orientation,
                    bone.position,
                ));
            }
        }

        pose
    }
}

/// Retail `UMeshAnimation` container holding an entire set of animation sequences for a character or weapon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UMeshAnimation {
    pub sequences: Vec<UAnimSequence>,
}

impl UMeshAnimation {
    pub fn new() -> Self {
        Self {
            sequences: Vec::new(),
        }
    }

    pub fn add_sequence(&mut self, seq: UAnimSequence) {
        self.sequences.push(seq);
    }

    pub fn find_sequence(&self, name: &str) -> Option<&UAnimSequence> {
        self.sequences.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Serializes the animation container into OpenKF binary format.
    pub fn to_bytes(&self) -> Result<Vec<u8>, AnimationError> {
        let mut buf = Vec::new();
        buf.write_all(&UANIM_MAGIC)?;
        buf.write_all(&UANIM_VERSION.to_le_bytes())?;

        buf.write_all(&(self.sequences.len() as u32).to_le_bytes())?;
        for seq in &self.sequences {
            let name_bytes = seq.name.as_bytes();
            buf.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
            buf.write_all(name_bytes)?;

            buf.write_all(&seq.num_frames.to_le_bytes())?;
            buf.write_all(&seq.rate.to_le_bytes())?;

            buf.write_all(&(seq.tracks.len() as u32).to_le_bytes())?;
            for track in &seq.tracks {
                let track_name_bytes = track.bone_name.as_bytes();
                buf.write_all(&(track_name_bytes.len() as u32).to_le_bytes())?;
                buf.write_all(track_name_bytes)?;

                buf.write_all(&(track.rotations.len() as u32).to_le_bytes())?;
                for rot in &track.rotations {
                    buf.write_all(&rot.x.to_le_bytes())?;
                    buf.write_all(&rot.y.to_le_bytes())?;
                    buf.write_all(&rot.z.to_le_bytes())?;
                    buf.write_all(&rot.w.to_le_bytes())?;
                }

                buf.write_all(&(track.positions.len() as u32).to_le_bytes())?;
                for pos in &track.positions {
                    buf.write_all(&pos.x.to_le_bytes())?;
                    buf.write_all(&pos.y.to_le_bytes())?;
                    buf.write_all(&pos.z.to_le_bytes())?;
                }
            }
        }

        Ok(buf)
    }

    /// Deserializes the animation container from OpenKF binary format.
    pub fn from_bytes(slice: &[u8]) -> Result<Self, AnimationError> {
        let mut cur = Cursor::new(slice);

        let mut magic = [0u8; 4];
        cur.read_exact(&mut magic)?;
        if magic != UANIM_MAGIC {
            return Err(AnimationError::InvalidMagic {
                expected: UANIM_MAGIC,
                got: magic,
            });
        }

        let mut v_bytes = [0u8; 4];
        cur.read_exact(&mut v_bytes)?;
        let version = u32::from_le_bytes(v_bytes);
        if version != UANIM_VERSION {
            return Err(AnimationError::InvalidVersion {
                expected: UANIM_VERSION,
                got: version,
            });
        }

        let read_f32 = |cur: &mut Cursor<&[u8]>| -> Result<f32, AnimationError> {
            let mut b = [0u8; 4];
            cur.read_exact(&mut b)?;
            Ok(f32::from_le_bytes(b))
        };

        cur.read_exact(&mut v_bytes)?;
        let seq_count = u32::from_le_bytes(v_bytes) as usize;
        let mut sequences = Vec::with_capacity(seq_count);

        for _ in 0..seq_count {
            cur.read_exact(&mut v_bytes)?;
            let name_len = u32::from_le_bytes(v_bytes) as usize;
            let mut name_buf = vec![0u8; name_len];
            cur.read_exact(&mut name_buf)?;
            let name = String::from_utf8_lossy(&name_buf).to_string();

            cur.read_exact(&mut v_bytes)?;
            let num_frames = u32::from_le_bytes(v_bytes);

            let rate = read_f32(&mut cur)?;

            cur.read_exact(&mut v_bytes)?;
            let track_count = u32::from_le_bytes(v_bytes) as usize;
            let mut tracks = Vec::with_capacity(track_count);

            for _ in 0..track_count {
                cur.read_exact(&mut v_bytes)?;
                let track_name_len = u32::from_le_bytes(v_bytes) as usize;
                let mut track_name_buf = vec![0u8; track_name_len];
                cur.read_exact(&mut track_name_buf)?;
                let bone_name = String::from_utf8_lossy(&track_name_buf).to_string();

                cur.read_exact(&mut v_bytes)?;
                let rot_count = u32::from_le_bytes(v_bytes) as usize;
                let mut rotations = Vec::with_capacity(rot_count);
                for _ in 0..rot_count {
                    let rx = read_f32(&mut cur)?;
                    let ry = read_f32(&mut cur)?;
                    let rz = read_f32(&mut cur)?;
                    let rw = read_f32(&mut cur)?;
                    rotations.push(Quat::from_xyzw(rx, ry, rz, rw));
                }

                cur.read_exact(&mut v_bytes)?;
                let pos_count = u32::from_le_bytes(v_bytes) as usize;
                let mut positions = Vec::with_capacity(pos_count);
                for _ in 0..pos_count {
                    let px = read_f32(&mut cur)?;
                    let py = read_f32(&mut cur)?;
                    let pz = read_f32(&mut cur)?;
                    positions.push(Vec3::new(px, py, pz));
                }

                tracks.push(AnimBoneTrack {
                    bone_name,
                    rotations,
                    positions,
                });
            }

            sequences.push(UAnimSequence {
                name,
                num_frames,
                rate,
                tracks,
            });
        }

        Ok(Self { sequences })
    }
}
