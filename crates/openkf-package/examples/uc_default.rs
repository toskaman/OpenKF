//! `cargo run -p openkf-package --example uc_default -- <KF dir> <Class> <Key>...`
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let t = std::time::Instant::now();
    let d = openkf_package::uc_defaults::ScriptDefaults::load(std::path::Path::new(&a[0]));
    println!("{} classes in {:?}", d.classes.len(), t.elapsed());
    let mut cur = a[1].to_ascii_lowercase();
    for _ in 0..12 {
        let Some(c) = d.classes.get(&cur) else { println!("  (missing {cur})"); break };
        println!("  {cur} [{}] -> {:?}", c.package, c.parent);
        match &c.parent { Some(p) => cur = p.to_ascii_lowercase(), None => break }
    }
    for k in &a[2..] {
        println!("{}.{} = {:?}", a[1], k, d.get(&a[1], k));
    }
}
