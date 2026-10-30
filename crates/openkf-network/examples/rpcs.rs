//! Lists client->server RPC calls of a capture (to learn retail's ready/spawn flow).
//! `cargo run -p openkf-network --example rpcs -- <capture> <KF dir>`
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
    let mut uses = Vec::new();
    for r in recs.iter().filter(|r| !r.from_client) {
        let Ok(p) = Packet::decode(&r.bytes) else { continue };
        for it in p.items {
            let Item::Bunch(b) = it else { continue };
            if b.ch_index == 0 && b.reliable {
                for m in control_messages(&b).unwrap_or_default() {
                    if let Some(u) = UsesLine::parse(&m) {
                        uses.push(u);
                    }
                }
            }
        }
    }
    let mut s = GameSchema::load(&kf, uses);
    println!("objects={} names={}", s.map.max_object_index, s.names.len());
    // Re-resolve opens with the real object index bound.
    let mut chans: HashMap<u32, Arc<ClassNet>> = HashMap::new();
    for r in recs.iter().filter(|r| !r.from_client) {
        let Ok(p) = Packet::decode(&r.bytes) else { continue };
        for it in p.items {
            let Item::Bunch(b) = it else { continue };
            if b.ch_index != 0 && b.open && b.ch_type == Some(CHTYPE_ACTOR) && !chans.contains_key(&b.ch_index) {
                let mut rd = b.reader();
                let Ok(o) = read_actor_open(&mut rd, s.map.max_object_index) else { continue };
                let Some(((cs, ce), _)) = s.class_of_object(o.object_index) else { continue };
                if let Some(c) = s.class_net(cs, ce) {
                    if b.ch_index <= 8 {
                        println!("ch{} = {}", b.ch_index, c.name);
                    }
                    chans.insert(b.ch_index, c);
                }
            }
        }
    }
    for r in recs.iter().filter(|r| r.from_client) {
        let Ok(p) = Packet::decode(&r.bytes) else { continue };
        for it in p.items {
            let Item::Bunch(b) = it else { continue };
            if b.ch_index == 0 || b.data_bits == 0 {
                continue;
            }
            let Some(c) = chans.get(&b.ch_index).cloned() else { continue };
            let mut rd = b.reader();
            if let Ok(u) = read_updates(&mut rd, &c, &s) {
                for x in &u {
                    if let Update::Rpc { name, params } = x {
                        let ps: Vec<String> = params.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                        println!("{:>7} ch{} {}.{}({})", r.ms, b.ch_index, c.name, name, ps.join(", "));
                    }
                }
            }
        }
    }
}
