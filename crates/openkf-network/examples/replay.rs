//! Decodes every actor-channel bunch of a capture (server -> client).
//! `cargo run -p openkf-network --example replay -- <capture> <KF dir> [verbose channel]`
use openkf_network::actor_open::*;
use openkf_network::package_map::UsesLine;
use openkf_network::props::*;
use openkf_network::schema::*;
use openkf_network::wire::*;
use std::collections::HashMap;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let recs = parse_capture(&std::fs::read_to_string(&a[0]).unwrap());
    let kf = std::path::PathBuf::from(&a[1]);
    let verbose: Option<u32> = a.get(2).and_then(|x| x.parse().ok());
    let rot = std::env::var("OPEN_ROT").map(|v| v == "1").unwrap_or(false);
    let mut uses = Vec::new();
    let mut bunches = Vec::new();
    for r in recs.iter().filter(|r| !r.from_client) {
        let Ok(p) = Packet::decode(&r.bytes) else { continue };
        for it in p.items {
            if let Item::Bunch(b) = it {
                if b.ch_index == 0 {
                    if b.reliable {
                        for m in control_messages(&b).unwrap_or_default() {
                            if let Some(u) = UsesLine::parse(&m) {
                                uses.push(u);
                            }
                        }
                    }
                } else {
                    bunches.push((r.ms, b));
                }
            }
        }
    }
    let mut s = GameSchema::load(&kf, uses);
    println!("objects={} names={}", s.map.max_object_index, s.names.len());
    let mut chans: HashMap<u32, Arc<ClassNet>> = HashMap::new();
    let mut rot_choice: HashMap<String, bool> = HashMap::new();
    let (mut ok, mut fail, mut skipped) = (0, 0, 0);
    let mut fail_by: HashMap<String, usize> = HashMap::new();
    let mut prop_count: HashMap<String, usize> = HashMap::new();
    for (ms, b) in &bunches {
        let mut r = b.reader();
        if b.open && b.ch_type == Some(CHTYPE_ACTOR) {
            let o = read_actor_open(&mut r, s.map.max_object_index).unwrap();
            let Some(((cs, ce), is_class)) = s.class_of_object(o.object_index) else { skipped += 1; continue };
            let Some(c) = s.class_net(cs, ce) else { skipped += 1; continue };
            if is_class {
                let _ = read_compressed_vector(&mut r);
                // bNetInitialRotation (class default): learned per class by trying both.
                let mut with_rot = r.clone();
                let _ = read_rotator(&mut with_rot);
                let use_rot = *rot_choice.entry(c.name.clone()).or_insert_with(|| {
                    let a = read_updates(&mut r.clone(), &c, &s).is_ok();
                    let b = read_updates(&mut with_rot.clone(), &c, &s).is_ok();
                    b && !a || (rot && b)
                });
                if use_rot {
                    r = with_rot;
                }
            }
            if c.fields.is_empty() {
                println!("EMPTY class {} obj={} ch{}", c.name, s.object_name(o.object_index), b.ch_index);
            }
            if std::env::var("DUMPCLASS").ok().as_deref() == Some(c.name.as_str()) {
                for (i, f) in c.fields.iter().enumerate() {
                    println!("  [{i}] {}.{} dim={} {:?} {:?}", f.owner, f.name, f.array_dim, f.ty, f.params.iter().map(|p| &p.0).collect::<Vec<_>>());
                }
                let rest = r.clone().bits(r.remaining()).unwrap();
                println!("  rest {} bits: {}", r.remaining(), (0..r.remaining()).map(|i| if rest[i>>3]>>(i&7)&1==1 {'1'} else {'0'}).collect::<String>());
            }
            chans.insert(b.ch_index, c);
        } else if b.ch_type.is_some() && b.ch_type != Some(CHTYPE_ACTOR) {
            continue;
        }
        if b.close && b.data_bits == 0 {
            chans.remove(&b.ch_index);
            continue;
        }
        let Some(c) = chans.get(&b.ch_index).cloned() else { skipped += 1; continue };
        match read_updates(&mut r, &c, &s) {
            Ok(u) => {
                ok += 1;
                for x in &u {
                    let n = match x { Update::Property { name, .. } => name, Update::Rpc { name, .. } => name };
                    *prop_count.entry(format!("{}.{}", c.name, n)).or_default() += 1;
                }
                if verbose == Some(b.ch_index) || verbose == Some(0) {
                    println!("{ms:>7} ch{} {} {:?}", b.ch_index, c.name, u);
                }
            }
            Err((u, e)) => {
                fail += 1;
                if std::env::var("DUMPFAIL").ok().and_then(|v| v.parse::<u64>().ok()) == Some(*ms) {
                    let mut rr = b.reader();
                    let bits: String = (0..b.data_bits).map(|_| if rr.bit().unwrap() { '1' } else { '0' }).collect();
                    println!("BITS {} {bits}", b.data_bits);
                    for (i, f) in c.fields.iter().enumerate() {
                        if f.ty.is_none() && (f.name.contains("Localized") || f.name.contains("ResetMovement") || f.name.contains("TeamMessage")) {
                            println!("  idx {i} {} params {:?}", f.name, f.params.iter().map(|p| (&p.0, format!("{:?}", p.2))).collect::<Vec<_>>());
                        }
                    }
                    println!("  max {}", c.fields.len());
                    for i in [100usize, 178] {
                        let f = &c.fields[i];
                        println!("  field {i} = {}.{} {:?} params {:?}", f.owner, f.name, f.ty, f.params.iter().map(|p| &p.0).collect::<Vec<_>>());
                    }
                }
                *fail_by.entry(e.split(" @").next().unwrap_or("").to_string()).or_default() += 1;
                if verbose.is_some() && (verbose == Some(b.ch_index) || verbose == Some(0)) {
                    println!("{ms:>7} ch{} {} FAIL {e} after {:?}", b.ch_index, c.name, u);
                }
            }
        }
        if b.close {
            chans.remove(&b.ch_index);
        }
    }
    println!("bunches ok={ok} fail={fail} skipped={skipped}");
    let mut f: Vec<_> = fail_by.into_iter().collect();
    f.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, v) in f.iter().take(25) {
        println!("  FAIL {v:>5} {k}");
    }
    let mut p: Vec<_> = prop_count.into_iter().collect();
    p.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, v) in p.iter().take(25) {
        println!("  PROP {v:>5} {k}");
    }
}
