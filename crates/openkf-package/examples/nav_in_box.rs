use openkf_package::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let g = openkf_package::navigation::read_nav_graph(&m.as_package());
    for (i, n) in g.nodes.iter().enumerate() {
        let p = n.location;
        if (p[0]+6179.0).hypot(p[1]-1210.0) < 700.0 { println!("{i} {:?} {:?}", n.class, p); }
    }
}
