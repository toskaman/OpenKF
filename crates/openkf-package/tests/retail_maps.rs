//! Loads every retail KF-*.rom: world BSP + static mesh placements.
//! Set KF_DIR to the game root (defaults to E:\SteamLibrary\steamapps\common\KillingFloor).
use openkf_package::*;
use std::path::PathBuf;

fn kf_dir() -> Option<PathBuf> {
    let p = std::env::var("KF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"E:\SteamLibrary\steamapps\common\KillingFloor"));
    p.join("Maps").is_dir().then_some(p)
}

#[test]
fn westlondon_bsp_and_placements() {
    let Some(dir) = kf_dir() else { return };
    let data = std::fs::read(dir.join("Maps/KF-WestLondon.rom")).unwrap();
    let pkg = UnrealPackage::from_bytes(&data).unwrap();
    let model = read_level_model(&pkg).unwrap();
    assert_eq!(model.nodes.len(), 2347);
    assert_eq!(model.surfs.len(), 1046);
    assert_eq!(model.num_zones, 26);
    assert!(model.polygons().len() > 2000);
    let placements = static_mesh_placements(&pkg);
    assert_eq!(placements.len(), 1681);
}

#[test]
fn all_retail_maps_load() {
    let Some(dir) = kf_dir() else { return };
    let mut bad = Vec::new();
    let mut n = 0;
    for e in std::fs::read_dir(dir.join("Maps")).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("KF-") || !name.ends_with(".rom") {
            continue;
        }
        n += 1;
        let data = std::fs::read(e.path()).unwrap();
        match UnrealPackage::from_bytes(&data) {
            Ok(pkg) => {
                if let Err(err) = read_level_model(&pkg) {
                    bad.push(format!("{name}: BSP {err:?}"));
                }
                let _ = static_mesh_placements(&pkg);
            }
            Err(err) => bad.push(format!("{name}: {err:?}")),
        }
    }
    assert!(n > 0);
    assert!(bad.is_empty(), "{bad:#?}");
}
