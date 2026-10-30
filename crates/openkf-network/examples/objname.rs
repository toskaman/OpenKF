use openkf_network::package_map::UsesLine;
use openkf_network::schema::GameSchema;
use openkf_network::wire::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let recs = parse_capture(&std::fs::read_to_string(&a[0]).unwrap());
    let mut uses = Vec::new();
    for r in recs.iter().filter(|r| !r.from_client) { for it in Packet::decode(&r.bytes).unwrap().items { if let Item::Bunch(b) = it { if b.ch_index == 0 && b.reliable { uses.extend(control_messages(&b).unwrap().iter().filter_map(|m| UsesLine::parse(m))); } } } }
    let s = GameSchema::load(std::path::Path::new(&a[1]), uses);
    for x in &a[2..] { let i: u32 = x.parse().unwrap(); println!("{i} = {}", s.object_name(i)); }
}
