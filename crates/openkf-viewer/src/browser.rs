//! Server browser (v3): favorites + recent + LAN + internet + direct IP.
//!
//! KF1 dedicated servers answer the GameSpy `\status\` query on `game + 10`
//! (default 7717) with hostname, map, and player counts, so every saved
//! server shows live details. `lan` discovers servers on the local network
//! via broadcast; `net` lists internet servers through the Steam Web API
//! (`IGameServersService/GetServerList`, needs a free key). The old UDP
//! masters (`hl2master.steampowered.com`, GameSpy) are gone and KF1 does
//! not speak A2S, so there is no keyless internet source.
//! Lists live in `%LOCALAPPDATA%\OpenKF` (`favorites.txt`, `recent.txt`).

use std::net::{SocketAddr, SocketAddrV4};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub addr: String,
}

fn dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(d) => PathBuf::from(d).join("OpenKF"),
        None => PathBuf::from("OpenKF-data"),
    }
}

fn load(file: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(text) = std::fs::read_to_string(dir().join(file)) else { return out };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, addr) = match line.split_once('|') {
            Some((n, a)) => (n.trim().to_string(), a.trim().to_string()),
            None => (line.to_string(), line.to_string()),
        };
        if !addr.is_empty() && !out.iter().any(|e: &Entry| e.addr == addr) {
            out.push(Entry { name, addr });
        }
    }
    out
}

fn save(file: &str, list: &[Entry]) {
    let _ = std::fs::create_dir_all(dir());
    let text: String = list.iter().map(|e| format!("{}|{}\n", e.name.replace('|', "/"), e.addr)).collect();
    let _ = std::fs::write(dir().join(file), text);
}

pub fn favorites() -> Vec<Entry> {
    load("favorites.txt")
}

pub fn recent() -> Vec<Entry> {
    load("recent.txt")
}

/// Records a joined server at the head of the recents (dedupe, cap 20).
pub fn push_recent(addr: &str) {
    let addr = addr.trim().to_string();
    if addr.is_empty() {
        return;
    }
    let mut list = recent();
    list.retain(|e| e.addr != addr);
    list.insert(0, Entry { name: addr.clone(), addr });
    list.truncate(20);
    save("recent.txt", &list);
}

pub fn add_favorite(name: &str, addr: &str) {
    let mut list = favorites();
    list.retain(|e| e.addr != addr);
    list.push(Entry { name: name.trim().to_string(), addr: addr.trim().to_string() });
    save("favorites.txt", &list);
}

pub fn remove_favorite(addr: &str) -> bool {
    let mut list = favorites();
    let n = list.len();
    list.retain(|e| e.addr != addr);
    save("favorites.txt", &list);
    list.len() != n
}

/// Validates `host:port` (resolvable, port present).
pub fn valid_addr(s: &str) -> bool {
    let s = s.trim();
    s.rsplit_once(':').is_some_and(|(h, p)| !h.is_empty() && p.parse::<u16>().is_ok()) && std::net::ToSocketAddrs::to_socket_addrs(s).is_ok()
}

fn resolve_v4(s: &str) -> Option<SocketAddrV4> {
    std::net::ToSocketAddrs::to_socket_addrs(s).ok()?.find_map(|a| match a {
        SocketAddr::V4(v) => Some(v),
        _ => None,
    })
}

/// Live GameSpy details for one game address (`None` when offline).
pub fn details(addr: &str) -> Option<openkf_network::browser::GameSpyStatus> {
    let v4 = resolve_v4(addr)?;
    let q = openkf_network::browser::SteamServerQuery::new().with_timeout(std::time::Duration::from_millis(600));
    q.query_gamespy_status(v4).ok()
}

/// Discovers LAN servers (broadcast `\status\` on the default query port).
pub fn lan() -> Vec<(String, openkf_network::browser::GameSpyStatus)> {
    let q = openkf_network::browser::SteamServerQuery::new();
    q.scan_lan(7707 + openkf_network::browser::KF_QUERY_PORT_OFFSET, std::time::Duration::from_secs(2))
        .into_iter()
        .map(|(a, st)| (a.to_string(), st))
        .collect()
}

/// Steam Web API key: `OPENKF_STEAM_API_KEY` env or `%LOCALAPPDATA%\OpenKF\steam_api_key.txt`.
pub fn steam_api_key() -> Option<String> {
    if let Some(k) = std::env::var("OPENKF_STEAM_API_KEY").ok().filter(|k| !k.trim().is_empty()) {
        return Some(k.trim().to_string());
    }
    let p = dir().join("steam_api_key.txt");
    std::fs::read_to_string(p).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// Internet-list cache (`netcache.txt` in the data dir): fetching the Steam
/// list plus a GameSpy query per server takes a while, so fresh results are
/// reused for [`NETCACHE_TTL_SECS`]. TSV: `# <unix_secs>` header, then
/// `addr` + tab + `hostname|mapname|maptitle|gametype|num|max|gamever|hostport`
/// (`|` escaped as `/`; empty status = silent server).
pub const NETCACHE_TTL_SECS: u64 = 600;

type NetRow = (String, Option<openkf_network::browser::GameSpyStatus>);

fn esc(s: &str) -> String {
    s.replace('|', "/").replace('\t', " ").replace('\n', " ")
}

fn netcache_save_to(path: &std::path::Path, servers: &[NetRow]) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut text = format!("# {now}\n");
    for (addr, st) in servers {
        text.push_str(addr);
        text.push('\t');
        if let Some(st) = st {
            text.push_str(&format!("{}|{}|{}|{}|{}|{}|{}|{}", esc(&st.hostname), esc(&st.mapname), esc(&st.maptitle), esc(&st.gametype), st.numplayers, st.maxplayers, esc(&st.gamever), st.hostport));
        }
        text.push('\n');
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    let _ = std::fs::write(path, text);
}

fn netcache_load_from(path: &std::path::Path, max_age_secs: u64) -> Option<Vec<NetRow>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let stamp: u64 = lines.next()?.strip_prefix("# ")?.trim().parse().ok()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let age = now.saturating_sub(stamp);
    if age > max_age_secs {
        return None;
    }
    let mut out = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (addr, st) = match line.split_once('\t') {
            Some((a, s)) => (a.trim(), s.trim()),
            None => (line, ""),
        };
        if addr.is_empty() {
            continue;
        }
        let status = if st.is_empty() {
            None
        } else {
            let f: Vec<&str> = st.split('|').collect();
            if f.len() != 8 {
                continue;
            }
            Some(openkf_network::browser::GameSpyStatus {
                hostname: f[0].into(), mapname: f[1].into(), maptitle: f[2].into(), gametype: f[3].into(),
                numplayers: f[4].parse().unwrap_or(0), maxplayers: f[5].parse().unwrap_or(0),
                gamever: f[6].into(), hostport: f[7].parse().unwrap_or(0),
            })
        };
        out.push((addr.to_string(), status));
    }
    Some(out)
}

/// Internet servers via the Steam Web API (needs a key, see [`steam_api_key`]).
/// Each listed address is GameSpy-queried for details (parallel); silent ones
/// are kept with no details. Returns (servers, error, cache age in seconds
/// when served from the cache).
pub fn net(limit: u32) -> (Vec<NetRow>, Option<String>, Option<u64>) {
    let cache = dir().join("netcache.txt");
    if let Some(rows) = netcache_load_from(&cache, NETCACHE_TTL_SECS) {
        let age = cache_age_secs(&cache).unwrap_or(0);
        return (rows.into_iter().take(limit as usize).collect(), None, Some(age));
    }
    net_refresh(limit)
}

/// Bypasses the cache: always fetches the Steam list, re-queries details,
/// and rewrites the cache. Returns (servers, error).
pub fn net_refresh(limit: u32) -> (Vec<NetRow>, Option<String>, Option<u64>) {
    let Some(key) = steam_api_key() else {
        // No key: a stale cache is better than nothing.
        if let Some(rows) = netcache_load_from(&dir().join("netcache.txt"), u64::MAX) {
            return (rows.into_iter().take(limit as usize).collect(), None, cache_age_secs(&dir().join("netcache.txt")));
        }
        return (Vec::new(), Some("no Steam API key: set OPENKF_STEAM_API_KEY or save one to %LOCALAPPDATA%\\OpenKF\\steam_api_key.txt (free at steamcommunity.com/dev/apikey)".into()), None);
    };
    let addrs = match openkf_network::browser::SteamServerQuery::fetch_steam_web_list(&key, openkf_network::browser::KILLING_FLOOR_APP_ID, limit) {
        Ok(a) => a,
        Err(e) => return (Vec::new(), Some(format!("Steam Web API: {e}")), None),
    };
    let mut out: Vec<NetRow> = Vec::new();
    std::thread::scope(|s| {
        let hs: Vec<_> = addrs.iter().map(|a| s.spawn(|| (a.to_string(), details(&a.to_string())))).collect();
        for h in hs {
            out.push(h.join().unwrap());
        }
    });
    // Answering servers first, then the silent ones.
    out.sort_by_key(|(_, st)| st.is_none());
    netcache_save_to(&dir().join("netcache.txt"), &out);
    (out, None, None)
}

fn cache_age_secs(path: &std::path::Path) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    let stamp: u64 = text.lines().next()?.strip_prefix("# ")?.trim().parse().ok()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    Some(now.saturating_sub(stamp))
}

/// One-line row summary + details-pane line for a queried server.
pub fn detail_lines(st: &openkf_network::browser::GameSpyStatus) -> (String, String) {
    let row = format!("{} | {} | {}/{}", st.hostname, st.mapname, st.numplayers, st.maxplayers);
    let title = if st.maptitle.is_empty() { st.mapname.clone() } else { st.maptitle.clone() };
    let mut pane = format!("{} | {} | {}/{}", st.gametype, title, st.numplayers, st.maxplayers);
    if !st.gamever.is_empty() {
        pane.push_str(&format!(" | v{}", st.gamever));
    }
    (row, pane)
}

fn detail_line(st: &openkf_network::browser::GameSpyStatus) -> String {
    format!("{} players", detail_lines(st).0)
}

/// Interactive pick: numbered favorites + recents + LAN, or a typed `ip:port`.
/// `fav`/`unfav` edit the list, `lan` scans the local network; re-prompts
/// after each; empty input / EOF quits.
pub fn pick() -> Option<String> {
    use std::io::{BufRead, Write};
    let mut lan_servers: Vec<(String, openkf_network::browser::GameSpyStatus)> = Vec::new();
    let mut net_servers: Vec<(String, Option<openkf_network::browser::GameSpyStatus>)> = Vec::new();
    loop {
        let favs = favorites();
        let rec = recent();
        // Live details for saved servers, queried in parallel (600 ms each max).
        let mut probe: Vec<String> = favs.iter().map(|e| e.addr.clone()).collect();
        for e in rec.iter().filter(|e| !favs.iter().any(|f| f.addr == e.addr)) {
            probe.push(e.addr.clone());
        }
        let mut info: std::collections::HashMap<String, String> = Default::default();
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for addr in &probe {
                handles.push(s.spawn(|| (addr.clone(), details(addr).map(|st| detail_line(&st)))));
            }
            for h in handles {
                let (addr, d) = h.join().unwrap();
                info.insert(addr, d.unwrap_or_else(|| "(offline)".to_string()));
            }
        });
        println!("=== OpenKF servers ===");
        let mut n = 0usize;
        let mut addrs: Vec<String> = Vec::new();
        if !favs.is_empty() {
            println!("-- favorites --");
            for e in &favs {
                n += 1;
                println!("{n:3}  {:21}  {:24}  {}", e.addr, e.name, info.get(&e.addr).map(String::as_str).unwrap_or(""));
                addrs.push(e.addr.clone());
            }
        }
        if !rec.is_empty() {
            println!("-- recent --");
            for e in rec.iter().filter(|e| !favs.iter().any(|f| f.addr == e.addr)) {
                n += 1;
                println!("{n:3}  {:21}  {}", e.addr, info.get(&e.addr).map(String::as_str).unwrap_or(""));
                addrs.push(e.addr.clone());
            }
        }
        if !lan_servers.is_empty() {
            println!("-- lan --");
            for (addr, st) in &lan_servers {
                if addrs.contains(addr) {
                    continue;
                }
                n += 1;
                println!("{n:3}  {:21}  {}", addr, detail_line(st));
                addrs.push(addr.clone());
            }
        }
        if !net_servers.is_empty() {
            println!("-- internet (Steam) --");
            for (addr, st) in &net_servers {
                if addrs.contains(addr) {
                    continue;
                }
                n += 1;
                println!("{n:3}  {:21}  {}", addr, st.as_ref().map(detail_line).as_deref().unwrap_or("(silent)"));
                addrs.push(addr.clone());
            }
        }
        if n == 0 {
            println!("(no saved servers yet)");
        }
        println!("Enter a number, an ip:port, 'lan' to scan the local network, 'net' for the Steam internet list ('net!' forces a refresh), 'fav <num-or-ip> <name>' to save, 'unfav <num-or-ip-or-name>' to remove, or empty to quit.");
        print!("> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).ok().is_none_or(|n| n == 0) {
            return None;
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            return None;
        }
        if line.eq_ignore_ascii_case("lan") {
            println!("scanning LAN for KF1 servers...");
            lan_servers = lan();
            if lan_servers.is_empty() {
                println!("no LAN servers found");
            }
            continue;
        }
        if line.eq_ignore_ascii_case("net") || line.eq_ignore_ascii_case("net!") {
            let force = line.ends_with('!');
            println!("fetching Steam internet list...");
            let (servers, err, cached) = if force { net_refresh(50) } else { net(50) };
            if let Some(e) = err {
                println!("{e}");
            } else if servers.is_empty() {
                println!("no internet servers listed");
            } else if let Some(age) = cached {
                println!("{} servers (cached {}s ago)", servers.len(), age);
            }
            net_servers = servers;
            continue;
        }
        if let Some(rest) = line.strip_prefix("unfav ").or_else(|| line.strip_prefix("UNFAV ")) {
            let what = rest.trim();
            let addr = match what.parse::<usize>() {
                Ok(i) if i >= 1 && i <= addrs.len() => addrs[i - 1].clone(),
                _ => favs.iter().find(|e| e.name == what).map(|e| e.addr.clone()).unwrap_or_else(|| what.to_string()),
            };
            if remove_favorite(&addr) {
                println!("removed favorite {addr}");
            } else {
                println!("not a favorite: {addr}");
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("fav ").or_else(|| line.strip_prefix("FAV ")) {
            let rest = rest.trim();
            let (what, name) = rest.split_once(' ').unwrap_or((rest, rest));
            let addr = match what.parse::<usize>() {
                Ok(i) if i >= 1 && i <= addrs.len() => addrs[i - 1].clone(),
                _ => what.to_string(),
            };
            if !valid_addr(&addr) {
                println!("bad address: {addr}");
                continue;
            }
            add_favorite(name, &addr);
            println!("saved favorite '{name}' -> {addr}");
            return Some(addr);
        }
        if let Ok(i) = line.parse::<usize>() {
            if i >= 1 && i <= addrs.len() {
                return Some(addrs[i - 1].clone());
            }
            println!("no such entry: {i}");
            continue;
        }
        if valid_addr(&line) {
            return Some(line);
        }
        println!("enter a number or ip:port (got '{line}')");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_addresses() {
        assert!(valid_addr("127.0.0.1:7707"));
        assert!(valid_addr("localhost:7707"));
        assert!(!valid_addr("127.0.0.1"));
        assert!(!valid_addr("127.0.0.1:abc"));
        assert!(!valid_addr(""));
    }

    #[test]
    fn netcache_roundtrip_and_ttl() {
        let dir = std::env::temp_dir().join(format!("openkf-netcache-test-{}", std::process::id()));
        let path = dir.join("netcache.txt");
        let st = || openkf_network::browser::GameSpyStatus {
            hostname: "H|T".into(), mapname: "KF-B".into(), maptitle: "".into(), gametype: "G".into(),
            numplayers: 3, maxplayers: 6, gamever: "3339".into(), hostport: 7717,
        };
        let rows = vec![("1.2.3.4:7707".to_string(), Some(st())), ("5.6.7.8:7707".to_string(), None)];
        netcache_save_to(&path, &rows);
        let back = netcache_load_from(&path, NETCACHE_TTL_SECS).expect("fresh cache loads");
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].1.as_ref().unwrap().hostname, "H/T"); // '|' escaped
        assert_eq!(back[0].1.as_ref().unwrap().numplayers, 3);
        assert!(back[1].1.is_none());
        std::fs::write(&path, "# 1\n1.2.3.4:7707\t\n").unwrap();
        assert!(netcache_load_from(&path, NETCACHE_TTL_SECS).is_none(), "expired cache rejected");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detail_lines_fallbacks() {
        let st = openkf_network::browser::GameSpyStatus {
            hostname: "H".into(), mapname: "KF-B".into(), maptitle: "".into(), gametype: "G".into(),
            numplayers: 3, maxplayers: 6, gamever: "".into(), hostport: 0,
        };
        let (row, pane) = detail_lines(&st);
        assert_eq!(row, "H | KF-B | 3/6");
        assert_eq!(pane, "G | KF-B | 3/6"); // empty title/version fall back cleanly
    }
}
