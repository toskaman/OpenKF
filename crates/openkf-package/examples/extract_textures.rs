//! Exports every texture of one or more UE2 packages to PNG (mip 0).
//!
//! Usage: cargo run --release -p openkf-package --example extract_textures -- <out_dir> <file.utx|dir>...
use openkf_package::*;
use std::path::{Path, PathBuf};

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let f = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut wr = enc.write_header().map_err(std::io::Error::other)?;
    wr.write_image_data(rgba).map_err(std::io::Error::other)?;
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    // Pass "--check" as <out_dir> to decode everything without writing PNGs.
    let out_arg = args.next().expect("usage: <out_dir|--check> <files|dirs>...");
    let check_only = out_arg == "--check";
    let out = PathBuf::from(out_arg);
    let mut files = Vec::new();
    for a in args {
        let p = PathBuf::from(a);
        if p.is_dir() {
            for e in std::fs::read_dir(&p).unwrap().flatten() {
                let ep = e.path();
                if ep.extension().map(|x| x.eq_ignore_ascii_case("utx")).unwrap_or(false) {
                    files.push(ep);
                }
            }
        } else {
            files.push(p);
        }
    }
    files.sort();
    let (mut ok, mut unsupported, mut failed) = (0usize, 0usize, 0usize);
    let mut fmt_count = std::collections::BTreeMap::<String, usize>::new();
    for f in &files {
        let data = match std::fs::read(f) { Ok(d) => d, Err(e) => { eprintln!("{}: {e}", f.display()); continue; } };
        let pkg = match UnrealPackage::from_bytes(&data) { Ok(p) => p, Err(e) => { eprintln!("{}: {e:?}", f.display()); failed += 1; continue; } };
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        let dir = out.join(&stem);
        if !check_only {
            std::fs::create_dir_all(&dir).unwrap();
        }
        for idx in texture_exports(&pkg) {
            match read_texture(&pkg, idx) {
                Ok(t) => {
                    *fmt_count.entry(format!("{:?}", t.format)).or_default() += 1;
                    match t.decode_rgba(0) {
                        Some((w, h, rgba)) => {
                            let name = t.name.replace(['/', '\\', ':'], "_");
                            if !check_only {
                                write_png(&dir.join(format!("{name}.png")), w, h, &rgba).unwrap();
                            }
                            ok += 1;
                        }
                        None => unsupported += 1,
                    }
                }
                Err(e) => { eprintln!("{} #{idx}: {e:?}", stem); failed += 1; }
            }
        }
    }
    println!("{} packages | exported {ok} | unsupported format {unsupported} | errors {failed}", files.len());
    println!("formats: {fmt_count:?}");
}
