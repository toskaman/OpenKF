//! Builds the package map from a capture + local game files, then dumps actor
//! channel openings. `cargo run -p openkf-network --example actors -- <capture> <KF dir>`
use openkf_network::package_map::*;
use openkf_network::wire::*;
use openkf_package::MmapPackage;
use std::path::{Path, PathBuf};

const DIRS: &[&str] = &["System", "Maps", "Textures", "Sounds", "StaticMeshes", "Animations", "Music", "KarmaData"];

fn find(kf: &Path, fname: &str) -> Option<PathBuf> {
    DIRS.iter().map(|d| kf.join(d).join(fname)).find(|p| p.exists())
}

pub fn load(kf: &Path, u: &UsesLine) -> Option<PackageExports> {
    let path = find(kf, &u.fname)?;
    let pkg = MmapPackage::open(&path).ok()?;
    let p = pkg.as_package();
    let exports = p
        .exports
        .entries
        .iter()
        .map(|e| {
            let name = p.resolve_name(e.object_name).unwrap_or("?").to_string();
            let class = if e.class_index == 0 {
                "Class".to_string()
            } else {
                p.resolve_object_ref(e.class_index)
                    .ok()
                    .map(|_| p.format_full_name(e.class_index))
                    .unwrap_or_default()
            };
            (name, class)
        })
        .collect();
    let generation_exports = p.summary.generations.iter().map(|g| g.export_count as u32).collect();
    Some(PackageExports { exports, generation_exports })
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let kf = PathBuf::from(a.get(1).cloned().unwrap_or("/mnt/KillingFloor".into()));
    let recs = parse_capture(&std::fs::read_to_string(&a[0]).unwrap());
    let mut uses = Vec::new();
    let mut opens = Vec::new();
    for r in &recs {
        let Ok(p) = Packet::decode(&r.bytes) else { continue };
        for it in p.items {
            let Item::Bunch(b) = it else { continue };
            if r.from_client {
                continue;
            }
            if b.ch_index == 0 && b.reliable {
                for m in control_messages(&b).unwrap_or_default() {
                    if let Some(u) = UsesLine::parse(&m) {
                        uses.push(u);
                    }
                }
            } else if b.open && b.ch_type == Some(CHTYPE_ACTOR) {
                opens.push((r.ms, b));
            }
        }
    }
    let map = PackageMap::build(uses, |u| load(&kf, u));
    let missing: Vec<_> = map.packages.iter().filter(|p| p.local.is_none()).map(|p| p.uses.fname.clone()).collect();
    println!("packages={} max_object_index={} missing={:?}", map.packages.len(), map.max_object_index, missing);
    for p in map.packages.iter().take(5) {
        println!("  {} base={} count={}", p.uses.pkg, p.object_base, p.object_count);
    }
    if let Ok(q) = std::env::var("FINDCLS") {
        for p in &map.packages {
            if let Some(l) = &p.local {
                for (i, (n, c)) in l.exports.iter().enumerate() {
                    if q.split(',').any(|w| w == n) && c == "Class" {
                        println!("FIND {}.{} idx={} (base {} ei {})", p.uses.pkg, n, p.object_base + i as u32, p.object_base, i);
                    }
                }
            }
        }
        for p in &map.packages { if p.uses.pkg=="XGame"||p.uses.pkg=="KFMod"||p.uses.pkg=="ROEffects" { println!("BASE {} {} {}", p.uses.pkg, p.object_base, p.object_count);} }
    }
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let (mut lvl, mut cls, mut bad) = (0, 0, 0);
    let mut classes = std::collections::BTreeMap::<String, usize>::new();
    for (k, (ms, b)) in opens.iter().enumerate() {
        let mut r = b.reader();
        let o = openkf_network::actor_open::read_actor_open(&mut r, map.max_object_index).unwrap();
        let obj = map.resolve(o.object_index);
        let (pkg, name, class) = obj.as_ref().map(|o| (o.package, o.name.unwrap_or("?"), o.class.unwrap_or("?"))).unwrap_or(("?", "?", "?"));
        let is_level = map.packages.first().map(|p| p.uses.pkg.as_str()) == Some(pkg);
        let mut loc = None;
        if class == "Class" {
            cls += 1;
            *classes.entry(format!("{pkg}.{name}")).or_default() += 1;
            loc = openkf_network::actor_open::read_compressed_vector(&mut r).ok();
        } else if is_level {
            lvl += 1;
        } else {
            bad += 1;
            println!("SUSPECT {ms} ch{} idx={} {pkg}.{name} ({class}) flag={} bits={}", b.ch_index, o.object_index, o.flag, b.data_bits);
        }
        if k < n {
            println!("{ms:>7} ch{:<4} {pkg}.{name} [{class}] loc={loc:?} rest={} bits", b.ch_index, r.remaining());
        }
    }
    println!("opens={} level={lvl} dynamic={cls} suspect={bad}", opens.len());
    for (c, k) in classes { println!("  {k:>4} x {c}"); }
}
