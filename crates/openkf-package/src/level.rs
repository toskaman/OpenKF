//! Map (`.rom`) helpers: actor property reading and static-mesh placements.

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::model::{read_model, Model};
use crate::properties::{read_properties, PropValue, PropertyList};
use crate::reader::{ObjectRef, UnrealPackage};

/// `RF_HasStack`: object starts with a serialized `FStateFrame`.
pub const RF_HAS_STACK: u32 = 0x0200_0000;

/// Class name (without package) of an export.
pub fn export_class_name<'p>(pkg: &'p UnrealPackage<'_>, export_index: usize) -> &'p str {
    let Some(e) = pkg.exports.get(export_index) else { return "" };
    match pkg.resolve_object_ref(e.class_index) {
        Ok(ObjectRef::Import(i)) => i.resolve_object_name(&pkg.names).unwrap_or(""),
        Ok(ObjectRef::Export(x)) => x.resolve_object_name(&pkg.names).unwrap_or(""),
        _ => "Class",
    }
}

/// Reads the tagged properties of an export, skipping the `FStateFrame` if present.
pub fn read_object_properties<'a>(
    pkg: &UnrealPackage<'a>,
    export_index: usize,
) -> Result<PropertyList<'a>, PackageError> {
    let e = pkg
        .exports
        .get(export_index)
        .ok_or(PackageError::ExportIndexOutOfBounds(export_index, pkg.exports.len()))?;
    let d: &'a [u8] = pkg.get_export_slice(e)?;
    let mut o = 0usize;
    if e.object_flags & RF_HAS_STACK != 0 {
        let node = read_compact_index(d, &mut o)?;
        let _state_node = read_compact_index(d, &mut o)?;
        o += 8 + 4; // ProbeMask (u64) + LatentAction (i32)
        if node != 0 {
            let _offset = read_compact_index(d, &mut o)?;
        }
    }
    read_properties(pkg, d, o)
}

/// A static mesh placed in a map.
#[derive(Debug, Clone)]
pub struct MeshPlacement {
    pub actor: String,
    pub class: String,
    /// Fully qualified mesh name, e.g. `KillingFloorStatics.Barrels.Barrel1`.
    pub mesh: String,
    /// Package object index of the mesh in the map package.
    pub mesh_ref: i32,
    pub location: [f32; 3],
    /// Pitch, Yaw, Roll (65536 = 360°).
    pub rotation: [i32; 3],
    pub draw_scale: f32,
    pub draw_scale_3d: [f32; 3],
    pub pre_pivot: [f32; 3],
    pub hidden: bool,
    /// Per-section material overrides (`Skins` array) as object indices (0 = keep mesh material).
    pub skins: Vec<i32>,
    /// Baked per-vertex lighting from the actor's `StaticMeshInstance` (RGBA, one per
    /// mesh vertex; UE2 modulates texture by 2x this color). Empty if unlit/dynamic.
    pub vertex_colors: Vec<[u8; 4]>,
    /// `bUnlit`: render at full brightness.
    pub unlit: bool,
}

impl MeshPlacement {
    /// Transforms a mesh-local point to world space (UE2 LocalToWorld:
    /// -PrePivot, scale, rotation, +Location).
    pub fn transform_point(&self, p: [f32; 3]) -> [f32; 3] {
        let s = [
            self.draw_scale * self.draw_scale_3d[0],
            self.draw_scale * self.draw_scale_3d[1],
            self.draw_scale * self.draw_scale_3d[2],
        ];
        let l = [
            (p[0] - self.pre_pivot[0]) * s[0],
            (p[1] - self.pre_pivot[1]) * s[1],
            (p[2] - self.pre_pivot[2]) * s[2],
        ];
        let m = rotation_matrix(self.rotation);
        [
            l[0] * m[0][0] + l[1] * m[1][0] + l[2] * m[2][0] + self.location[0],
            l[0] * m[0][1] + l[1] * m[1][1] + l[2] * m[2][1] + self.location[1],
            l[0] * m[0][2] + l[1] * m[1][2] + l[2] * m[2][2] + self.location[2],
        ]
    }

    /// Transforms a mesh-local normal (rotation + non-uniform scale correction).
    pub fn transform_normal(&self, n: [f32; 3]) -> [f32; 3] {
        let s = [
            self.draw_scale * self.draw_scale_3d[0],
            self.draw_scale * self.draw_scale_3d[1],
            self.draw_scale * self.draw_scale_3d[2],
        ];
        let l = [n[0] / s[0], n[1] / s[1], n[2] / s[2]];
        let m = rotation_matrix(self.rotation);
        let w = [
            l[0] * m[0][0] + l[1] * m[1][0] + l[2] * m[2][0],
            l[0] * m[0][1] + l[1] * m[1][1] + l[2] * m[2][1],
            l[0] * m[0][2] + l[1] * m[1][2] + l[2] * m[2][2],
        ];
        let len = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt().max(1e-12);
        [w[0] / len, w[1] / len, w[2] / len]
    }

    /// True if the scale mirrors the mesh (odd number of negative axes): flip winding.
    pub fn is_mirrored(&self) -> bool {
        let s = self.draw_scale * self.draw_scale_3d[0] * self.draw_scale_3d[1] * self.draw_scale_3d[2];
        s < 0.0
    }
}

/// UE2 `FRotationMatrix`: rows are the rotated X, Y, Z axes.
pub fn rotation_matrix(r: [i32; 3]) -> [[f32; 3]; 3] {
    let k = std::f32::consts::PI * 2.0 / 65536.0;
    let (sp, cp) = ((r[0] as f32) * k).sin_cos();
    let (sy, cy) = ((r[1] as f32) * k).sin_cos();
    let (sr, cr) = ((r[2] as f32) * k).sin_cos();
    [
        [cp * cy, cp * sy, sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp],
        [-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp],
    ]
}

fn vec_prop(p: &PropertyList<'_>, name: &str, default: [f32; 3]) -> [f32; 3] {
    match p.get(name) {
        Some(PropValue::Vector(v)) => *v,
        _ => default,
    }
}

/// Reads an array property of object references (either one `Array` property
/// `compact Count, compact Obj...` or indexed entries `Skins[i]`).
fn object_array(p: &PropertyList<'_>, name: &str) -> Vec<i32> {
    let mut out = Vec::new();
    for prop in p.props.iter().filter(|x| x.name.eq_ignore_ascii_case(name)) {
        match &prop.value {
            PropValue::Raw(raw) => {
                let mut o = 0usize;
                if let Ok(n) = read_compact_index(raw, &mut o) {
                    for _ in 0..n.max(0) {
                        match read_compact_index(raw, &mut o) {
                            Ok(v) => out.push(v),
                            Err(_) => break,
                        }
                    }
                }
            }
            PropValue::Object(v) => {
                let i = prop.array_index as usize;
                if out.len() <= i {
                    out.resize(i + 1, 0);
                }
                out[i] = *v;
            }
            _ => {}
        }
    }
    out
}

/// Collects every actor with a `StaticMesh` property and `DrawType` static mesh
/// (StaticMeshActor and subclasses: movers, glass, doors...).
pub fn static_mesh_placements(pkg: &UnrealPackage<'_>) -> Vec<MeshPlacement> {
    let mut out = Vec::new();
    for i in 0..pkg.exports.len() {
        let e = &pkg.exports.entries[i];
        if e.object_flags & RF_HAS_STACK == 0 {
            continue; // only actors carry a state frame in maps
        }
        let Ok(props) = read_object_properties(pkg, i) else { continue };
        let Some(mesh_ref) = props.get_object("StaticMesh").filter(|&m| m != 0) else { continue };
        // DrawType: DT_StaticMesh = 8. StaticMeshActor defaults to it, so absent = ok.
        if let Some(dt) = props.get_byte("DrawType") {
            if dt != 8 {
                continue;
            }
        }
        let class = export_class_name(pkg, i).to_string();
        out.push(MeshPlacement {
            actor: pkg.format_full_name(i as i32 + 1),
            class,
            mesh: pkg.format_full_name(mesh_ref),
            mesh_ref,
            location: vec_prop(&props, "Location", [0.0; 3]),
            rotation: match props.get("Rotation") {
                Some(PropValue::Rotator(r)) => *r,
                _ => [0; 3],
            },
            draw_scale: match props.get("DrawScale") {
                Some(PropValue::Float(f)) => *f,
                _ => 1.0,
            },
            draw_scale_3d: vec_prop(&props, "DrawScale3D", [1.0; 3]),
            pre_pivot: vec_prop(&props, "PrePivot", [0.0; 3]),
            hidden: props.get_bool("bHidden").unwrap_or(false),
            skins: object_array(&props, "Skins"),
            vertex_colors: if props.get_bool("bUnlit").unwrap_or(false) {
                Vec::new()
            } else {
                let c: Vec<[u8; 4]> = props
                    .get_object("StaticMeshInstance")
                    .filter(|&x| x > 0)
                    .map(|x| read_instance_colors(pkg, (x - 1) as usize))
                    .unwrap_or_default();
                // An all-black stream means "never lit" (e.g. lighting not rebuilt).
                if c.iter().all(|x| x[0] == 0 && x[1] == 0 && x[2] == 0) { Vec::new() } else { c }
            },
            unlit: props.get_bool("bUnlit").unwrap_or(false),
        });
    }
    out
}

/// Reads the baked `ColorStream` of a `StaticMeshInstance` export.
pub fn read_instance_colors(pkg: &UnrealPackage<'_>, export_index: usize) -> Vec<[u8; 4]> {
    let Ok(raw) = pkg.get_export_data(export_index) else { return Vec::new() };
    let Ok(pl) = read_properties(pkg, raw, 0) else { return Vec::new() };
    let mut o = pl.end_offset;
    let Ok(n) = read_compact_index(raw, &mut o) else { return Vec::new() };
    let n = n.max(0) as usize;
    if o + n * 4 > raw.len() {
        return Vec::new();
    }
    raw[o..o + n * 4].chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
}

/// The level's world BSP: the `Model` export with the most nodes.
pub fn read_level_model(pkg: &UnrealPackage<'_>) -> Result<Model, PackageError> {
    let mut best: Option<Model> = None;
    for i in 0..pkg.exports.len() {
        if export_class_name(pkg, i) != "Model" {
            continue;
        }
        if let Ok(m) = read_model(pkg, i) {
            if best.as_ref().is_none_or(|b| m.nodes.len() > b.nodes.len()) {
                best = Some(m);
            }
        }
    }
    best.ok_or(PackageError::ExportIndexOutOfBounds(0, 0))
}
