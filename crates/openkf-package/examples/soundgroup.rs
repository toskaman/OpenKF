use openkf_package::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let p = m.as_package();
    for i in 0..p.exports.len() {
        let n = p.resolve_name(p.exports.get(i).unwrap().object_name).unwrap_or("?");
        if n != a[1] { continue; }
        println!("{n} class {}", export_class_name(&p, i));
        let d = p.get_export_data(i).unwrap();
        println!("len {} head {:02x?}", d.len(), &d[..d.len().min(64)]);
        match read_object_properties(&p, i) { Ok(pl) => println!("props {:?}", pl), Err(e) => println!("props err {e}") }
    }
}
