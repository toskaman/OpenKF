//! CPU skinning of UE2 skeletal meshes (one actor per rayon task).
//!
//! UE2 convention: bone quaternions of non-root bones are stored conjugated.
//! Global[i] = Global[parent] * T(pos) * R(quat); skinned point =
//! sum(w * Global_anim[b] * inverse(Global_ref[b]) * p).

use glam::{Mat4, Quat, Vec3};
use openkf_package::{MeshAnimation, SkeletalMesh};
use std::sync::Arc;

pub fn ue_quat(q: [f32; 4], is_root: bool) -> Quat {
    let q = Quat::from_xyzw(q[0], q[1], q[2], q[3]);
    let q = if is_root { q } else { q.conjugate() };
    q.normalize()
}

fn local(q: [f32; 4], p: [f32; 3], is_root: bool) -> Mat4 {
    Mat4::from_rotation_translation(ue_quat(q, is_root), Vec3::from(p))
}

fn globals(locals: &[Mat4], parents: &[i32]) -> Vec<Mat4> {
    let mut g: Vec<Mat4> = Vec::with_capacity(locals.len());
    for (i, l) in locals.iter().enumerate() {
        let p = parents[i];
        let m = if p >= 0 && (p as usize) < i { g[p as usize] * *l } else { *l };
        g.push(m);
    }
    g
}

pub struct SkinRig {
    pub mesh: Arc<SkeletalMesh>,
    pub anim: Option<Arc<MeshAnimation>>,
    /// mesh bone -> animation bone
    pub bone_map: Vec<Option<usize>>,
    pub inv_ref: Vec<Mat4>,
    parents: Vec<i32>,
}

impl SkinRig {
    pub fn new(mesh: Arc<SkeletalMesh>, anim: Option<Arc<MeshAnimation>>) -> Self {
        let parents: Vec<i32> = mesh.bones.iter().map(|b| b.parent).collect();
        let locals: Vec<Mat4> = mesh.bones.iter().enumerate().map(|(i, b)| local(b.orientation, b.position, i == 0)).collect();
        let inv_ref = globals(&locals, &parents).iter().map(|m| m.inverse()).collect();
        let bone_map = mesh
            .bones
            .iter()
            .map(|b| anim.as_ref().and_then(|a| a.bone_names.iter().position(|n| n.eq_ignore_ascii_case(&b.name))))
            .collect();
        Self { mesh, anim, bone_map, inv_ref, parents }
    }

    /// Bone global matrices (raw mesh space) for `seq` at `time`.
    pub fn pose_globals(&self, seq: usize, time: f32) -> Vec<Mat4> {
        let m = &self.mesh;
        let sample = self.anim.as_ref().map(|a| a.sample(seq, time)).unwrap_or_default();
        let locals: Vec<Mat4> = m
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| match self.bone_map[i].and_then(|ai| sample.get(ai).copied().flatten()) {
                Some((q, p)) => local(q, p, i == 0),
                None => local(b.orientation, b.position, i == 0),
            })
            .collect();
        globals(&locals, &self.parents)
    }

    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.mesh.bones.iter().position(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// Skinned point positions (mesh space) for `seq` at `time` seconds.
    pub fn pose_points(&self, seq: usize, time: f32) -> Vec<Vec3> {
        let m = &self.mesh;
        let sample = self.anim.as_ref().map(|a| a.sample(seq, time)).unwrap_or_default();
        let locals: Vec<Mat4> = m
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| match self.bone_map[i].and_then(|ai| sample.get(ai).copied().flatten()) {
                Some((q, p)) => local(q, p, i == 0),
                None => local(b.orientation, b.position, i == 0),
            })
            .collect();
        let g = globals(&locals, &self.parents);
        let skin: Vec<Mat4> = g.iter().zip(&self.inv_ref).map(|(a, b)| *a * *b).collect();
        let mut out = vec![Vec3::ZERO; m.points.len()];
        for inf in &m.influences {
            let (pi, bi) = (inf.point as usize, inf.bone as usize);
            if let (Some(o), Some(s)) = (out.get_mut(pi), skin.get(bi)) {
                *o += s.transform_point3(Vec3::from(m.points[pi])) * inf.weight;
            }
        }
        out
    }
}
