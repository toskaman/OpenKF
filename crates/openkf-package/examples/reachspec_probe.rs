use openkf_package::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let p = m.as_package();
    let mut n = 0;
    let mut classes = std::collections::BTreeMap::<String, usize>::new();
    for i in 0..p.exports.len() { *classes.entry(export_class_name(&p, i).to_string()).or_default() += 1; }
    for (k, v) in &classes { if k.contains("Path") || k.contains("Reach") || k.contains("Shop") || k.contains("Nav") || k.contains("Start") || k.contains("Spot") { println!("{k}: {v}"); } }
    for i in 0..p.exports.len() {
        if export_class_name(&p, i) != "ReachSpec" { continue; }
        let d = p.get_export_data(i).unwrap();
        println!("ReachSpec {} len {} {:02x?}", i, d.len(), &d[..d.len().min(40)]);
        n += 1;
        if n > 3 { break; }
    }
}
