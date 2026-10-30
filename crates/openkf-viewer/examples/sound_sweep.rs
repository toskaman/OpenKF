//! Sound sweep: decode every `Sound` export in every package under a directory
//! through the exact live chain (`export_data` -> `embedded_wav` ->
//! `WavSound::parse`) and report ok/fail by reason.
//!
//! Usage: `sound_sweep <dir>` — e.g. the OpenKF download folder or a game
//! `Sounds` folder. Exit 0 always; the counts are the result.

use openkf_audio::wav::WavSound;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["u", "uax", "ukx", "utx", "usx", "rom", "uxx"].iter().any(|x| e.eq_ignore_ascii_case(x)))
        })
        .collect();
    files.sort();
    let (mut ok, mut no_data, mut no_riff, mut bad_wav, mut shown) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for path in &files {
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => m,
            Err(_) => continue,
        };
        let pkg = match openkf_package::UnrealPackage::from_bytes(&mmap) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let mut n = 0u32;
        for i in 0..pkg.exports.len() {
            if openkf_package::export_class_name(&pkg, i) != "Sound" {
                continue;
            }
            n += 1;
            let data = match pkg.get_export_data(i) {
                Ok(d) => d,
                Err(_) => {
                    no_data += 1;
                    continue;
                }
            };
            let Some(wav) = openkf_package::sound::embedded_wav(data) else {
                no_riff += 1;
                if shown < 15 {
                    shown += 1;
                    println!("[NoRiff] {} export {i}", path.display());
                }
                continue;
            };
            match WavSound::parse(wav) {
                Ok(_) => ok += 1,
                Err(e) => {
                    bad_wav += 1;
                    if shown < 15 {
                        shown += 1;
                        println!("[BadWav] {} export {i}: {e:?}", path.display());
                    }
                }
            }
        }
        if n > 0 {
            println!("{}: {n} sounds", path.file_name().unwrap().to_string_lossy());
        }
    }
    println!("TOTAL ok={ok} no_data={no_data} no_riff={no_riff} bad_wav={bad_wav}");
}
