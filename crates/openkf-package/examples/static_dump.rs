//! Dumps static mesh sections (triangles, material, UV/position bounds).
//!
//! Usage: cargo run --release -p openkf-package --example static_dump -- <package_dir_or_file> <MeshName>
use openkf_package::*;
use std::path::PathBuf;

fn main() {
    let kf = PathBuf::from(std::env::args().nth(1).expect("usage: <kf_dir_or_pkg_file> <MeshName>"));
    let want = std::env::args().nth(2).expect("usage: <kf_dir_or_pkg_file> <MeshName>");
    let files: Vec<PathBuf> = if kf.is_dir() {
        ["Maps", "StaticMeshes", "Textures", "Sounds", "Animations", "System"]
            .into_iter()
            .flat_map(|d| std::fs::read_dir(kf.join(d)).map(|r| r.flatten().map(|e| e.path()).collect::<Vec<_>>()).unwrap_or_default())
            .filter(|p| p.extension().map(|x| ["rom", "usx", "utx", "u", "uax"].iter().any(|e| x.eq_ignore_ascii_case(e))).unwrap_or(false))
            .collect()
    } else {
        vec![kf]
    };
    for f in &files {
        let data = match std::fs::read(f) { Ok(d) => d, Err(_) => continue };
        let pkg = match UnrealPackage::from_bytes(&data) { Ok(p) => p, Err(_) => continue };
        for i in 0..pkg.exports.len() {
            if export_class_name(&pkg, i) != "StaticMesh" { continue; }
            let name = pkg.exports.get(i).and_then(|e| pkg.resolve_name(e.object_name).ok()).unwrap_or_default();
            if !name.eq_ignore_ascii_case(&want) { continue; }
            let m = match read_static_mesh(&pkg, i) { Ok(m) => m, Err(e) => { println!("{}: error {e:?}", f.display()); continue } };
            println!("{} ({} verts, {} uv channels, {} indices):", m.name, m.positions.len(), m.uv_channels.len(), m.indices.len());
            let uv0 = m.uv_channels.first();
            for (si, s) in m.sections.iter().enumerate() {
                let tris = m.section_triangles(s);
                let mat = m.materials.get(si).map(|x| pkg.format_full_name(x.material)).unwrap_or_default();
                let (mut umn, mut umx) = ([f32::MAX; 2], [f32::MIN; 2]);
                let (mut pmn, mut pmx) = ([f32::MAX; 3], [f32::MIN; 3]);
                for t in &tris {
                    for &k in t {
                        if let Some(u) = uv0.and_then(|u| u.get(k as usize)) {
                            for a in 0..2 { umn[a] = umn[a].min(u[a]); umx[a] = umx[a].max(u[a]); }
                        }
                        if let Some(p) = m.positions.get(k as usize) {
                            for a in 0..3 { pmn[a] = pmn[a].min(p[a]); pmx[a] = pmx[a].max(p[a]); }
                        }
                    }
                }
                println!("  sec {si}: {} tris mat={mat} strip={} uvx=[{:.2},{:.2}] uvy=[{:.2},{:.2}] bb=[{:.0},{:.0},{:.0}]..[{:.0},{:.0},{:.0}]",
                    tris.len(), s.is_strip, umn[0], umx[0], umn[1], umx[1], pmn[0], pmn[1], pmn[2], pmx[0], pmx[1], pmx[2]);
            }
        }
    }
}
