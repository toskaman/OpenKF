use openkf_package::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let p = m.as_package();
    let t = std::time::Instant::now();
    let g = navigation::read_nav_graph(&p);
    let edges: usize = g.edges.iter().map(|e| e.len()).sum();
    println!("{} nodes, {edges} edges in {:?}", g.nodes.len(), t.elapsed());
    let shops: Vec<(usize, [f32; 3])> = (0..p.exports.len()).filter(|&i| export_class_name(&p, i) == "ShopVolume").filter_map(|i| match read_object_properties(&p, i).ok()?.get("Location") { Some(PropValue::Vector(v)) => Some((i, *v)), _ => None }).collect();
    println!("shops {:?}", shops);
    let start = [-3110.0, 1313.0, -3807.0];
    for (i, s) in &shops {
        let path = g.path(start, *s);
        println!("to shop export {i}: {:?} points", path.as_ref().map(|p| p.len()));
    }
}
