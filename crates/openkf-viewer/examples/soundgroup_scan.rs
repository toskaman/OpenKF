//! Dumps SoundGroup export headers to diagnose member parsing.
use openkf_package::{export_class_name, MmapPackage};
fn main() {
    let kf = std::env::args().nth(1).unwrap_or_else(|| "E:/SteamLibrary/steamapps/common/KillingFloor".into());
    let kf = std::path::PathBuf::from(kf);
    let mut shown = 0;
    for dir in ["Sounds", "Cache"] {
        let Ok(rd) = std::fs::read_dir(kf.join(dir)) else { continue };
        for e in rd.flatten() {
            let pth = e.path();
            if !pth.extension().is_some_and(|x| x == "uax" || x == "uxx" || x == "u") {
                continue;
            }
            let m = match MmapPackage::open(&pth) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let p = m.as_package();
            for i in 0..p.exports.len() {
                if export_class_name(&p, i) != "SoundGroup" {
                    continue;
                }
                let name = p.resolve_name(p.exports.get(i).unwrap().object_name).unwrap_or("?").to_string();
                let d = p.get_export_data(i).unwrap_or_default();
                println!("{}#{} len={} head={:02x?}", pth.file_name().unwrap().to_string_lossy(), name, d.len(), &d[..d.len().min(48)]);
                // Try current parser logic: two compact indices then count + refs.
                let mut o = 0usize;
                let a = openkf_package::read_compact_index(&d, &mut o);
                let b = openkf_package::read_compact_index(&d, &mut o);
                let c = openkf_package::read_compact_index(&d, &mut o);
                println!("  compact a={a:?} b={b:?} c={c:?} o={o}");
                shown += 1;
                if shown >= 15 {
                    return;
                }
            }
        }
    }
}
