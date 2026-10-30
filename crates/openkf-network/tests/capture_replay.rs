//! Replays every datagram of the reference capture through `wire::Packet`
//! and checks decode + bit-exact re-encode. Skipped if the capture is absent.
use openkf_network::wire::*;

#[test]
fn reference_capture_roundtrip() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../captures/capture_1792954639.txt");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("capture absent, test skipped");
        return;
    };
    let recs = parse_capture(&text);
    assert!(recs.len() > 7000);
    let mut ctl = Vec::new();
    for r in &recs {
        let p = Packet::decode(&r.bytes).unwrap_or_else(|e| panic!("t={} {e}", r.ms));
        assert_eq!(p.encode().unwrap(), r.bytes, "re-encode mismatch t={}", r.ms);
        for it in &p.items {
            if let Item::Bunch(b) = it {
                if b.ch_index == 0 && b.reliable {
                    for m in control_messages(b).unwrap() {
                        ctl.push(m.split(' ').next().unwrap_or("").to_string());
                    }
                }
            }
        }
    }
    for want in ["HELLO", "CHALLENGE", "NETSPEED", "LOGIN", "WELCOME", "USES", "JOIN"] {
        assert!(ctl.iter().any(|c| c == want), "missing {want}");
    }
}

#[test]
fn first_actor_is_player_controller() {
    use openkf_network::actor_open::*;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../captures/capture_1792954639.txt");
    let Ok(text) = std::fs::read_to_string(path) else { return };
    // 336 USES packages in this session -> MaxObjectIndex 115642 (from local game files).
    // KFMod base = 26096, KFPlayerController = export 3.
    for r in parse_capture(&text).iter().filter(|r| !r.from_client) {
        let p = Packet::decode(&r.bytes).unwrap();
        for it in p.items {
            if let Item::Bunch(b) = it {
                if b.open && b.ch_type == Some(CHTYPE_ACTOR) {
                    let mut rd = b.reader();
                    let o = read_actor_open(&mut rd, 115_642).unwrap();
                    assert_eq!(o.object_index, 26_099);
                    let loc = read_compressed_vector(&mut rd).unwrap();
                    assert!(loc.iter().all(|c| c.abs() < 65_536), "{loc:?}");
                    return;
                }
            }
        }
    }
    panic!("no actor open");
}

/// Full decode of every actor bunch, using the local game install.
/// Set OPENKF_KF_DIR (e.g. /mnt/KillingFloor or E:\SteamLibrary\...\KillingFloor).
#[test]
fn actor_bunches_decode_rate() {
    use openkf_network::package_map::UsesLine;
    use openkf_network::schema::GameSchema;
    use openkf_network::session::ActorSession;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../captures/capture_1792954639.txt");
    let (Ok(text), Ok(kf)) = (std::fs::read_to_string(path), std::env::var("OPENKF_KF_DIR")) else {
        eprintln!("capture or OPENKF_KF_DIR absent, skipped");
        return;
    };
    let recs = parse_capture(&text);
    let mut uses = Vec::new();
    let mut bunches = Vec::new();
    for r in recs.iter().filter(|r| !r.from_client) {
        for it in Packet::decode(&r.bytes).unwrap().items {
            if let Item::Bunch(b) = it {
                if b.ch_index == 0 && b.reliable {
                    uses.extend(control_messages(&b).unwrap().iter().filter_map(|m| UsesLine::parse(m)));
                } else if b.ch_index != 0 {
                    bunches.push(b);
                }
            }
        }
    }
    let mut s = ActorSession::new(GameSchema::load(std::path::Path::new(&kf), uses));
    if std::env::var("OPENKF_UC_DEFAULTS").is_ok() {
        s.defaults = Some(std::sync::Arc::new(openkf_package::uc_defaults::ScriptDefaults::load(std::path::Path::new(&kf))));
    }
    for b in &bunches {
        s.handle(b);
    }
    let st = s.stats;
    let rate = st.bunches_ok as f64 / (st.bunches_ok + st.bunches_failed) as f64;
    eprintln!("{st:?} rate={rate:.4}");
    assert!(rate > 0.995, "{st:?}");
}

/// write_rpc must reproduce the official client's ServerSetReadyToStart bunch bit-for-bit.
#[test]
fn rpc_encoding_matches_official_client() {
    use openkf_network::package_map::UsesLine;
    use openkf_network::props::*;
    use openkf_network::schema::GameSchema;
    use openkf_network::session::ActorSession;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../captures/capture_1792954639.txt");
    let (Ok(text), Ok(kf)) = (std::fs::read_to_string(path), std::env::var("OPENKF_KF_DIR")) else { return };
    let recs = parse_capture(&text);
    let mut uses = Vec::new();
    for r in recs.iter().filter(|r| !r.from_client) {
        for it in Packet::decode(&r.bytes).unwrap().items {
            if let Item::Bunch(b) = it {
                if b.ch_index == 0 && b.reliable {
                    uses.extend(control_messages(&b).unwrap().iter().filter_map(|m| UsesLine::parse(m)));
                }
            }
        }
    }
    let mut s = ActorSession::new(GameSchema::load(std::path::Path::new(&kf), uses));
    'outer: for r in recs.iter().filter(|r| !r.from_client) {
        for it in Packet::decode(&r.bytes).unwrap().items {
            if let Item::Bunch(b) = it {
                s.handle(&b);
                if s.actors.contains_key(&1) {
                    break 'outer;
                }
            }
        }
    }
    let class = s.actors[&1].class.clone();
    // Official reliable bunch on ch1 at t=7848 (17 bits): ServerSetClassicTrans(false) + ServerSetReadyToStart.
    for r in recs.iter().filter(|r| r.from_client && r.ms == 7848) {
        for it in Packet::decode(&r.bytes).unwrap().items {
            let Item::Bunch(b) = it else { continue };
            if b.ch_index == 1 && b.reliable {
                let mut w = openkf_network::wire::BitsOut::new();
                write_rpc(&mut w, &class, &s.schema, "ServerSetClassicTrans", &[("B", Value::Bool(false))]).unwrap();
                write_rpc(&mut w, &class, &s.schema, "ServerSetReadyToStart", &[]).unwrap();
                let (d, n) = w.into_parts();
                assert_eq!(n, b.data_bits);
                assert_eq!(d, b.data);
                return;
            }
        }
    }
    panic!("official bunch not found");
}
