//! Reads a map's level Model and prints sections / lightmaps summary: <map.rom>
use openkf_package::*;
fn main() {
    let d = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let pkg = UnrealPackage::from_bytes(&d).unwrap();
    let m = read_level_model(&pkg).unwrap();
    let used: std::collections::BTreeSet<i32> = m.sections.iter().map(|s| s.light_map_texture).collect();
    let bad_nodes = m.nodes.iter().filter(|n| n.num_vertices >= 3 && n.i_section >= 0 && m.sections.get(n.i_section as usize).map_or(true, |s| (n.i_first_vertex + n.num_vertices as i32) as usize > s.vertices.len())).count();
    println!("{} nodes, {} sections ({} verts), {} lightmaps {:?}, used idx {:?}, nodes out of section range: {bad_nodes}",
        m.nodes.len(), m.sections.len(), m.sections.iter().map(|s| s.vertices.len()).sum::<usize>(), m.lightmaps.len(),
        m.lightmaps.iter().map(|l| (l.width, l.dxt5)).collect::<std::collections::BTreeSet<_>>(), used);
    for (i, s) in m.sections.iter().enumerate().take(12) {
        let (mut mnx, mut mxx, mut mny, mut mxy) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for v in &s.vertices {
            mnx = mnx.min(v.lightmap_uv[0]); mxx = mxx.max(v.lightmap_uv[0]);
            mny = mny.min(v.lightmap_uv[1]); mxy = mxy.max(v.lightmap_uv[1]);
        }
        println!("sec {i}: lm_tex={} n={} lmuv x=[{mnx:.3},{mxx:.3}] y=[{mny:.3},{mxy:.3}]", s.light_map_texture, s.vertices.len());
    }
    let (mut aux, mut aux2, mut auy, mut auy2) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for s in m.sections.iter() { for v in &s.vertices {
        aux = aux.min(v.uv[0]); aux2 = aux2.max(v.uv[0]); auy = auy.min(v.uv[1]); auy2 = auy2.max(v.uv[1]);
    } }
    println!("section diffuse uv global: x=[{aux:.1},{aux2:.1}] y=[{auy:.1},{auy2:.1}]");
    // Floor nodes (plane normal ~ +Z): compare stored section UVs vs texel_uv recompute.
    let mut shown = 0;
    for (ni, n) in m.nodes.iter().enumerate() {
        if n.num_vertices < 3 || n.i_surf < 0 || n.plane[2] < 0.9 { continue; }
        let su = &m.surfs[n.i_surf as usize];
        let sec_ok = n.i_section >= 0 && m.sections.get(n.i_section as usize).map_or(false, |s| (n.i_first_vertex.max(0) as usize + n.num_vertices as usize) <= s.vertices.len());
        let mut stored = Vec::new();
        let mut recomp = Vec::new();
        if sec_ok {
            let s = &m.sections[n.i_section as usize];
            for v in &s.vertices[n.i_first_vertex as usize..n.i_first_vertex as usize + n.num_vertices as usize] {
                stored.push(v.uv);
            }
        }
        let st = n.i_vert_pool.max(0) as usize;
        if let Some(vs) = m.verts.get(st..st + n.num_vertices as usize) {
            for v in vs.iter().filter_map(|v| m.points.get(v.p_vertex as usize)) {
                recomp.push(m.texel_uv(su, *v));
            }
        }
        let spread = |u: &[[f32; 2]]| u.iter().fold(0.0f32, |a, x| a.max((x[0] - u[0][0]).abs()).max((x[1] - u[0][1]).abs()));
        if shown < 14 {
            println!("node {ni}: sec={} mat={} p_base={} vtu={} vtv={} stored_n={} stored_spread={:.1} recomp_n={} recomp_spread={:.1} stored0={:?}",
                n.i_section, pkg.format_full_name(su.material), su.p_base, su.v_texture_u, su.v_texture_v,
                stored.len(), if stored.is_empty() { -1.0 } else { spread(&stored) },
                recomp.len(), if recomp.is_empty() { -1.0 } else { spread(&recomp) },
                stored.first());
            shown += 1;
        }
    }
}
