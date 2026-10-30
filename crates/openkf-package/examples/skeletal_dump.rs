//! Lists skeletal meshes of a .ukx and validates their geometry: <file.ukx>
use openkf_package::*;
fn main() {
    let d = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let pkg = UnrealPackage::from_bytes(&d).unwrap();
    for i in 0..pkg.exports.len() {
        if export_class_name(&pkg, i) != "SkeletalMesh" { continue; }
        match read_skeletal_mesh(&pkg, i) {
            Ok(m) => {
                let bad_w = m.wedges.iter().filter(|w| w.point as usize >= m.points.len()).count();
                let bad_t = m.triangles.iter().filter(|t| t.wedges.iter().any(|&w| w as usize >= m.wedges.len())).count();
                let bad_i = m.influences.iter().filter(|x| x.point as usize >= m.points.len() || x.bone as usize >= m.bones.len()).count();
                let mut wsum = vec![0f32; m.points.len()];
                for x in &m.influences { if (x.point as usize) < wsum.len() { wsum[x.point as usize] += x.weight; } }
                let off = wsum.iter().filter(|&&w| (w - 1.0).abs() > 0.02).count();
                println!("scale {:?} origin {:?} rot {:?}", m.mesh_scale, m.mesh_origin, m.rot_origin);
                println!("{:<40} v{} bones {:>3} pts {:>5} wedges {:>5} tris {:>5} infl {:>5} | bad w/t/i {bad_w}/{bad_t}/{bad_i}, weight!=1: {off} | anim {} | tex {:?}",
                    m.name, m.version, m.bones.len(), m.points.len(), m.wedges.len(), m.triangles.len(), m.influences.len(),
                    pkg.format_full_name(m.animation), m.textures.iter().map(|&t| pkg.format_full_name(t)).collect::<Vec<_>>());
            }
            Err(e) => println!("{}: ERR {e:?}", pkg.format_full_name(i as i32 + 1)),
        }
    }
}
