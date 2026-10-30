//! `cargo run -p openkf-package --example class_dump -- <pkg.u> <ClassName>`
use openkf_package::{class_fields::read_class_info, MmapPackage};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let p = m.as_package();
    let idx = p.exports.entries.iter().position(|e| p.resolve_name(e.object_name).ok() == Some(a[1].as_str()) && e.class_index == 0).expect("class");
    let c = read_class_info(&p, idx).unwrap();
    println!("class {} extends {}", c.name, c.super_full);
    for f in &c.fields {
        println!("  {}{:<28} {:<16} dim={} pf={:08x} ff={:08x} {}", if f.is_net() { "*" } else { " " }, f.name, f.kind, f.array_dim, f.property_flags, f.function_flags, f.inner);
    }
}
