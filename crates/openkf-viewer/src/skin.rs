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
        self.pose_points_with(seq, time, None)
    }

    #[cfg(test)]
    fn unit_rig() -> SkinRig {
        use openkf_package::{Bone, Influence};
        let mesh = SkeletalMesh {
            name: "t".into(),
            version: 0,
            textures: vec![],
            materials: vec![],
            mesh_scale: [1.0; 3],
            mesh_origin: [0.0; 3],
            rot_origin: [0; 3],
            bones: vec![
                Bone { name: "root".into(), flags: 0, orientation: [0.0, 0.0, 0.0, 1.0], position: [0.0; 3], num_children: 1, parent: -1 },
                Bone { name: "arm".into(), flags: 0, orientation: [0.0, 0.0, 0.0, 1.0], position: [0.0, 0.0, 10.0], num_children: 0, parent: 0 },
            ],
            animation: 0,
            points: vec![[0.0, 0.0, 20.0]],
            wedges: vec![],
            triangles: vec![],
            influences: vec![Influence { weight: 1.0, point: 0, bone: 1 }],
        };
        SkinRig::new(Arc::new(mesh), None)
    }

    /// Same, with per-bone local-rotation deltas post-multiplied (ragdolls).
    /// `delta[i]` is the extra rotation in bone-local space (identity = none);
    /// bones past the slice keep their animated/reference rotation.
    pub fn pose_points_with(&self, seq: usize, time: f32, delta: Option<&[[f32; 4]]>) -> Vec<Vec3> {
        let m = &self.mesh;
        let sample = self.anim.as_ref().map(|a| a.sample(seq, time)).unwrap_or_default();
        let locals: Vec<Mat4> = m
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let l = match self.bone_map[i].and_then(|ai| sample.get(ai).copied().flatten()) {
                    Some((q, p)) => local(q, p, i == 0),
                    None => local(b.orientation, b.position, i == 0),
                };
                match delta.and_then(|d| d.get(i)) {
                    Some(dq) => l * Mat4::from_quat(Quat::from_array(*dq)),
                    None => l,
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_pose_is_identity() {
        let r = SkinRig::unit_rig();
        let pts = r.pose_points(0, 0.0);
        assert_eq!(pts.len(), 1);
        assert!((pts[0] - Vec3::new(0.0, 0.0, 20.0)).length() < 1e-4, "{:?}", pts[0]);
    }

    #[test]
    fn ragdoll_delta_rotates_limb() {
        let r = SkinRig::unit_rig();
        let ident = [[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]];
        let same = r.pose_points_with(0, 0.0, Some(&ident));
        assert!((same[0] - Vec3::new(0.0, 0.0, 20.0)).length() < 1e-4, "{same:?}");
        // 90 deg about X on the arm: the point swings from +Z to -Y/+Y.
        let q = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2).to_array();
        let bent = r.pose_points_with(0, 0.0, Some(&[[0.0, 0.0, 0.0, 1.0], q]));
        assert!(!bent[0].x.is_nan() && !bent[0].y.is_nan() && !bent[0].z.is_nan(), "{bent:?}");
        assert!((bent[0] - Vec3::new(0.0, 0.0, 20.0)).length() > 5.0, "{bent:?}");
        // Rotation about the bone origin [0,0,10]: stays 10 from it.
        assert!(((bent[0] - Vec3::new(0.0, 0.0, 10.0)).length() - 10.0).abs() < 1e-3, "{bent:?}");
    }
}
