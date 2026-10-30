//! Decodes every static mesh of the retail install. Set KF_DIR to the game root
//! (defaults to E:\SteamLibrary\steamapps\common\KillingFloor); skipped if absent.
use openkf_package::*;
use std::path::PathBuf;

fn kf_dir() -> Option<PathBuf> {
    let p = std::env::var("KF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"E:\SteamLibrary\steamapps\common\KillingFloor"));
    p.join("StaticMeshes").is_dir().then_some(p)
}

#[test]
fn decode_warehouse_door() {
    let Some(dir) = kf_dir() else { return };
    let data = std::fs::read(dir.join("StaticMeshes/22Patch.usx")).unwrap();
    let pkg = UnrealPackage::from_bytes(&data).unwrap();
    let idx = static_mesh_exports(&pkg)
        .into_iter()
        .find(|&i| pkg.format_full_name(i as i32 + 1).ends_with("WarehouseDoor"))
        .unwrap();
    let m = read_static_mesh(&pkg, idx).unwrap();
    assert_eq!(m.positions.len(), 302);
    assert_eq!(m.sections.len(), 1);
    assert_eq!(m.section_triangles(&m.sections[0]).len(), 300);
    assert_eq!(m.materials[0].material_name, "PatchTex.Common.WHouseDoorSkin");
}

#[test]
fn decode_all_retail_static_meshes() {
    let Some(dir) = kf_dir() else { return };
    let (mut ok, mut bad) = (0, Vec::new());
    for e in std::fs::read_dir(dir.join("StaticMeshes")).unwrap().flatten() {
        let data = std::fs::read(e.path()).unwrap();
        let Ok(pkg) = UnrealPackage::from_bytes(&data) else { continue };
        for idx in static_mesh_exports(&pkg) {
            match read_static_mesh(&pkg, idx) {
                Ok(m) if m.indices.iter().all(|&i| (i as usize) < m.positions.len()) => ok += 1,
                Ok(m) => bad.push(format!("{} out-of-range index", m.name)),
                Err(err) => bad.push(format!("{:?} #{idx}: {err:?}", e.file_name())),
            }
        }
    }
    assert!(ok > 4000, "only {ok} meshes parsed");
    assert!(bad.is_empty(), "{bad:#?}");
}

fn placement_with_scale(ds: f32, ds3d: [f32; 3]) -> MeshPlacement {
    MeshPlacement {
        actor: String::new(), class: String::new(), mesh: String::new(), mesh_ref: 0,
        location: [0.0; 3], rotation: [0; 3], draw_scale: ds, draw_scale_3d: ds3d,
        pre_pivot: [0.0; 3], hidden: false, skins: Vec::new(),
        vertex_colors: Vec::new(), unlit: false,
    }
}

#[test]
fn mirrored_placement_detection() {
    // Negative determinant: uniform scale counts 3x (sign(ds^3) == sign(ds)).
    assert!(!placement_with_scale(1.0, [1.0, 1.0, 1.0]).is_mirrored());
    assert!(!placement_with_scale(1.0, [-1.0, -1.0, 1.0]).is_mirrored()); // two flips = rotation
    assert!(!placement_with_scale(2.0, [1.0, 2.0, 1.0]).is_mirrored());
    assert!(placement_with_scale(-1.0, [1.0, 1.0, 1.0]).is_mirrored()); // uniform -1: det -1
    assert!(placement_with_scale(1.0, [1.0, -1.0, 1.0]).is_mirrored()); // WestLondon Car_Tunnel
    assert!(placement_with_scale(1.0, [1.0, -2.0, 1.0]).is_mirrored()); // Car_Tunnel_Front_Wall
}

#[test]
fn westlondon_has_mirrored_tunnel_shells() {
    // Guards the premise of the winding fix: the mapper mirrors tunnel shells.
    let Some(dir) = kf_dir() else { return };
    let data = std::fs::read(dir.join("Maps/KF-WestLondon.rom")).unwrap();
    let pkg = UnrealPackage::from_bytes(&data).unwrap();
    let mirrored: Vec<_> = static_mesh_placements(&pkg)
        .into_iter()
        .filter(|p| p.is_mirrored() && p.mesh.contains("Tunnel"))
        .collect();
    assert!(!mirrored.is_empty(), "expected mirrored Car_Tunnel placements");
    assert!(mirrored.iter().any(|p| p.mesh.contains("Car_Tunnel_Front_Wall")), "{}", mirrored.iter().map(|p| p.mesh.as_str()).collect::<Vec<_>>().join(","));
}
