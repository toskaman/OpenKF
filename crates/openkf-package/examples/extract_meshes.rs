//! Exports every static mesh of UE2 packages to Wavefront OBJ (+ MTL naming the materials).
//!
//! Usage: cargo run --release -p openkf-package --example extract_meshes -- <out_dir|--check> <file.usx|dir>...
use openkf_package::*;
use std::fmt::Write as _;
use std::path::PathBuf;

fn to_obj(m: &StaticMesh) -> (String, String) {
    let mut obj = String::new();
    let mut mtl = String::new();
    let short = m.name.rsplit('.').next().unwrap_or(&m.name).to_string();
    writeln!(obj, "# {}\nmtllib {short}.mtl\no {short}", m.name).unwrap();
    // UE2 is Z-up, left-handed: export as Y-up right-handed (x, z, y).
    for p in &m.positions {
        writeln!(obj, "v {} {} {}", p[0], p[2], p[1]).unwrap();
    }
    let uv = m.uv_channels.first();
    if let Some(uv) = uv {
        for t in uv {
            writeln!(obj, "vt {} {}", t[0], 1.0 - t[1]).unwrap();
        }
    }
    for n in &m.normals {
        writeln!(obj, "vn {} {} {}", n[0], n[2], n[1]).unwrap();
    }
    for (si, s) in m.sections.iter().enumerate() {
        let mat = m.materials.get(si).map(|x| x.material_name.as_str()).filter(|s| !s.is_empty()).unwrap_or("None");
        let mname = format!("mat{si}_{}", mat.replace(' ', "_"));
        writeln!(mtl, "newmtl {mname}\nmap_Kd {mat}.png\n").unwrap();
        writeln!(obj, "usemtl {mname}").unwrap();
        for t in m.section_triangles(s) {
            // UE2 is clockwise; the (x,z,y) axis swap mirrors it to counter-clockwise (OBJ front).
            let f: Vec<String> = [t[0], t[1], t[2]]
                .iter()
                .map(|&i| {
                    let i = i as usize + 1;
                    if uv.is_some() { format!("{i}/{i}/{i}") } else { format!("{i}//{i}") }
                })
                .collect();
            writeln!(obj, "f {}", f.join(" ")).unwrap();
        }
    }
    (obj, mtl)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out_arg = args.next().expect("usage: <out_dir|--check> <files|dirs>...");
    let check_only = out_arg == "--check";
    let out = PathBuf::from(&out_arg);
    let mut files = Vec::new();
    for a in args {
        let p = PathBuf::from(a);
        if p.is_dir() {
            for e in std::fs::read_dir(&p).unwrap().flatten() {
                let ep = e.path();
                if ep.extension().map(|x| x.eq_ignore_ascii_case("usx")).unwrap_or(false) {
                    files.push(ep);
                }
            }
        } else {
            files.push(p);
        }
    }
    files.sort();
    let (mut ok, mut failed, mut tris, mut verts) = (0usize, 0usize, 0usize, 0usize);
    for f in &files {
        let data = match std::fs::read(f) { Ok(d) => d, Err(e) => { eprintln!("{}: {e}", f.display()); continue; } };
        let pkg = match UnrealPackage::from_bytes(&data) { Ok(p) => p, Err(e) => { eprintln!("{}: {e:?}", f.display()); failed += 1; continue; } };
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        for idx in static_mesh_exports(&pkg) {
            match read_static_mesh(&pkg, idx) {
                Ok(m) => {
                    // Sanity: every index must reference a vertex.
                    if m.indices.iter().any(|&i| i as usize >= m.positions.len()) {
                        eprintln!("{stem}: {} has out-of-range indices", m.name);
                        failed += 1;
                        continue;
                    }
                    ok += 1;
                    verts += m.positions.len();
                    tris += m.sections.iter().map(|s| m.section_triangles(s).len()).sum::<usize>();
                    if !check_only {
                        let dir = out.join(&stem);
                        std::fs::create_dir_all(&dir).unwrap();
                        let short = m.name.rsplit('.').next().unwrap_or(&m.name).replace(['/', '\\', ':'], "_");
                        let (obj, mtl) = to_obj(&m);
                        std::fs::write(dir.join(format!("{short}.obj")), obj).unwrap();
                        std::fs::write(dir.join(format!("{short}.mtl")), mtl).unwrap();
                    }
                }
                Err(e) => { eprintln!("{stem} #{idx}: {e:?}"); failed += 1; }
            }
        }
    }
    println!("{} packages | meshes {ok} | errors {failed} | {verts} vertices | {tris} triangles", files.len());
}
