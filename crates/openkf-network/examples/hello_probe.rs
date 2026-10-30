//! Probe: sends one UE2 HELLO to `ip:port`, prints CHALLENGE/FAILURE or timeout.
//! No LOGIN, no downloads — safe against public servers.
use openkf_network::wire::*;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn main() {
    let addr = std::env::args().nth(1).expect("ip:port");
    let sock = UdpSocket::bind("0.0.0.0:0").unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    let mut w = BitsOut::new();
    w.fstring("HELLO REVISION=0 MINVER=3180 VER=3339 STEAMID=0");
    let (data, bits) = w.into_parts();
    let b = Bunch { control: true, open: true, close: false, reliable: true, ch_index: 0, ch_sequence: Some(1), ch_type: Some(CHTYPE_CONTROL), data_bits: bits, data };
    let bytes = (Packet { id: 0, items: vec![Item::Bunch(b)] }).encode().unwrap();
    let t0 = Instant::now();
    for _ in 0..3 {
        sock.send_to(&bytes, &addr).unwrap();
        let mut buf = [0u8; 2048];
        loop {
            let n = match sock.recv(&mut buf) {
                Ok(n) => n,
                Err(_) => break,
            };
            let Ok(p) = Packet::decode(&buf[..n]) else { continue };
            for it in p.items {
                let Item::Bunch(b) = it else { continue };
                if b.ch_index != 0 || !b.reliable {
                    continue;
                }
                for m in control_messages(&b).unwrap_or_default() {
                    let head: String = m.chars().take(100).collect();
                    println!("REPLY {}ms: {head}", t0.elapsed().as_millis());
                    if head.starts_with("CHALLENGE ") || head.starts_with("FAILURE") || head.starts_with("UPGRADE") {
                        return;
                    }
                }
            }
            if t0.elapsed() > Duration::from_secs(10) {
                println!("TIMEOUT");
                return;
            }
        }
    }
    println!("TIMEOUT");
}
