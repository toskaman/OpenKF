use openkf_package::uc_defaults::ScriptDefaults;
fn main() {
    let d = ScriptDefaults::load(std::path::Path::new(&std::env::args().nth(1).unwrap()));
    let is_monster = |c: &str| { let mut cur = c.to_string(); for _ in 0..20 { if cur == "kfmonster" { return true; } match d.classes.get(&cur).and_then(|x| x.parent.clone()) { Some(p) => cur = p.to_ascii_lowercase(), None => return false } } false };
    let mut m = std::collections::BTreeMap::<String, Vec<String>>::new();
    for k in d.classes.keys() { if is_monster(k) { if let Some((kind, p, n)) = d.object_ref(k, "Mesh") { m.entry(format!("{kind} {p}.{n}")).or_default().push(k.clone()); } } }
    for (k, v) in &m { println!("{k}: {} classes e.g. {:?}", v.len(), &v[..v.len().min(3)]); }
}
