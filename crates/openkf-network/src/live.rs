//! Live UDP client: handshake + reliable control channel + actor replication.
//!
//! Spectator-level: it joins, acknowledges everything and feeds actor bunches
//! to [`ActorSession`]; it does not send movement yet.

use crate::handshake::*;
use crate::package_map::UsesLine;
use crate::schema::GameSchema;
use crate::session::{ActorSession, SessionEvent};
use crate::wire::*;
use md5::{Digest, Md5};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Identity data the server checks. Taken from a capture of the official
/// client for now (`from_capture`); a Steam-backed provider comes later.
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub steam_id: String,
    pub cd_key_hash: String,
    pub gmd5: [u8; 16],
    /// Full `STEAMCLIENTBLOB ...` messages, sent verbatim (capture mode).
    pub steam_blob: Vec<String>,
    /// Use the local Steam client to build a fresh blob for each server.
    pub live_steam: bool,
}

/// Default `AUTH HASH` / `GMD5` of a Steam v1065 install (KF does not validate them).
pub const DEFAULT_CD_KEY_HASH: &str = "1990084c866612acb11a64c4730001df";
pub const DEFAULT_GMD5: [u8; 16] = [0x00, 0x41, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

impl AuthConfig {
    /// Identity from the running Steam client (no capture needed).
    pub fn from_steam(steam: &crate::steam::SteamSession) -> Self {
        AuthConfig {
            steam_id: steam.steam_id().to_string(),
            cd_key_hash: DEFAULT_CD_KEY_HASH.into(),
            gmd5: DEFAULT_GMD5,
            steam_blob: Vec::new(),
            live_steam: true,
        }
    }
}

impl AuthConfig {
    pub fn from_capture(text: &str) -> Option<Self> {
        let mut a = AuthConfig::default();
        for r in parse_capture(text).iter().filter(|r| r.from_client) {
            let Ok(p) = Packet::decode(&r.bytes) else { continue };
            for it in p.items {
                let Item::Bunch(b) = it else { continue };
                if b.ch_index != 0 || !b.reliable {
                    continue;
                }
                for m in control_messages(&b).unwrap_or_default() {
                    if m.starts_with("HELLO ") {
                        a.steam_id = param(&m, "STEAMID")?.to_string();
                    } else if m.starts_with("AUTH ") {
                        a.cd_key_hash = param(&m, "HASH")?.to_string();
                        a.gmd5 = decode_gm(param(&m, "GM")?)?;
                    } else if m.starts_with("STEAMCLIENTBLOB ") && !a.steam_blob.contains(&m) {
                        a.steam_blob.push(m);
                    }
                }
            }
        }
        (!a.steam_id.is_empty() && !a.cd_key_hash.is_empty()).then_some(a)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveState {
    Hello,
    Challenged,
    LoggedIn,
    Joined,
    /// Server asked us to travel (map change) or went silent: reconnect.
    Traveling,
    Failed,
}

const MAX_IN_BUF_PER_CHANNEL: usize = 64;
const MAX_IN_BUF_GLOBAL: usize = 512;
const MAX_PENDING_PER_CHANNEL: usize = 16;
const MAX_RETRANSMIT_MS: u64 = 4000;

struct OutReliable {
    bunch: Bunch,
    packet: u32,
    sent: Instant,
    retries: u32,
}

pub struct LiveClient {
    sock: UdpSocket,
    server: SocketAddr,
    kf_dir: PathBuf,
    url: String,
    auth: AuthConfig,
    pub state: LiveState,
    out_id: u32,
    acks: Vec<u32>,
    out_seq: HashMap<u32, u32>,
    reliable: Vec<OutReliable>,
    queued: Vec<Bunch>,
    in_seq: HashMap<u32, u32>,
    in_buf: HashMap<(u32, u32), Bunch>,
    uses: Vec<UsesLine>,
    gz: u32,
    pub session: Option<ActorSession>,
    pub log: Vec<String>,
    pub map: Option<String>,
    last_send: Instant,
    rng: u32,
    pub packets_in: u64,
    pub download: Option<crate::download::DownloadManager>,
    /// Our pawn location as last confirmed by the server (ClientAdjustPosition).
    pub my_loc: Option<[f32; 3]>,
    pub my_vel: [f32; 3],
    move_t0: Instant,
    pub moves_sent: u64,
    move_actions: u8,
    /// Local Steam session (live auth mode).
    pub steam: Option<crate::steam::SteamSession>,
    last_recv: Instant,
    /// URL from the server's ClientTravel, if any.
    pub travel_url: Option<String>,
    /// Class defaults from the .uc sources (given to the ActorSession).
    pub defaults: Option<std::sync::Arc<openkf_package::uc_defaults::ScriptDefaults>>,
    /// `Engine.ChannelDownload` offered by the server (`DLMGR`).
    channel_dl_offered: bool,
    /// Packages still missing after FastDL, queued for in-connection download.
    file_queue: Vec<UsesLine>,
    /// Active in-connection (`CHTYPE_FILE`) download, if any.
    file_active: Option<FileActive>,
    /// Next client-opened file channel index (high range, away from actor channels).
    file_next_ch: u32,
    /// `WELCOME` message held while channel downloads run (map announced after).
    welcome_pending: Option<String>,
    /// Background FastDL (HTTP) progress, shared with the download thread.
    http_state: Option<std::sync::Arc<std::sync::Mutex<HttpProgress>>>,
    /// Background FastDL thread handle (joined from `poll` when finished).
    http_handle: Option<std::thread::JoinHandle<Vec<HttpResult>>>,
}

/// Live progress of the background FastDL thread (for the HUD progress bar).
#[derive(Debug, Default, Clone)]
pub struct HttpProgress {
    /// Currently downloading: (file, received bytes, total or None).
    pub current: Option<(String, u64, Option<u64>)>,
    /// Files still queued behind the current one.
    pub queued: usize,
    /// Thread finished (results ready to join).
    pub done: bool,
}

/// One background FastDL outcome: (file, guid, storage result).
pub type HttpResult = (String, String, Result<PathBuf, String>);

/// One in-connection file download (`UFileChannel`): the client opens a
/// `CHTYPE_FILE` channel whose payload is the 16-byte package GUID; the
/// server streams the raw file bytes across reliable bunches until close.
#[derive(Debug)]
struct FileActive {
    ch: u32,
    fname: String,
    guid: String,
    size: u64,
    data: Vec<u8>,
    started: Instant,
    /// Last time new bytes arrived (stall detection).
    last_data: Instant,
}

fn rbits(data: &[u8]) -> Bunch {
    Bunch { control: false, open: false, close: false, reliable: true, ch_index: 0, ch_sequence: None, ch_type: None, data_bits: data.len() * 8, data: data.to_vec() }
}

impl LiveClient {
    pub fn connect(server: SocketAddr, kf_dir: &Path, auth: AuthConfig, player_name: &str) -> std::io::Result<Self> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_nonblocking(true)?;
        let url = format!("Index.rom?Class=Engine.Pawn?Character=Corporal_Lewis?team=1?Sex=M?Name={player_name}?VAC=1");
        let mut c = Self {
            sock,
            server,
            kf_dir: kf_dir.to_path_buf(),
            url,
            auth,
            state: LiveState::Hello,
            out_id: 0,
            acks: Vec::new(),
            out_seq: HashMap::new(),
            reliable: Vec::new(),
            queued: Vec::new(),
            in_seq: HashMap::new(),
            in_buf: HashMap::new(),
            uses: Vec::new(),
            gz: 0,
            session: None,
            log: Vec::new(),
            map: None,
            last_send: Instant::now() - Duration::from_secs(1),
            rng: 0x1234_5678 ^ std::process::id(),
            packets_in: 0,
            my_loc: None,
            my_vel: [0.0; 3],
            move_t0: Instant::now(),
            moves_sent: 0,
            move_actions: 0,
            steam: None,
            last_recv: Instant::now(),
            travel_url: None,
            defaults: None,
            download: None,
            channel_dl_offered: false,
            file_queue: Vec::new(),
            file_active: None,
            file_next_ch: 1000,
            welcome_pending: None,
            http_state: None,
            http_handle: None,
        };
        let hello = format!("HELLO REVISION=0 MINVER=3180 VER=3339 STEAMID={}", c.auth.steam_id);
        c.send_control(&hello, true);
        c.flush();
        Ok(c)
    }

    /// Connects using the local Steam client for identity + auth blob (KF app id 1250).
    pub fn connect_steam(server: SocketAddr, kf_dir: &Path, player_name: &str) -> Result<Self, String> {
        let steam = crate::steam::SteamSession::open(1250)?;
        let auth = AuthConfig::from_steam(&steam);
        let mut c = Self::connect(server, kf_dir, auth, player_name).map_err(|e| e.to_string())?;
        c.log.push(format!("steam: account {} (SteamID {})", steam.account, steam.steam_id()));
        c.steam = Some(steam);
        Ok(c)
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn send_control(&mut self, msg: &str, open: bool) {
        self.log.push(format!("C>S {}", msg.chars().take(120).collect::<String>()));
        let mut w = BitsOut::new();
        w.fstring(msg);
        let (data, bits) = w.into_parts();
        let seq = self.out_seq.entry(0).or_insert(0);
        *seq = (*seq + 1) % MAX_CH_SEQUENCE;
        let mut b = rbits(&data);
        b.data_bits = bits;
        b.ch_sequence = Some(*seq);
        b.ch_type = Some(CHTYPE_CONTROL);
        if open {
            b.control = true;
            b.open = true;
        }
        self.queued.push(b);
    }

    /// Sends queued bunches + pending acks (several packets if needed).
    /// Ultra-stable: exponential backoff (1s → 2s → 4s, capped 4s) + ±10% jitter, control priority, flood cap.
    pub fn flush(&mut self) {
        let now = Instant::now();
        let mut resend = Vec::new();
        for r in self.reliable.iter_mut() {
            let backoff = (1000u64 << r.retries.min(2)).min(MAX_RETRANSMIT_MS);
            let jitter = (self.rng % 200) as u64; // 0..200ms, cheap jitter
            self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            if now - r.sent > Duration::from_millis(backoff + jitter / 10) {
                resend.push(r.bunch.clone());
                r.sent = now;
                r.retries += 1;
                r.packet = u32::MAX;
            }
        }
        self.reliable.retain(|r| r.packet != u32::MAX);
        let mut todo: Vec<Bunch> = resend;
        todo.append(&mut self.queued);
        let keepalive = now - self.last_send > Duration::from_millis(100);
        if todo.is_empty() && self.acks.is_empty() && !keepalive {
            return;
        }
        loop {
            let mut items: Vec<Item> = self.acks.drain(..).map(Item::Ack).collect();
            let mut bits = 14 + items.len() * 15;
            let mut sent_rel = Vec::new();
            while let Some(b) = todo.first() {
                let need = b.data_bits + 50;
                if bits + need > 480 * 8 && (items.iter().any(|i| matches!(i, Item::Bunch(_)))) {
                    break;
                }
                bits += need;
                let b = todo.remove(0);
                if b.reliable {
                    sent_rel.push(b.clone());
                }
                items.push(Item::Bunch(b));
            }
            let id = self.out_id;
            self.out_id = (self.out_id + 1) % MAX_PACKET_ID;
            if let Ok(bytes) = (Packet { id, items }).encode() {
                let _ = self.sock.send_to(&bytes, self.server);
            }
            for b in sent_rel {
                self.reliable.push(OutReliable { bunch: b, packet: id, sent: now, retries: 0 });
            }
            self.last_send = now;
            if todo.is_empty() {
                break;
            }
        }
    }

    /// Receives and processes everything available; returns actor events.
    pub fn poll(&mut self) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok((n, from)) = self.sock.recv_from(&mut buf) {
            if from != self.server {
                continue;
            }
            self.packets_in += 1;
            self.last_recv = Instant::now();
            let Ok(p) = Packet::decode(&buf[..n]) else { continue };
            self.acks.push(p.id);
            for it in p.items {
                match it {
                    Item::Ack(a) => self.reliable.retain(|r| r.packet != a),
                    Item::Bunch(b) => self.receive_bunch(b, &mut events),
                }
            }
        }
        // Own movement corrections, map travel.
        for e in &events {
            if let SessionEvent::Updated(_, ups) = e {
                for u in ups {
                    let crate::props::Update::Rpc { name, params } = u else { continue };
                    if name == "ClientTravel" {
                        let url = params.iter().find_map(|p| if let crate::props::Value::Str(s) = &p.1 { Some(s.clone()) } else { None });
                        self.log.push(format!("S>C ClientTravel {url:?}"));
                        self.travel_url = url;
                        self.state = LiveState::Traveling;
                    }
                    if name != "ClientAdjustPosition" && name != "VeryShortClientAdjustPosition" && name != "ShortClientAdjustPosition" {
                        continue;
                    }
                    let f = |k: &str| params.iter().find(|p| p.0 == k).and_then(|p| if let crate::props::Value::Float(v) = p.1 { Some(v) } else { None });
                    if let (Some(x), Some(y), Some(z)) = (f("NewLocX"), f("NewLocY"), f("NewLocZ")) {
                        self.my_loc = Some([x, y, z]);
                        self.my_vel = [f("NewVelX").unwrap_or(0.0), f("NewVelY").unwrap_or(0.0), f("NewVelZ").unwrap_or(0.0)];
                    }
                }
            }
        }
        self.check_file_timeout();
        self.poll_http_downloads();
        self.flush();
        events
    }

    /// Our weapons (channel, class name): Weapon actors are only relevant to their owner.
    pub fn weapons(&self) -> Vec<(u32, String)> {
        let Some(s) = self.session.as_ref() else { return Vec::new() };
        let mut v: Vec<(u32, String)> = s.actors.values().filter(|a| a.class.is_a("Weapon")).map(|a| (a.channel, a.class.name.clone())).collect();
        v.sort();
        v
    }

    /// Our pawn channel (lowest KFHumanPawn channel).
    pub fn pawn_channel(&self) -> Option<u32> {
        self.session.as_ref()?.actors.values().filter(|a| a.class.is_a("KFHumanPawn")).map(|a| a.channel).min()
    }

    /// `Pawn.ServerChangedWeapon(OldWeapon, NewWeapon)` as sent by the official client.
    pub fn change_weapon(&mut self, old: Option<u32>, new: u32) -> Result<(), String> {
        use crate::props::Value;
        let pawn = self.pawn_channel().ok_or("no pawn")?;
        let mut args = vec![("newWeapon", Value::ActorChannel(new))];
        if let Some(o) = old {
            args.push(("OldWeapon", Value::ActorChannel(o)));
        }
        self.call_rpc(pawn, "ServerChangedWeapon", &args)
    }

    /// `Weapon.ServerStartFire(Mode)` / `ServerStopFire(Mode)` on the weapon's channel.
    pub fn fire(&mut self, weapon: u32, start: bool, mode: u8) -> Result<(), String> {
        use crate::props::Value;
        let args: Vec<(&str, Value)> = if mode != 0 { vec![("Mode", Value::Byte(mode as u32))] } else { vec![] };
        self.call_rpc(weapon, if start { "ServerStartFire" } else { "ServerStopFire" }, &args)
    }

    /// `Pawn.ServerBuyWeapon(WClass, ItemWeight)` — trader must be open and the pawn in a shop.
    pub fn buy_weapon(&mut self, class: &str, weight: f32) -> Result<(), String> {
        use crate::props::Value;
        let pawn = self.pawn_channel().ok_or("no pawn")?;
        let idx = self.session.as_ref().and_then(|s| s.schema.object_index(class)).ok_or(format!("unknown class {class}"))?;
        self.call_rpc(pawn, "ServerBuyWeapon", &[("WClass", Value::Object(idx)), ("ItemWeight", Value::Float(weight))])
    }

    /// `Pawn.ServerBuyAmmo(aClass, bOnlyClip)`.
    pub fn buy_ammo(&mut self, ammo_class: &str, only_clip: bool) -> Result<(), String> {
        use crate::props::Value;
        let pawn = self.pawn_channel().ok_or("no pawn")?;
        let idx = self.session.as_ref().and_then(|s| s.schema.object_index(ammo_class)).ok_or(format!("unknown class {ammo_class}"))?;
        self.call_rpc(pawn, "ServerBuyAmmo", &[("aClass", Value::Object(idx)), ("bOnlyClip", Value::Bool(only_clip))])
    }

    /// Weapon channel by class name (e.g. "Single", "knife", "Frag").
    pub fn weapon_by_class(&self, class: &str) -> Option<u32> {
        self.weapons().into_iter().find(|w| w.1.eq_ignore_ascii_case(class)).map(|w| w.0)
    }

    /// Unreliable `ServerMove` (PlayerController.ReplicateMove format, 1 move).
    /// `accel`: desired acceleration in UU/s² (sent ×10 like the game); angles in UE units.
    pub fn send_move(&mut self, accel: [f32; 3], pitch: i32, yaw: i32) -> Result<(), String> {
        let a = self.move_actions;
        self.send_move_actions(accel, pitch, yaw, a)
    }

    /// Toggles bJumpStatus: the server makes the pawn jump when it changes (PlayerController.ServerMove).
    pub fn jump(&mut self) {
        self.move_actions ^= 4;
    }

    /// `ServerMove` with explicit `NewActions` (1 run, 2 duck, 4 jump status, 8 sprint, 16 crawl).
    pub fn send_move_actions(&mut self, accel: [f32; 3], pitch: i32, yaw: i32, actions: u8) -> Result<(), String> {
        use crate::props::Value;
        let ch = self.controller_channel().ok_or("no controller")?;
        if self.my_loc.is_none() {
            // Before the first correction: our pawn's spawn location.
            let pawn = self.session.as_ref().and_then(|s| s.actors.values().find(|a| a.class.is_a("KFHumanPawn")).map(|a| a.location));
            self.my_loc = pawn;
        }
        let loc = self.my_loc.ok_or("no pawn yet")?;
        let s = self.session.as_ref().ok_or("not joined")?;
        let a = s.actors.get(&ch).ok_or("unknown channel")?;
        let view = ((pitch / 2) & 32767) * 32768 + ((yaw / 2) & 32767);
        let acc = [(accel[0] * 10.0) as i32, (accel[1] * 10.0) as i32, (accel[2] * 10.0) as i32];
        let mut w = BitsOut::new();
        let ts = self.move_t0.elapsed().as_secs_f32();
        let args = [
            ("TimeStamp", Value::Float(ts)),
            ("InAccel", Value::Vector(acc)),
            ("ClientLoc", Value::Vector([loc[0] as i32, loc[1] as i32, loc[2] as i32])),
            ("View", Value::Int(view)),
            ("NewActions", Value::Byte(actions as u32)),
        ];
        let rpc = if acc == [0, 0, 0] { "ShortServerMove" } else { "ServerMove" };
        crate::props::write_rpc(&mut w, &a.class, &s.schema, rpc, &args)?;
        let (data, bits) = w.into_parts();
        self.queued.push(Bunch { control: false, open: false, close: false, reliable: false, ch_index: ch, ch_sequence: None, ch_type: None, data_bits: bits, data });
        self.moves_sent += 1;
        Ok(())
    }

    fn receive_bunch(&mut self, b: Bunch, events: &mut Vec<SessionEvent>) {
        if !b.reliable {
            self.dispatch(b, events);
            return;
        }
        let ch = b.ch_index;
        let seq = b.ch_sequence.unwrap_or(0);
        let last = *self.in_seq.get(&ch).unwrap_or(&0);
        let expected = (last + 1) % MAX_CH_SEQUENCE;
        let ahead = (seq + MAX_CH_SEQUENCE - expected) % MAX_CH_SEQUENCE;
        if ahead >= MAX_CH_SEQUENCE / 2 {
            return; // duplicate
        }
        // Cap per-channel and global out-of-order buffers (DoS hardening)
        if self.in_buf.len() >= MAX_IN_BUF_GLOBAL {
            // drop oldest entry to make room
            if let Some(k) = self.in_buf.keys().next().cloned() { self.in_buf.remove(&k); }
        }
        let per_ch = self.in_buf.keys().filter(|(c, _)| *c == ch).count();
        if per_ch >= MAX_IN_BUF_PER_CHANNEL {
            return;
        }
        self.in_buf.insert((ch, seq), b);
        let mut next = expected;
        while let Some(b) = self.in_buf.remove(&(ch, next)) {
            self.in_seq.insert(ch, next);
            // UE2 keeps InReliable[ChIndex] for the whole connection: a reused channel
            // index continues the sequence, so never reset it on close.
            self.dispatch(b, events);
            next = (next + 1) % MAX_CH_SEQUENCE;
        }
    }

    /// Starts background FastDL downloads for every `USES` package missing
    /// locally (like the retail `UPendingLevel` before joining). The network
    /// thread keeps polling (acks + keepalives) while the files stream in;
    /// [`poll`](Self::poll) joins the results and moves on to channel
    /// downloads / `JOIN`. Files go to `download_dir`, never into the game.
    fn spawn_http_downloads(&mut self, missing: Vec<(String, String)>) {
        let Some((base, compressed)) = self.download.as_ref().and_then(|d| d.redirect_url.clone().map(|u| (u, d.compressed))) else {
            return;
        };
        let dir = crate::download::download_dir(&self.kf_dir);
        let state = std::sync::Arc::new(std::sync::Mutex::new(HttpProgress { current: None, queued: missing.len(), done: false }));
        let st = state.clone();
        let handle = std::thread::Builder::new()
            .name("openkf-fastdl".into())
            .spawn(move || {
                let mut out = Vec::new();
                let total = missing.len();
                for (i, (fname, guid)) in missing.into_iter().enumerate() {
                    {
                        let mut s = st.lock().unwrap();
                        s.current = Some((fname.clone(), 0, None));
                        s.queued = total - i - 1;
                    }
                    let t = std::time::Instant::now();
                    let fname2 = fname.clone();
                    let st2 = st.clone();
                    let r = crate::download::fetch_package_with_progress(&base, compressed, &fname, Some(&guid), &dir, &mut move |rx, tot| {
                        if let Ok(mut s) = st2.lock() {
                            s.current = Some((fname2.clone(), rx, tot));
                        }
                    });
                    match &r {
                        Ok(p) => {
                            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                            println!("[Download] {fname} {} KB in {:.1} s", size / 1024, t.elapsed().as_secs_f32());
                        }
                        Err(e) => println!("[Download] {fname} failed: {e}"),
                    }
                    out.push((fname, guid, r));
                }
                if let Ok(mut s) = st.lock() {
                    s.current = None;
                    s.queued = 0;
                    s.done = true;
                }
                out
            });
        match handle {
            Ok(h) => {
                self.http_state = Some(state);
                self.http_handle = Some(h);
            }
            Err(e) => self.log.push(format!("download: cannot spawn FastDL thread: {e}")),
        }
    }

    /// Joins finished background FastDL downloads and continues the handshake
    /// (channel downloads for leftovers, then `JOIN`). Called from [`poll`](Self::poll).
    fn poll_http_downloads(&mut self) {
        let finished = self.http_handle.as_ref().is_some_and(|h| h.is_finished());
        if !finished {
            return;
        }
        let results = match self.http_handle.take() {
            Some(h) => h.join().unwrap_or_default(),
            None => Vec::new(),
        };
        self.http_state = None;
        for (fname, guid, r) in &results {
            match r {
                Ok(p) => self.log.push(format!("fastdl: {fname} -> {}", p.display())),
                Err(e) => self.log.push(format!("fastdl: {fname} ({guid}) failed: {e}")),
            }
        }
        // Leftovers go through the in-connection file channel, if offered.
        let missing: Vec<UsesLine> = self
            .uses
            .iter()
            .filter(|u| !u.fname.is_empty())
            .filter(|u| crate::download::find_package_by_guid(&self.kf_dir, &u.fname, &u.guid).is_none())
            .cloned()
            .collect();
        if !missing.is_empty() && self.channel_dl_offered {
            self.log.push(format!("channel-dl: {} package(s) queued for in-connection download", missing.len()));
            println!("[Download] {} package(s) via in-connection channel", missing.len());
            self.file_queue = missing.into_iter().rev().collect();
            self.start_next_file_download();
            self.flush();
            return;
        }
        if !missing.is_empty() {
            self.log.push(format!("download: {} package(s) unavailable: {:?}", missing.len(), missing.iter().map(|m| &m.fname).collect::<Vec<_>>()));
        }
        if let Some(w) = self.welcome_pending.take() {
            self.finish_welcome(&w);
            self.flush();
        }
    }

    fn dispatch(&mut self, b: Bunch, events: &mut Vec<SessionEvent>) {
        if b.ch_index == 0 {
            if b.reliable {
                for m in control_messages(&b).unwrap_or_default() {
                    self.on_control(&m);
                }
            }
            return;
        }
        // File/Voice isolation: File -> channel download, Voice -> drop.
        if let Some(t) = b.ch_type {
            if t == CHTYPE_FILE {
                self.on_file_bunch(&b);
                return;
            }
            if t == CHTYPE_VOICE {
                return;
            }
        }
        if let Some(s) = self.session.as_mut() {
            if let Some(e) = s.handle(&b) {
                events.push(e);
            }
        }
    }

    /// True when the server has been silent for `secs` (crash, map change, kick).
    pub fn silent_for(&self, secs: f32) -> bool {
        self.last_recv.elapsed().as_secs_f32() > secs
    }

    /// Built-in timeout state for ultra-stable supervision (5s warn, 10s stale, 30s dead).
    pub fn timeout_state(&self) -> &'static str {
        let s = self.last_recv.elapsed().as_secs_f32();
        if s > 30.0 { "dead" } else if s > 10.0 { "stale" } else if s > 5.0 { "warn" } else { "healthy" }
    }

    /// Channel of our own PlayerController (first actor channel the server opens).
    pub fn controller_channel(&self) -> Option<u32> {
        self.session.as_ref()?.actors.values().filter(|a| a.class.is_a("PlayerController")).map(|a| a.channel).min()
    }

    /// Sends a reliable RPC on an actor channel (flood-capped at 16 per channel).
    pub fn call_rpc(&mut self, ch: u32, name: &str, args: &[(&str, crate::props::Value)]) -> Result<(), String> {
        let pending = self.queued.iter().filter(|b| b.ch_index == ch).count() + self.reliable.iter().filter(|r| r.bunch.ch_index == ch).count();
        if pending >= MAX_PENDING_PER_CHANNEL {
            return Err("channel flood limit".into());
        }
        let s = self.session.as_ref().ok_or("not joined")?;
        let a = s.actors.get(&ch).ok_or("unknown channel")?;
        let mut w = BitsOut::new();
        crate::props::write_rpc(&mut w, &a.class, &s.schema, name, args)?;
        let (data, bits) = w.into_parts();
        let seq = self.out_seq.entry(ch).or_insert(0);
        *seq = (*seq + 1) % MAX_CH_SEQUENCE;
        self.queued.push(Bunch {
            control: false,
            open: false,
            close: false,
            reliable: true,
            ch_index: ch,
            ch_sequence: Some(*seq),
            ch_type: Some(CHTYPE_ACTOR),
            data_bits: bits,
            data,
        });
        if !name.ends_with("Fire") {
            self.log.push(format!("C>S RPC ch{ch} {name} {args:?}"));
        }
        Ok(())
    }

    fn pete_entries(&self) -> Vec<String> {
        PETE_PACKAGES
            .iter()
            .filter_map(|p| {
                let path = self.kf_dir.join("System").join(format!("{p}.u"));
                let data = std::fs::read(&path).ok()?;
                if data.len() < 52 {
                    return None;
                }
                // GUID from FPackageFileSummary (offset 36), printed as 4 u32.
                let g: Vec<String> = (0..4)
                    .map(|i| {
                        let s = 36 + 4 * i;
                        u32::from_le_bytes(data[s..s + 4].try_into().unwrap())
                    })
                    .map(|v| format!("{v:08X}"))
                    .collect();
                let md5: String = Md5::digest(&data).iter().map(|x| format!("{x:02x}")).collect();
                Some(format!("{}{}", g.concat(), md5))
            })
            .collect()
    }

    fn on_control(&mut self, m: &str) {
        self.log.push(format!("S>C {}", m.chars().take(160).collect::<String>()));
        let cmd = m.split(' ').next().unwrap_or("");
        match cmd {
            "STEAMENCRYPTIONKEY" => {
                if self.auth.live_steam {
                    let server_id: u64 = param(m, "STEAMID").and_then(|v| v.parse().ok()).unwrap_or(0);
                    let secure = param(m, "SECURE") == Some("1");
                    let ip = match self.server.ip() {
                        std::net::IpAddr::V4(v) => u32::from(v),
                        _ => 0,
                    };
                    let port = self.server.port();
                    // Close any game connection left open by a previous run (crash / kill),
                    // as the retail client does on disconnect, before starting a new one.
                    if let Some(s) = self.steam.as_ref() {
                        s.terminate(ip, port);
                    }
                    match self.steam.as_ref().map(|s| s.initiate(server_id, ip, port, secure)) {
                        Some(Ok((buf, n))) => {
                            self.log.push(format!("steam: InitiateGameConnection -> {n} bytes"));
                            self.auth.steam_blob = crate::steam::blob_messages(&buf, n);
                        }
                        Some(Err(e)) => self.log.push(format!("steam: {e}")),
                        None => self.log.push("steam: no session".into()),
                    }
                }
                for blob in self.auth.steam_blob.clone() {
                    self.send_control(&blob, false);
                }
                for c in 0..4 {
                    self.send_control(&format!("STEAMTICKET SIZE=1 CHUNK={c}"), false);
                }
            }
            "CHALLENGE" => {
                let ch: i32 = param(m, "CHALLENGE").and_then(|v| v.parse().ok()).unwrap_or(0);
                self.gz = param(m, "GZ").and_then(|v| v.parse().ok()).unwrap_or(0);
                let md = self.auth.gmd5;
                let mut st = self.rand();
                let gm = make_gm(&md, || {
                    st ^= st << 13;
                    st ^= st >> 17;
                    st ^= st << 5;
                    st as u8
                });
                // Retail v1065 appends empty USERNAME / PASSWORD; strict servers drop logins without them.
                let auth = format!("AUTH HASH={} GM={gm} USERNAME= PASSWORD=", self.auth.cd_key_hash);
                self.send_control(&auth, false);
                self.send_control("NETSPEED 25000", false);
                let login = format!("LOGIN RESPONSE={} URL={}", challenge_response(ch), self.url);
                self.send_control(&login, false);
                self.state = LiveState::Challenged;
            }
            "USES" => {
                if let Some(u) = UsesLine::parse(m) {
                    self.uses.push(u);
                }
            }
            "WELCOME" => {
                self.state = LiveState::LoggedIn;
                let missing: Vec<UsesLine> = self
                    .uses
                    .iter()
                    .filter(|u| !u.fname.is_empty())
                    .filter(|u| crate::download::find_package_by_guid(&self.kf_dir, &u.fname, &u.guid).is_none())
                    .cloned()
                    .collect();
                // FastDL first, in a background thread (the connection stays alive);
                // leftovers fall through to the in-connection file channel below.
                let has_http = self.download.as_ref().and_then(|d| d.redirect_url.clone()).is_some();
                if !missing.is_empty() && has_http {
                    self.log.push(format!("fastdl: {} package(s) in background", missing.len()));
                    println!("[Download] {} package(s) via FastDL (background)", missing.len());
                    self.welcome_pending = Some(m.to_string());
                    let jobs = missing.iter().map(|u| (u.fname.clone(), u.guid.clone())).collect();
                    self.spawn_http_downloads(jobs);
                    self.flush();
                    return;
                }
                // No HTTP redirect: packages still missing go through the
                // in-connection file channel (retail `Engine.ChannelDownload`).
                if !missing.is_empty() && self.channel_dl_offered {
                    self.log.push(format!("channel-dl: {} package(s) queued for in-connection download", missing.len()));
                    println!("[Download] {} package(s) via in-connection channel", missing.len());
                    self.file_queue = missing.into_iter().rev().collect();
                    self.welcome_pending = Some(m.to_string());
                    self.start_next_file_download();
                    self.flush();
                    return;
                }
                if !missing.is_empty() {
                    self.log.push(format!("download: {} package(s) unavailable (no HTTP redirect, no ChannelDownload)", missing.len()));
                }
                self.finish_welcome(m);
            }
            "DLMGR" => {
                // Download managers offered by the server, in preference order:
                // IpDrv.HTTPDownload (FastDL redirect) and/or Engine.ChannelDownload.
                let dm = self.download.get_or_insert_with(|| crate::download::DownloadManager::new(&self.kf_dir));
                let class = param(m, "CLASS").unwrap_or("");
                if class.eq_ignore_ascii_case("IpDrv.HTTPDownload") {
                    if let Some(url) = param(m, "PARAMS").filter(|u| u.starts_with("http://")) {
                        if dm.redirect_url.is_none() {
                            dm.redirect_url = Some(url.to_string());
                            dm.compressed = param(m, "COMPRESSION") == Some("1");
                            self.log.push(format!("dlmgr: HTTP redirect {url} (uz2 {})", dm.compressed));
                        }
                    }
                } else if class.eq_ignore_ascii_case("Engine.ChannelDownload") {
                    self.channel_dl_offered = true;
                    self.log.push("dlmgr: Engine.ChannelDownload offered".into());
                } else {
                    self.log.push(format!("dlmgr: {class} offered"));
                }
            }
            "NEEDED" => {
                // Server indicates a needed package: register expected file
                if let Some(fname) = param(m, "FNAME") {
                    let size: u64 = param(m, "SIZE").and_then(|v| v.parse().ok()).unwrap_or(0);
                    if let Some(dm) = self.download.as_mut() {
                        dm.expect(fname.to_string(), size);
                    }
                }
            }
            _ if m.starts_with("RedirectToURL") || m.contains("UPGRADE URL=") => {
                if let Some(url) = param(m, "URL").or_else(|| param(m, "RedirectToURL")) {
                    if let Some(dm) = self.download.as_mut() {
                        dm.set_redirect(url.to_string());
                    } else {
                        let mut dm = crate::download::DownloadManager::new(&self.kf_dir);
                        dm.set_redirect(url.to_string());
                        self.download = Some(dm);
                    }
                }
            }
            "FAILURE" | "FAILCODE" | "UPGRADE" => self.state = LiveState::Failed,
            _ => {}
        }
    }

    /// Completes the `WELCOME` handshake: announces the map, builds the
    /// replication schema and sends `PETE` + `JOIN`. Runs immediately unless
    /// channel downloads are pending, in which case it runs after the last one.
    fn finish_welcome(&mut self, m: &str) {
        self.map = param(m, "LEVEL").map(str::to_string);
        let schema = GameSchema::load(&self.kf_dir, std::mem::take(&mut self.uses));
        self.log.push(format!("schema: {} objects, {} names", schema.map.max_object_index, schema.names.len()));
        let mut session = ActorSession::new(schema);
        session.defaults = self.defaults.clone();
        self.session = Some(session);
        let entries = self.pete_entries();
        let mut st = self.rand();
        for msg in pete_messages(&entries, self.gz, || {
            st ^= st << 13;
            st ^= st >> 17;
            st ^= st << 5;
            50 - (st % 20) as usize
        }) {
            self.send_control(&msg, false);
        }
        self.send_control("JOIN", false);
        // Required for movement ack on strict servers (see network_protocol.md:164)
        self.send_control("OPENVOICE 1", false);
        self.send_control("SA N=0", false);
        self.state = LiveState::Joined;
    }

    /// Starts the next queued in-connection download, or finishes `WELCOME`
    /// when the queue is empty. One file at a time, like the retail client.
    fn start_next_file_download(&mut self) {
        if self.file_active.is_some() {
            return;
        }
        let Some(next) = self.file_queue.pop() else {
            if let Some(w) = self.welcome_pending.take() {
                self.log.push("channel-dl: queue complete".into());
                self.finish_welcome(&w);
                self.flush();
            }
            return;
        };
        let Some(guid) = crate::download::guid_to_bytes(&next.guid) else {
            self.log.push(format!("channel-dl: bad GUID for {}", next.fname));
            self.start_next_file_download();
            return;
        };
        let ch = self.file_next_ch;
        self.file_next_ch += 1;
        if self.file_next_ch >= MAX_CHANNELS {
            self.file_next_ch = 1000;
        }
        let seq = self.out_seq.entry(ch).or_insert(0);
        *seq = (*seq + 1) % MAX_CH_SEQUENCE;
        self.queued.push(Bunch {
            control: true,
            open: true,
            close: false,
            reliable: true,
            ch_index: ch,
            ch_sequence: Some(*seq),
            ch_type: Some(CHTYPE_FILE),
            data_bits: 128,
            data: guid.to_vec(),
        });
        self.log.push(format!("channel-dl: requesting {} ({}) on ch{ch}", next.fname, next.guid));
        println!("[Download] requesting {} via channel ch{ch}", next.fname);
        let now = Instant::now();
        self.file_active = Some(FileActive { ch, fname: next.fname, guid: next.guid, size: next.size, data: Vec::new(), started: now, last_data: now });
    }

    /// Accumulates one `CHTYPE_FILE` bunch into the active download.
    fn on_file_bunch(&mut self, b: &Bunch) {
        let Some(active) = self.file_active.as_mut() else { return };
        if b.ch_index != active.ch {
            return;
        }
        // File payloads are whole bytes; ignore any trailing partial byte.
        let n = (b.data_bits / 8).min(b.data.len());
        if active.data.len() + n > crate::download::MAX_DOWNLOAD {
            self.log.push(format!("channel-dl: {} exceeds size cap, aborting", active.fname));
            self.file_active = None;
            self.start_next_file_download();
            return;
        }
        active.data.extend_from_slice(&b.data[..n]);
        if n > 0 {
            active.last_data = Instant::now();
        }
        let done = b.close || (active.size > 0 && active.data.len() as u64 >= active.size);
        if b.close || active.data.len() % (256 * 1024) < n {
            println!("[Download] {} {} KB{}", active.fname, active.data.len() / 1024, if active.size > 0 { format!(" / {} KB", active.size / 1024) } else { String::new() });
        }
        if !done {
            return;
        }
        let active = self.file_active.take().unwrap();
        let dir = crate::download::download_dir(&self.kf_dir);
        let tmp = dir.clone().join(format!("{}.part", active.fname));
        let dst = dir.join(&active.fname);
        let mut bytes = active.data;
        // Same normalization as FastDL: tweaked package tags are stored fixed.
        if bytes.len() >= 4 {
            let tag = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            if tag != crate::download::PACKAGE_TAG && tag & 0xFFFF_FF00 == crate::download::PACKAGE_TAG & 0xFFFF_FF00 {
                bytes[0..4].copy_from_slice(&crate::download::PACKAGE_TAG.to_le_bytes());
            }
        }
        let saved = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&tmp, &bytes))
            .map_err(|e| e.to_string())
            .and_then(|_| {
                match crate::download::package_guid(&tmp) {
                    Some(g) if g.eq_ignore_ascii_case(&active.guid) => std::fs::rename(&tmp, &dst).map_err(|e| e.to_string()),
                    Some(g) => Err(format!("GUID mismatch: got {g}, want {}", active.guid)),
                    None => Err("not an Unreal package".into()),
                }
            });
        match saved {
            Ok(()) => {
                self.log.push(format!("channel-dl: {} saved ({} KB)", active.fname, bytes.len() / 1024));
                println!("[Download] {} saved ({} KB)", active.fname, bytes.len() / 1024);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.log.push(format!("channel-dl: {} failed: {e}", active.fname));
                println!("[Download] {} failed: {e}", active.fname);
            }
        }
        self.start_next_file_download();
        self.flush();
    }

    /// Aborts a stalled channel download (30 s without new bytes) and moves on.
    /// Slow-but-moving transfers (the server throttles file data to ~14 KB/s
    /// at `NETSPEED 25000`) are allowed to run as long as bytes keep arriving.
    fn check_file_timeout(&mut self) {
        let stalled = self.file_active.as_ref().is_some_and(|a| a.last_data.elapsed() > Duration::from_secs(30));
        if stalled {
            if let Some(a) = self.file_active.take() {
                self.log.push(format!("channel-dl: {} stalled after {} KB in {:.0} s", a.fname, a.data.len() / 1024, a.started.elapsed().as_secs_f32()));
                println!("[Download] {} stalled, aborting", a.fname);
            }
            self.start_next_file_download();
        }
    }

    /// Current channel-download progress for the HUD: (file, received, total, queued).
    pub fn file_progress(&self) -> Option<(String, u64, u64, usize)> {
        self.file_active.as_ref().map(|a| (a.fname.clone(), a.data.len() as u64, a.size, self.file_queue.len()))
    }

    /// Unified download progress (background FastDL or channel) for the HUD:
    /// (source "HTTP"/"CHAN", file, received bytes, total or 0, queued files).
    pub fn download_progress(&self) -> Option<(String, String, u64, u64, usize)> {
        if let Some((f, rx, tot, q)) = self.file_progress() {
            return Some(("CHAN".into(), f, rx, tot, q));
        }
        let st = self.http_state.as_ref()?.lock().ok()?;
        let (f, rx, tot) = st.current.clone()?;
        Some(("HTTP".into(), f, rx, tot.unwrap_or(0), st.queued))
    }

    /// True while downloads (FastDL background or channel) hold `WELCOME`.
    pub fn downloading(&self) -> bool {
        self.welcome_pending.is_some()
    }
}

impl Drop for LiveClient {
    /// Ends the Steam game connection (retail `TerminateGameConnection` on disconnect).
    fn drop(&mut self) {
        if let (Some(s), std::net::IpAddr::V4(v)) = (self.steam.as_ref(), self.server.ip()) {
            s.terminate(u32::from(v), self.server.port());
        }
    }
}
