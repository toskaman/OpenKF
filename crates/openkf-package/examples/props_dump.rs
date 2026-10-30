use openkf_package::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let p = m.as_package();
    let i: usize = a[1].parse().unwrap();
    println!("{} ({})", p.resolve_name(p.exports.get(i).unwrap().object_name).unwrap_or("?"), export_class_name(&p, i));
    for pr in read_object_properties(&p, i).unwrap().props {
        let v = match &pr.value { PropValue::Object(r) => format!("Object({r}) = {}", p.format_full_name(*r)), v => format!("{v:?}") };
        println!("  {}[{}] = {}", pr.name, pr.array_index, v.chars().take(120).collect::<String>());
    }
}
