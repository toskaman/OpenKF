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
}
