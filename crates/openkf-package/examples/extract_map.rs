//! Loads a .rom map: world BSP + every placed static mesh (resolved from
//! StaticMeshes/*.usx or the map itself). Prints stats, optionally writes an OBJ
//! and a preview PNG (top-down + isometric, software rasterized).
//!
//! Usage: cargo run --release -p openkf-package --example extract_map -- <KF dir> <map.rom> [out.obj] [preview.png]
use openkf_package::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct Tri {
    p: [[f32; 3]; 3],
    n: [f32; 3],
    bsp: bool,
}

fn load_pkg_bytes(kf: &Path, name: &str, cache: &mut HashMap<String, Option<Vec<u8>>>) -> bool {
    if cache.contains_key(name) {
        return cache[name].is_some();
    }
    let mut found = None;
    for (dir, ext) in [("StaticMeshes", "usx"), ("Maps", "rom"), ("System", "u"), ("Textures", "utx"), ("Animations", "ukx")] {
        let p = kf.join(dir).join(format!("{name}.{ext}"));
        if p.exists() {
            found = std::fs::read(p).ok();
            break;
        }
    }
    let ok = found.is_some();
    cache.insert(name.to_string(), found);
    ok
}

fn find_export(pkg: &UnrealPackage<'_>, full_name: &str) -> Option<usize> {
    let short = full_name.split_once('.').map(|x| x.1).unwrap_or(full_name);
    static_mesh_exports(pkg).into_iter().find(|&i| {
        let n = pkg.format_full_name(i as i32 + 1);
        n.eq_ignore_ascii_case(full_name) || n.eq_ignore_ascii_case(short)
    })
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let kf = PathBuf::from(&a[0]);
    let rom = PathBuf::from(&a[1]);
    let t0 = std::time::Instant::now();
    let data = std::fs::read(&rom).unwrap();
    let pkg = UnrealPackage::from_bytes(&data).unwrap();
    let map_name = rom.file_stem().unwrap().to_string_lossy().to_string();

    let mut tris: Vec<Tri> = Vec::new();
    // --- BSP
    let model = read_level_model(&pkg).unwrap();
    let polys = model.polygons();
    let mut bsp_mats = std::collections::BTreeSet::new();
    for poly in &polys {
        bsp_mats.insert(model.surfs[poly.surf].material);
        for i in 1..poly.positions.len() - 1 {
            tris.push(Tri { p: [poly.positions[0], poly.positions[i], poly.positions[i + 1]], n: poly.normal, bsp: true });
        }
    }
    let bsp_tris = tris.len();
    let (mut ccw, mut cw) = (0, 0);
    for t in &tris {
        let e1 = [t.p[1][0] - t.p[0][0], t.p[1][1] - t.p[0][1], t.p[1][2] - t.p[0][2]];
        let e2 = [t.p[2][0] - t.p[0][0], t.p[2][1] - t.p[0][1], t.p[2][2] - t.p[0][2]];
        let c = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        if c[0] * t.n[0] + c[1] * t.n[1] + c[2] * t.n[2] > 0.0 { ccw += 1 } else { cw += 1 }
    }
    let _ = (ccw, cw);

    // --- Static meshes
    let placements = static_mesh_placements(&pkg);
    let mut cache: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    cache.insert(map_name.clone(), Some(data.clone()));
    let mut missing: HashMap<String, usize> = HashMap::new();
    let mut placed = 0usize;
    // Group placements by source package to parse each package once.
    let mut by_pkg: HashMap<String, Vec<&MeshPlacement>> = HashMap::new();
    for pl in &placements {
        if pl.hidden {
            continue;
        }
        let src = if pl.mesh_ref > 0 { map_name.clone() } else { pl.mesh.split('.').next().unwrap_or("").to_string() };
        by_pkg.entry(src).or_default().push(pl);
    }
    for (src, list) in &by_pkg {
        if !load_pkg_bytes(&kf, src, &mut cache) {
            *missing.entry(format!("package {src}")).or_default() += list.len();
            continue;
        }
        let bytes = cache[src].as_ref().unwrap();
        let Ok(spkg) = UnrealPackage::from_bytes(bytes) else { continue };
        let mut mesh_cache: HashMap<String, Option<StaticMesh>> = HashMap::new();
        for pl in list {
            let m = mesh_cache.entry(pl.mesh.clone()).or_insert_with(|| {
                let full = if pl.mesh_ref > 0 { pl.mesh.clone() } else { pl.mesh.clone() };
                find_export(&spkg, &full).and_then(|i| read_static_mesh(&spkg, i).ok())
            });
            let Some(m) = m else {
                *missing.entry(pl.mesh.clone()).or_default() += 1;
                continue;
            };
            placed += 1;
            let mir = pl.is_mirrored();
            for s in &m.sections {
                for t in m.section_triangles(s) {
                    let mut p = t.map(|i| pl.transform_point(m.positions[i as usize]));
                    if mir {
                        p.swap(1, 2);
                    }
                    let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
                    let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
                    // UE2 static meshes are wound clockwise: front normal = e2 x e1.
                    let mut n = [e2[1] * e1[2] - e2[2] * e1[1], e2[2] * e1[0] - e2[0] * e1[2], e2[0] * e1[1] - e2[1] * e1[0]];
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                    n = n.map(|x| x / l);
                    tris.push(Tri { p, n, bsp: false });
                }
            }
        }
    }
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("{map_name}: loaded in {load_ms:.0} ms");
    println!("  BSP: {} nodes, {} surfs, {} polygons -> {bsp_tris} triangles, {} materials", model.nodes.len(), model.surfs.len(), polys.len(), bsp_mats.len());
    println!("  Static meshes: {} placements, {placed} placed, {} triangles", placements.len(), tris.len() - bsp_tris);
    let mut miss: Vec<_> = missing.into_iter().collect();
    miss.sort_by(|a, b| b.1.cmp(&a.1));
    println!("  Unresolved: {} ({:?})", miss.iter().map(|x| x.1).sum::<usize>(), &miss[..miss.len().min(8)]);

    if let Some(obj_path) = a.get(2).filter(|s| s.ends_with(".obj")) {
        use std::io::Write;
        let mut w = std::io::BufWriter::new(std::fs::File::create(obj_path).unwrap());
        writeln!(w, "# {map_name} (Y-up)").unwrap();
        for t in &tris {
            for p in &t.p {
                writeln!(w, "v {} {} {}", p[0], p[2], p[1]).unwrap();
            }
        }
        for (i, t) in tris.iter().enumerate() {
            let b = i * 3 + 1;
            // BSP polys are counter-clockwise around their normal, meshes clockwise;
            // the axis swap mirrors both, so only BSP needs a flip for OBJ (CCW front).
            if t.bsp {
                writeln!(w, "f {} {} {}", b, b + 2, b + 1).unwrap();
            } else {
                writeln!(w, "f {} {} {}", b, b + 1, b + 2).unwrap();
            }
        }
    }
    if let Some(png_path) = a.iter().skip(2).find(|s| s.ends_with(".png")) {
        render_preview(&tris, png_path);
    }
}

// ---------------------------------------------------------------- preview
fn render_preview(tris: &[Tri], path: &str) {
    let (w, h) = (1600usize, 800usize);
    let mut img = vec![[17u8, 17, 17]; w * h];
    let light = { let l = [0.35f32, 0.45, 0.82]; let n = (l[0]*l[0]+l[1]*l[1]+l[2]*l[2]).sqrt(); l.map(|x| x / n) };
    // Bounds
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for t in tris { for p in &t.p { for k in 0..3 { mn[k] = mn[k].min(p[k]); mx[k] = mx[k].max(p[k]); } } }
    // Two views: top-down (x right, -y down... ) and isometric.
    for view in 0..2 {
        let ox = view * (w / 2);
        let vw = w / 2;
        let mut z = vec![f32::MIN; vw * h];
        let proj = |p: [f32; 3]| -> [f32; 3] {
            let c = [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0, (mn[2] + mx[2]) / 2.0];
            let q = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
            if view == 0 {
                [q[0], -q[1], q[2]] // top-down; depth = height
            } else {
                // isometric: yaw 45°, pitch 35°
                let (s, co) = (0.7071f32, 0.7071f32);
                let x = q[0] * co - q[1] * s;
                let y = q[0] * s + q[1] * co;
                let (sp, cp) = (0.5736f32, 0.8192f32);
                [x, -(q[2] * cp - y * sp), q[2] * sp + y * cp]
            }
        };
        let span = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]) * 1.02;
        let scale = (vw.min(h) as f32) / span * if view == 0 { 1.0 } else { 1.3 };
        for t in tris {
            let sh = (t.n[0] * light[0] + t.n[1] * light[1] + t.n[2] * light[2]).abs() * 0.7 + 0.3;
            let base = if t.bsp { [205.0, 200.0, 185.0] } else { [140.0, 175.0, 210.0] };
            let col = [(base[0] * sh) as u8, (base[1] * sh) as u8, (base[2] * sh) as u8];
            let v: Vec<[f32; 3]> = t.p.iter().map(|&p| { let q = proj(p); [q[0] * scale + vw as f32 / 2.0, q[1] * scale + h as f32 / 2.0, q[2]] }).collect();
            let minx = v.iter().map(|p| p[0]).fold(f32::MAX, f32::min).max(0.0) as usize;
            let maxx = (v.iter().map(|p| p[0]).fold(f32::MIN, f32::max).min(vw as f32 - 1.0)) as isize;
            let miny = v.iter().map(|p| p[1]).fold(f32::MAX, f32::min).max(0.0) as usize;
            let maxy = (v.iter().map(|p| p[1]).fold(f32::MIN, f32::max).min(h as f32 - 1.0)) as isize;
            if maxx < 0 || maxy < 0 { continue; }
            let area = (v[1][0] - v[0][0]) * (v[2][1] - v[0][1]) - (v[2][0] - v[0][0]) * (v[1][1] - v[0][1]);
            if area.abs() < 1e-6 { continue; }
            // Backface culling using the true normal against the view direction
            // (camera looks along -depth). Removes the outer shell of carved BSP.
            let nd = { let q0 = proj([0.0; 3]); let q1 = proj(t.n); q1[2] - q0[2] };
            if nd <= 0.0 { continue; }
            for y in miny..=maxy as usize {
                for x in minx..=maxx as usize {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((v[1][0] - px) * (v[2][1] - py) - (v[2][0] - px) * (v[1][1] - py)) / area;
                    let w1 = ((v[2][0] - px) * (v[0][1] - py) - (v[0][0] - px) * (v[2][1] - py)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 { continue; }
                    let d = w0 * v[0][2] + w1 * v[1][2] + w2 * v[2][2];
                    let zi = y * vw + x;
                    if d > z[zi] { z[zi] = d; img[y * w + ox + x] = col; }
                }
            }
        }
    }
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(&img.concat()).unwrap();
}
