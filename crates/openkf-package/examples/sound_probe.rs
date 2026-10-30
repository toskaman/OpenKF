use openkf_package::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let p = m.as_package();
    let mut n = 0;
    for i in 0..p.exports.len() {
        if export_class_name(&p, i) != "Sound" { continue; }
        let d = p.get_export_data(i).unwrap();
        let name = p.resolve_name(p.exports.get(i).unwrap().object_name).unwrap_or("?");
        let riff = d.windows(4).position(|w| w == b"RIFF");
        println!("{name} size={} riff_at={riff:?} head={:02x?}", d.len(), &d[..d.len().min(24)]);
        n += 1;
        if n > 4 { break; }
    }
}
