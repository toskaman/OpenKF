//! Server browser (v1): favorites + recent + direct IP.
//!
//! The old UDP master servers (`hl2master.steampowered.com`, GameSpy) are gone
//! and KF1 dedicated servers do not answer A2S/GameSpy queries in a usable
//! way, so there is no internet list yet (needs Steamworks
//! `ISteamMatchmakingServers`, future work). This browser manages direct
//! servers instead: named favorites, auto-saved recents, and manual entry.
//! Lists live in `%LOCALAPPDATA%\OpenKF` (`favorites.txt`, `recent.txt`).

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

/// Interactive pick: numbered favorites + recents, or a typed `ip:port`.
/// `fav`/`unfav` edit the list and re-prompt; empty input / EOF quits.
pub fn pick() -> Option<String> {
    use std::io::{BufRead, Write};
    loop {
        let favs = favorites();
        let rec = recent();
        println!("=== OpenKF servers ===");
        let mut n = 0usize;
        let mut addrs: Vec<String> = Vec::new();
        if !favs.is_empty() {
            println!("-- favorites --");
            for e in &favs {
                n += 1;
                println!("{n:3}  {:24}  {}", e.addr, e.name);
                addrs.push(e.addr.clone());
            }
        }
        if !rec.is_empty() {
            println!("-- recent --");
            for e in rec.iter().filter(|e| !favs.iter().any(|f| f.addr == e.addr)) {
                n += 1;
                println!("{n:3}  {}", e.addr);
                addrs.push(e.addr.clone());
            }
        }
        if n == 0 {
            println!("(no saved servers yet)");
        }
        println!("Enter a number, an ip:port, 'fav <num-or-ip> <name>' to save, 'unfav <num-or-ip-or-name>' to remove, or empty to quit.");
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
}
