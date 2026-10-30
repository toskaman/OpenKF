use openkf_package::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = MmapPackage::open(&a[0]).unwrap();
    let p = m.as_package();
    let i: usize = a[1].parse().unwrap();
    let md = read_model(&p, i).unwrap();
    let mut lo = [f32::MAX; 3]; let mut hi = [f32::MIN; 3];
    for v in &md.points { for k in 0..3 { lo[k] = lo[k].min(v[k]); hi[k] = hi[k].max(v[k]); } }
    println!("{} points lo {lo:?} hi {hi:?}", md.points.len());
}
