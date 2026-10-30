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
