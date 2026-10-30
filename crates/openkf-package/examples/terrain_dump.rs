use openkf_package::*;
fn main() {
    let d = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let pkg = UnrealPackage::from_bytes(&d).unwrap();
    for i in 0..pkg.exports.len() {
        if export_class_name(&pkg, i) != "TerrainInfo" { continue; }
        let props = read_object_properties(&pkg, i).unwrap();
        for p in props.props.iter().filter(|p| p.name == "Layers") {
            if let PropValue::Struct(n, raw) = &p.value {
                println!("Layers[{}] struct {n} {}b", p.array_index, raw.len());
                match read_properties(&pkg, raw, 0) {
                    Ok(lp) => for q in &lp.props { println!("   {} = {:?}", q.name, q.value); },
                    Err(e) => println!("   ERR {e:?}"),
                }
            }
        }
    }
    for t in read_terrains(&pkg) {
        let (mn, mx) = (t.heights.iter().min().unwrap(), t.heights.iter().max().unwrap());
        println!("{} loc {:?} scale {:?} {}x{} heights {mn}..{mx}", t.actor, t.location, t.scale, t.width, t.height);
        for l in &t.layers { println!("  layer {} alpha {} u {} v {}", pkg.format_full_name(l.texture), pkg.format_full_name(l.alpha_map), l.u_scale, l.v_scale); }
    }
}
