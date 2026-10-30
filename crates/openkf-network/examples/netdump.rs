//! Pretty-prints a netcap capture: `cargo run -p openkf-network --example netdump -- <capture.txt> [--ctl]`
use openkf_network::wire::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().expect("capture file");
    let only_ctl = args.iter().any(|a| a == "--ctl");
    let recs = parse_capture(&std::fs::read_to_string(path).unwrap());
    for r in recs {
        let dir = if r.from_client { "C>S" } else { "S>C" };
        let p = match Packet::decode(&r.bytes) {
            Ok(p) => p,
            Err(e) => {
                println!("{:>7} {dir} ERR {e}", r.ms);
                continue;
            }
        };
        for it in &p.items {
            match it {
                Item::Ack(a) if !only_ctl => println!("{:>7} {dir} #{} ack {a}", r.ms, p.id),
                Item::Bunch(b) if b.ch_index == 0 && b.reliable => {
                    for m in control_messages(b).unwrap_or_default() {
                        println!("{:>7} {dir} #{} CTL {m}", r.ms, p.id);
                    }
                }
                Item::Bunch(b) if !only_ctl => println!(
                    "{:>7} {dir} #{} ch{} {}{}{} seq={:?} type={:?} {} bits",
                    r.ms, p.id, b.ch_index,
                    if b.open { "O" } else { "-" }, if b.close { "C" } else { "-" },
                    if b.reliable { "R" } else { "-" }, b.ch_sequence, b.ch_type, b.data_bits
                ),
                _ => {}
            }
        }
    }
}
