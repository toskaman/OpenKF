//! Decodes client->server RPCs on the PlayerController channel of a capture.
use openkf_network::package_map::UsesLine;
use openkf_network::props::*;
use openkf_network::schema::*;
use openkf_network::wire::*;
use std::collections::HashMap;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let recs = parse_capture(&std::fs::read_to_string(&a[0]).unwrap());
    let mut uses = Vec::new();
    for r in recs.iter().filter(|r| !r.from_client) {
        for it in Packet::decode(&r.bytes).unwrap().items {
            if let Item::Bunch(b) = it { if b.ch_index == 0 && b.reliable { uses.extend(control_messages(&b).unwrap().iter().filter_map(|m| UsesLine::parse(m))); } }
        }
    }
    let mut s = GameSchema::load(std::path::Path::new(&a[1]), uses);
    // class of each channel from server opens
    let mut cls: HashMap<u32, std::sync::Arc<ClassNet>> = HashMap::new();
    for r in recs.iter().filter(|r| !r.from_client) {
        for it in Packet::decode(&r.bytes).unwrap().items {
            if let Item::Bunch(b) = it { if b.open && b.ch_type == Some(CHTYPE_ACTOR) {
                let mut rd = b.reader();
                let o = openkf_network::actor_open::read_actor_open(&mut rd, s.map.max_object_index).unwrap();
                if let Some(((cs, ce), _)) = s.class_of_object(o.object_index) { if let Some(c) = s.class_net(cs, ce) { cls.entry(b.ch_index).or_insert(c); } }
            } }
        }
    }
    if let Some(c) = cls.get(&1) { for f in c.fields.iter().filter(|f| f.ty.is_none() && f.name.contains(&std::env::var("SIGNAME").unwrap_or("ServerMove".into()))) { println!("SIG {} {:?}", f.name, f.params.iter().map(|p| (&p.0, &p.2)).collect::<Vec<_>>()); } }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in recs.iter().filter(|r| r.from_client) {
        for it in Packet::decode(&r.bytes).unwrap().items {
            let Item::Bunch(b) = it else { continue };
            if b.ch_index == 0 { continue; }
            let Some(c) = cls.get(&b.ch_index) else { continue };
            match read_updates(&mut b.reader(), c, &s) {
                Ok(u) => for x in u { if let Update::Rpc { name, params } = &x {
                    let n = counts.entry(name.clone()).or_default(); *n += 1;
                    if std::env::var("ONLY").map_or(*n <= 2 || !name.starts_with("Server") || name.contains("Ready"), |o| o.split(',').any(|x| name == x)) { println!("{:>7} ch{} {}{} {name} {params:?}", r.ms, b.ch_index, c.name, if b.reliable {" R"} else {""}); }
                } },
                Err((_, e)) => println!("{:>7} ch{} FAIL {e}", r.ms, b.ch_index),
            }
        }
    }
    let mut v: Vec<_> = counts.into_iter().collect(); v.sort_by(|a,b| b.1.cmp(&a.1)); println!("{v:?}");
}
