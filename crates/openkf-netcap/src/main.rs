//! openkf-netcap: transparent UDP relay that logs every datagram exchanged
//! between a Killing Floor client and server.
//!
//! Usage:
//!   openkf-netcap --listen 0.0.0.0:7707 --server 127.0.0.1:7717 [--out captures]
//!
//! Point the game at the relay ("open 127.0.0.1:7707" in the console). Each
//! datagram is appended to `<out>/capture_<unix time>.txt` as:
//!   <ms since start> <C>S or S>C> <length> <hex bytes>

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Args {
    listen: String,
    server: String,
    out: String,
}

fn parse_args() -> Args {
    let mut a = Args { listen: "0.0.0.0:7707".into(), server: "127.0.0.1:7717".into(), out: "captures".into() };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--listen" => a.listen = it.next().expect("--listen addr:port"),
            "--server" => a.server = it.next().expect("--server addr:port"),
            "--out" => a.out = it.next().expect("--out dir"),
            "-h" | "--help" => {
                println!("openkf-netcap --listen 0.0.0.0:7707 --server 127.0.0.1:7717 [--out captures]");
                std::process::exit(0);
            }
            other => panic!("unknown argument {other}"),
        }
    }
    a
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

fn main() -> std::io::Result<()> {
    let args = parse_args();
    let server: SocketAddr = args.server.parse().expect("--server must be ip:port");
    std::fs::create_dir_all(&args.out)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let path = format!("{}/capture_{stamp}.txt", args.out);
    let log = Arc::new(Mutex::new(std::io::BufWriter::new(std::fs::File::create(&path)?)));
    let start = Instant::now();

    let front = Arc::new(UdpSocket::bind(&args.listen)?);
    println!("[netcap] listening on {} -> server {} ; logging to {path}", args.listen, server);
    println!("[netcap] in Killing Floor console: open 127.0.0.1:{}", front.local_addr()?.port());

    // One upstream socket per client address (KF uses a single one per player).
    let upstreams: Arc<Mutex<HashMap<SocketAddr, Arc<UdpSocket>>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut buf = [0u8; 65536];
    let mut count = 0u64;
    loop {
        let (n, client) = front.recv_from(&mut buf)?;
        let up = {
            let mut map = upstreams.lock().unwrap();
            if let Some(u) = map.get(&client) {
                u.clone()
            } else {
                let u = Arc::new(UdpSocket::bind("0.0.0.0:0")?);
                u.connect(server)?;
                u.set_read_timeout(Some(Duration::from_secs(600)))?;
                map.insert(client, u.clone());
                println!("[netcap] new client {client} (upstream port {})", u.local_addr()?.port());
                // Server -> client pump.
                let (u2, front2, log2) = (u.clone(), front.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut b = [0u8; 65536];
                    while let Ok(m) = u2.recv(&mut b) {
                        let _ = front2.send_to(&b[..m], client);
                        let t = start.elapsed().as_millis();
                        let mut l = log2.lock().unwrap();
                        let _ = writeln!(l, "{t} S>C {m} {}", hex(&b[..m]));
                        let _ = l.flush();
                    }
                });
                u
            }
        };
        up.send(&buf[..n])?;
        let t = start.elapsed().as_millis();
        {
            let mut l = log.lock().unwrap();
            writeln!(l, "{t} C>S {n} {}", hex(&buf[..n]))?;
            l.flush()?;
        }
        count += 1;
        if count.is_multiple_of(500) {
            println!("[netcap] {count} client packets relayed");
        }
    }
}
