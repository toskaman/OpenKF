//! Live spectator test: connects, joins and prints actor activity.
//! `cargo run --release -p openkf-network --example live -- <host:port> <KF dir> <capture with official AUTH> [seconds]`
use openkf_network::live::*;
use openkf_network::session::SessionEvent;
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let server = a[0].parse().expect("host:port");
    let kf = std::path::PathBuf::from(&a[1]);
    let auth = AuthConfig::from_capture(&std::fs::read_to_string(&a[2]).unwrap()).expect("no AUTH in capture");
    let secs: u64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
    println!("steamid {} blob chunks {}", auth.steam_id, auth.steam_blob.len());
    std::fs::create_dir_all("output").ok();
    let mut logf = std::fs::File::create("output/live_log.txt").unwrap();
    macro_rules! out { ($($t:tt)*) => {{ let s = format!($($t)*); println!("{s}"); use std::io::Write; let _ = writeln!(logf, "{s}"); }} }
    let mut c = LiveClient::connect(server, &kf, auth, "OpenKF").unwrap();
    let t0 = Instant::now();
    let mut shown = 0;
    let mut last_report = Instant::now();
    let (mut opened, mut updated, mut failed) = (0, 0, 0);
    while t0.elapsed() < Duration::from_secs(secs) {
        for e in c.poll() {
            match e {
                SessionEvent::Opened(_) => opened += 1,
                SessionEvent::Updated(..) => updated += 1,
                SessionEvent::Failed(ch, err) => {
                    failed += 1;
                    if failed < 5 {
                        out!("decode fail ch{ch}: {err}");
                    }
                }
                _ => {}
            }
        }
        while shown < c.log.len() {
            out!("[{:6.2}] {}", t0.elapsed().as_secs_f32(), c.log[shown]);
            shown += 1;
        }
        if last_report.elapsed() > Duration::from_secs(5) {
            last_report = Instant::now();
            let actors = c.session.as_ref().map(|s| s.actors.len()).unwrap_or(0);
            out!("[{:6.2}] state {:?} packets_in {} actors {actors} opened {opened} updates {updated} fails {failed}", t0.elapsed().as_secs_f32(), c.state, c.packets_in);
            if let Some(s) = &c.session {
                for st in s.actors.values().filter(|a| a.class.is_a("Pawn")).take(8) {
                    out!("    {} ch{} at {:?}", st.class.name, st.channel, st.location.map(|x| x as i32));
                }
            }
        }
        if c.state == LiveState::Failed {
            out!("connection failed");
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
