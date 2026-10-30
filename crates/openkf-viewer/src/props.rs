//! Replicated movable static-mesh actors: pickups (weapons, ammo boxes, cash)
//! and static-mesh projectiles (grenades, rockets, pipe bombs, bolts...).
//!
//! Like the skeletal pool (`zeds.rs`), spawns are not known in advance, so a
//! fixed pool of instances per mesh is built at scene load from the class
//! defaults (`DrawType=DT_StaticMesh` + `StaticMesh=...`). Each frame the live
//! thread publishes `PropView`s; slots are positioned and re-transformed on the
//! CPU (meshes are small: a few hundred to a few thousand vertices).

use crate::assets::{AssetLibrary, Blend, Pkg};
use crate::scene::Vertex;
use openkf_package::*;
use rayon::prelude::*;
use std::sync::Arc;

/// One pool slot.
pub struct PropActor {
    pub name: String,
    /// Shared across pool slots of the same mesh (parsed once in `spawn_pool`).
    pub mesh: Arc<StaticMesh<'static>>,
    /// Per material slot: (texture index, blend) and triangle indices into mesh vertices.
    pub slots: Vec<(Option<(usize, Blend)>, Vec<u32>)>,
    pub visible: bool,
    pub location: [f32; 3],
    /// Pitch, Yaw, Roll (UE units).
    pub rotation: [i32; 3],
    /// DrawScale * DrawScale3D.
    pub scale: [f32; 3],
    pub pre_pivot: [f32; 3],
}

#[inline]
fn conv(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], p[1]]
}

fn build_prop(lib: &mut AssetLibrary, pkg: &'static Pkg, i: usize, texture_for: &mut impl FnMut(&mut AssetLibrary, &'static Pkg, i32) -> Option<(usize, Blend)>) -> Option<PropActor> {
    let mesh = read_static_mesh(pkg, i).ok()?;
    if mesh.positions.is_empty() || mesh.positions.len() > 65_536 {
        return None;
    }
    let mut slots = Vec::new();
    for (si, s) in mesh.sections.iter().enumerate() {
        let mt = mesh.materials.get(si).and_then(|m| texture_for(lib, pkg, m.material));
        if matches!(mt.map(|m| m.1), Some(Blend::Invisible)) {
            continue;
        }
        let n = mesh.positions.len();
        let idx: Vec<u32> = mesh.section_triangles(s).into_iter().filter(|t| t.iter().all(|&k| (k as usize) < n)).flat_map(|t| t.map(|k| k as u32)).collect();
        if !idx.is_empty() {
            slots.push((mt, idx));
        }
    }
    Some(PropActor { name: mesh.name.clone(), mesh: Arc::new(mesh), slots, visible: false, location: [0.0; 3], rotation: [0; 3], scale: [1.0; 3], pre_pivot: [0.0; 3] })
}

/// `count` hidden instances of each (package, static mesh name).
pub fn spawn_pool(lib: &mut AssetLibrary, pool: &[(String, String, usize)], mut texture_for: impl FnMut(&mut AssetLibrary, &'static Pkg, i32) -> Option<(usize, Blend)>) -> Vec<PropActor> {
    let mut out = Vec::new();
    for (package, mesh, count) in pool {
        let Some(pkg) = lib.package(package) else {
            eprintln!("[Props] package {package} missing");
            continue;
        };
        let Some(i) = (0..pkg.exports.len()).find(|&i| export_class_name(pkg, i) == "StaticMesh" && pkg.exports.get(i).and_then(|e| pkg.resolve_name(e.object_name).ok()).is_some_and(|n| n.eq_ignore_ascii_case(mesh))) else {
            eprintln!("[Props] mesh {package}.{mesh} missing");
            // Keep pool indices aligned with the published slots: push nothing, the
            // caller maps by (package, mesh) through `Scene::prop_base`.
            continue;
        };
        // Parse once: every slot shares the same mesh (Arc), only the small
        // per-slot state (name/slots/transform) is cloned.
        let Some(first) = build_prop(lib, pkg, i, &mut texture_for) else { continue };
        out.push(first);
        for _ in 1..*count {
            let t = out.last().unwrap();
            out.push(PropActor { name: t.name.clone(), mesh: Arc::clone(&t.mesh), slots: t.slots.clone(), visible: false, location: [0.0; 3], rotation: [0; 3], scale: [1.0; 3], pre_pivot: [0.0; 3] });
        }
    }
    out
}

impl PropActor {
    /// UE2 LocalToWorld: -PrePivot, scale, rotate, +Location (same as map placements).
    pub fn transform(&self, p: [f32; 3]) -> [f32; 3] {
        let l = [(p[0] - self.pre_pivot[0]) * self.scale[0], (p[1] - self.pre_pivot[1]) * self.scale[1], (p[2] - self.pre_pivot[2]) * self.scale[2]];
        let m = rotation_matrix(self.rotation);
        [
            l[0] * m[0][0] + l[1] * m[1][0] + l[2] * m[2][0] + self.location[0],
            l[0] * m[0][1] + l[1] * m[1][1] + l[2] * m[2][1] + self.location[1],
            l[0] * m[0][2] + l[1] * m[1][2] + l[2] * m[2][2] + self.location[2],
        ]
    }

    pub fn vertices(&self) -> Vec<Vertex> {
        let m = &self.mesh;
        let uv0 = m.uv_channels.first();
        if !self.visible {
            return m.positions.iter().enumerate().map(|(k, _)| Vertex { pos: [0.0, -1.0e6, 0.0], nrm: [0.0, 1.0, 0.0], uv: uv0.and_then(|u| u.get(k)).copied().unwrap_or([0.0; 2]), light: [0; 4], lm_uv: [0.0; 2] }).collect();
        }
        let rm = rotation_matrix(self.rotation);
        let s = self.scale.map(|x| if x.abs() < 1e-6 { 1e-6 } else { x });
        m.positions
            .iter()
            .enumerate()
            .map(|(k, &p)| {
                let n = m.normals.get(k).copied().unwrap_or([0.0, 0.0, 1.0]);
                let l = [n[0] / s[0], n[1] / s[1], n[2] / s[2]];
                let w = [
                    l[0] * rm[0][0] + l[1] * rm[1][0] + l[2] * rm[2][0],
                    l[0] * rm[0][1] + l[1] * rm[1][1] + l[2] * rm[2][1],
                    l[0] * rm[0][2] + l[1] * rm[1][2] + l[2] * rm[2][2],
                ];
                let len = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt().max(1e-12);
                Vertex {
                    pos: conv(self.transform(p)),
                    nrm: conv([w[0] / len, w[1] / len, w[2] / len]),
                    uv: uv0.and_then(|u| u.get(k)).copied().unwrap_or([0.0; 2]),
                    light: [0; 4],
                    lm_uv: [0.0; 2],
                }
            })
            .collect()
    }
}

/// Re-transforms every visible prop slot in parallel (hidden slots: empty Vec).
pub fn transform_all(props: &[PropActor]) -> Vec<Vec<Vertex>> {
    props.par_iter().map(|p| if p.visible { p.vertices() } else { Vec::new() }).collect()
}

/// Places the published props into the pool (unused slots hidden).
pub fn apply_live(props: &mut [PropActor], views: &[crate::live_view::PropView], pool: &[(String, String, usize)]) {
    for p in props.iter_mut() {
        p.visible = false;
    }
    // Slot ranges per pool entry, by mesh name (entries whose mesh failed to load have no slots).
    let mut used = vec![0usize; pool.len()];
    for v in views {
        let Some(entry) = pool.get(v.mesh) else { continue };
        let first = props.iter().position(|p| p.name.eq_ignore_ascii_case(&entry.1));
        let Some(first) = first else { continue };
        let k = first + used[v.mesh];
        if used[v.mesh] >= entry.2 || k >= props.len() || !props[k].name.eq_ignore_ascii_case(&entry.1) {
            continue;
        }
        used[v.mesh] += 1;
        let p = &mut props[k];
        p.visible = true;
        p.location = v.loc;
        p.rotation = v.rot;
        p.scale = v.scale;
        p.pre_pivot = v.pre_pivot;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_prop() -> PropActor {
        PropActor {
            name: "t".into(),
            mesh: Arc::new(StaticMesh {
                name: "t".into(),
                bounding_box: Default::default(),
                bounding_sphere: [0.0; 4],
                sections: Vec::new(),
                positions: vec![[1.0, 0.0, 0.0]],
                normals: vec![[1.0, 0.0, 0.0]],
                colors: Vec::new(),
                uv_channels: Vec::new(),
                indices: Vec::new(),
                materials: Vec::new(),
                props: Default::default(),
            }),
            slots: Vec::new(),
            visible: true,
            location: [10.0, 20.0, 30.0],
            rotation: [0, 16384, 0],
            scale: [2.0; 3],
            pre_pivot: [0.0; 3],
        }
    }

    #[test]
    fn yaw_90_scales_and_translates() {
        let p = unit_prop();
        let w = p.transform([1.0, 0.0, 0.0]);
        // +X rotated 90 deg yaw -> +Y, scaled x2, then + location.
        assert!((w[0] - 10.0).abs() < 1e-3 && (w[1] - 22.0).abs() < 1e-3 && (w[2] - 30.0).abs() < 1e-3, "{w:?}");
        let v = p.vertices();
        // Converted space swaps Y/Z.
        assert!((v[0].pos[2] - 22.0).abs() < 1e-3);
    }

    #[test]
    fn hidden_props_collapse_below_map() {
        let mut p = unit_prop();
        p.visible = false;
        assert!(p.vertices()[0].pos[1] < -1.0e5);
    }
}
