//! Scans Sound exports in retail + cached + downloaded packages, reports parse failures.
//! Usage: sound_scan [kf_dir]
use openkf_package::{export_class_name, MmapPackage};

fn main() {
    let kf = std::env::args().nth(1).unwrap_or_else(|| "E:/SteamLibrary/steamapps/common/KillingFloor".into());
    let kf = std::path::PathBuf::from(kf);
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in ["Sounds", "Cache"] {
        if let Ok(rd) = std::fs::read_dir(kf.join(dir)) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "uax" || x == "uxx" || x == "u") {
                    files.push(p);
                }
            }
        }
    }
    let dl = openkf_network::download::download_dir(&kf);
    if let Ok(rd) = std::fs::read_dir(&dl) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "uax" || x == "u") {
                files.push(p);
            }
        }
    }
    let (mut total, mut ok, mut no_riff, mut parse_fail) = (0u32, 0u32, 0u32, 0u32);
    let mut fails: Vec<String> = Vec::new();
    for f in &files {
        let m = match MmapPackage::open(f) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let p = m.as_package();
        for i in 0..p.exports.len() {
            if export_class_name(&p, i) != "Sound" {
                continue;
            }
            total += 1;
            let name = p.resolve_name(p.exports.get(i).unwrap().object_name).unwrap_or("?").to_string();
            let d = match p.get_export_data(i) {
                Ok(d) => d,
                Err(e) => {
                    parse_fail += 1;
                    if fails.len() < 30 {
                        fails.push(format!("{}#{name}: export_data err {e}", f.file_name().unwrap().to_string_lossy()));
                    }
                    continue;
                }
            };
            let Some(wav) = openkf_package::sound::embedded_wav(&d) else {
                no_riff += 1;
                if fails.len() < 30 {
                    let riff = d.windows(4).position(|w| w == b"RIFF");
                    fails.push(format!("{}#{name}: no RIFF in 64B (len {}, riff_at {riff:?}, head {:02x?})", f.file_name().unwrap().to_string_lossy(), d.len(), &d[..d.len().min(32)]));
                }
                continue;
            };
            match openkf_audio::wav::WavSound::parse(wav) {
                Ok(_) => ok += 1,
                Err(e) => {
                    parse_fail += 1;
                    if fails.len() < 30 {
                        // Peek format tag / channels / bits for diagnosis.
                        let peek = if wav.len() >= 28 { format!("fmt={:#x} ch={} bits={}", u16::from_le_bytes([wav[20], wav[21]]), u16::from_le_bytes([wav[22], wav[23]]), u16::from_le_bytes([wav[34], wav[35]])) } else { "short".into() };
                        fails.push(format!("{}#{name}: wav err {e} ({peek}, len {})", f.file_name().unwrap().to_string_lossy(), wav.len()));
                    }
                }
            }
        }
    }
    println!("files={} sounds={total} ok={ok} no_riff={no_riff} parse_fail={parse_fail}", files.len());
    for f in fails {
        println!("FAIL {f}");
    }
}
