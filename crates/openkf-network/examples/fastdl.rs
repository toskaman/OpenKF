//! Fetches one package from a FastDL redirect server, like the client does for
//! missing `USES` packages.
//!
//! cargo run --release -p openkf-network --example fastdl -- http://host:port/ KF-Foo.rom [out_dir]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: fastdl <redirect url> <file name> [out dir]");
        std::process::exit(1);
    }
    let out = a.get(3).map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("openkf-fastdl"));
    if std::env::var_os("FASTDL_PROBE").is_some() {
        for name in [format!("{}.uz2", a[2]), a[2].clone()] {
            let url = format!("{}{}", a[1], openkf_network::download::url_encode(&name));
            match openkf_network::download::http_get(&url, std::time::Duration::from_secs(20)) {
                Ok(b) => {
                    let _ = std::fs::create_dir_all(&out);
                    let _ = std::fs::write(out.join(format!("probe_{}", b.len())), &b);
                    println!("{url}: {} bytes, head {:02X?} {:?}", b.len(), &b[..b.len().min(24)], String::from_utf8_lossy(&b[..b.len().min(80)]));
                }
                Err(e) => println!("{url}: {e}"),
            }
        }
    }
    let t = std::time::Instant::now();
    match openkf_network::download::fetch_package(&a[1], true, &a[2], None, &out) {
        Ok(p) => {
            let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            println!("ok {} ({} KB, GUID {:?}) in {:.1} s", p.display(), len / 1024, openkf_network::download::package_guid(&p), t.elapsed().as_secs_f32());
        }
        Err(e) => {
            eprintln!("failed: {e}");
            std::process::exit(1);
        }
    }
}
