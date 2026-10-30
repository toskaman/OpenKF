//! Decodes every texture of the retail install. Set KF_DIR to the game root
//! (defaults to E:\SteamLibrary\steamapps\common\KillingFloor); skipped if absent.
use openkf_package::*;
use std::path::PathBuf;

fn kf_dir() -> Option<PathBuf> {
    let p = std::env::var("KF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"E:\SteamLibrary\steamapps\common\KillingFloor"));
    p.join("Textures").is_dir().then_some(p)
}

#[test]
fn decode_architecture_t_dxt1() {
    let Some(dir) = kf_dir() else { return };
    let data = std::fs::read(dir.join("Textures/Architecture_T.utx")).unwrap();
    let pkg = UnrealPackage::from_bytes(&data).unwrap();
    let texs = texture_exports(&pkg);
    assert!(!texs.is_empty());
    let t = read_texture(&pkg, texs[0]).unwrap();
    assert_eq!(t.format, TextureFormat::Dxt1);
    assert_eq!((t.width, t.height), (1024, 1024));
    assert_eq!(t.mips.len(), 11);
    let (w, h, rgba) = t.decode_rgba(0).unwrap();
    assert_eq!(rgba.len(), (w * h * 4) as usize);
}

#[test]
fn decode_all_retail_textures() {
    let Some(dir) = kf_dir() else { return };
    let (mut ok, mut bad) = (0, Vec::new());
    for e in std::fs::read_dir(dir.join("Textures")).unwrap().flatten() {
        let data = std::fs::read(e.path()).unwrap();
        let Ok(pkg) = UnrealPackage::from_bytes(&data) else { continue }; // e.g. old v121 sample
        for idx in texture_exports(&pkg) {
            match read_texture(&pkg, idx) {
                Ok(_) => ok += 1,
                Err(err) => bad.push(format!("{:?} #{idx}: {err:?}", e.file_name())),
            }
        }
    }
    assert!(ok > 7000, "only {ok} textures parsed");
    assert!(bad.is_empty(), "{bad:#?}");
}
