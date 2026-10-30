//! Lists MeshAnimations of a .ukx: <file.ukx>
use openkf_package::*;
fn main() {
    let d = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let pkg = UnrealPackage::from_bytes(&d).unwrap();
    for i in 0..pkg.exports.len() {
        if export_class_name(&pkg, i) != "MeshAnimation" { continue; }
        match read_mesh_animation(&pkg, i) {
            Ok(a) => println!("{:<28} bones {:>3} seqs {:>3} e.g. {:?}", a.name, a.bone_names.len(), a.sequences.len(),
                a.sequences.iter().take(6).map(|s| format!("{}({}f@{})", s.name, s.num_frames, s.rate)).collect::<Vec<_>>()),
            Err(e) => println!("{}: ERR {e:?}", pkg.format_full_name(i as i32 + 1)),
        }
    }
}
