//! FastDL / UFileChannel downloader for custom map packages.
//!
//! Handles two server download modes:
//! - UFileChannel (CHTYPE_FILE=3): chunked transfer inside UE2 bunches.
//! - RedirectToURL / HTTP: server sends URL via control message; we fetch via HTTP.
//!
//! Files are cached under `<kf_dir>/Cache/` and validated by GUID/MD5 from USES.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Download progress for UI.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub file: String,
    pub received: u64,
    pub total: Option<u64>,
}

/// Manages concurrent file downloads (FastDL path).
pub struct DownloadManager {
    /// base url from server RedirectToURL, if any
    pub redirect_url: Option<String>,
    /// `DLMGR CLASS=IpDrv.HTTPDownload ... COMPRESSION=1`: files are served as `.uz2`.
    pub compressed: bool,
    /// in-flight file chunks keyed by filename
    inflight: HashMap<String, Vec<u8>>,
    expected: HashMap<String, u64>,
    #[allow(dead_code)]
    kf_dir: PathBuf,
}

impl DownloadManager {
    pub fn new(kf_dir: &Path) -> Self {
        Self { redirect_url: None, compressed: false, inflight: HashMap::new(), expected: HashMap::new(), kf_dir: kf_dir.to_path_buf() }
    }

    pub fn set_redirect(&mut self, url: String) {
        self.redirect_url = Some(url);
    }

    /// Register expected file size from server USES/NEEDED.
    pub fn expect(&mut self, name: String, size: u64) {
        self.expected.insert(name.clone(), size);
        self.inflight.entry(name).or_insert_with(Vec::new);
    }

    /// Push a chunk received on CHTYPE_FILE.
    pub fn push_chunk(&mut self, name: &str, data: &[u8]) -> Option<DownloadProgress> {
        let buf = self.inflight.entry(name.to_string()).or_default();
        // Cap 256 MB per file to avoid OOM from malicious server.
        if buf.len() + data.len() > 256 * 1024 * 1024 {
            return None;
        }
        buf.extend_from_slice(data);
        let total = self.expected.get(name).copied();
        Some(DownloadProgress { file: name.to_string(), received: buf.len() as u64, total })
    }

    pub fn is_complete(&self, name: &str) -> bool {
        if let (Some(buf), Some(&exp)) = (self.inflight.get(name), self.expected.get(name)) {
            buf.len() as u64 >= exp
        } else { false }
    }

    pub fn pending_count(&self) -> usize {
        self.inflight.len()
    }
}

// ---------------------------------------------------------------- HTTP redirect (FastDL)

/// Folder where packages downloaded from servers are stored, outside the game
/// installation: `%LOCALAPPDATA%\OpenKF\Downloads` (fallback `<kf>/OpenKF-Downloads`).
pub fn download_dir(kf_dir: &Path) -> PathBuf {
    if let Some(d) = std::env::var_os("OPENKF_DOWNLOADS") {
        return PathBuf::from(d);
    }
    match std::env::var_os("LOCALAPPDATA") {
        Some(d) => PathBuf::from(d).join("OpenKF").join("Downloads"),
        None => kf_dir.join("OpenKF-Downloads"),
    }
}

/// Standard KF package folders.
pub const PACKAGE_DIRS: &[&str] = &["System", "Maps", "Textures", "Sounds", "StaticMeshes", "Animations", "Music", "KarmaData"];

/// Retail download cache: `<kf>/Cache/Cache.ini` maps `GUID=FileName.ext`, files are
/// stored as `<kf>/Cache/<GUID>.uxx`. Returns lower-cased file name -> cached path.
pub fn retail_cache(kf_dir: &Path) -> &'static HashMap<String, PathBuf> {
    static CACHE: std::sync::OnceLock<HashMap<String, PathBuf>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let dir = kf_dir.join("Cache");
        let mut m = HashMap::new();
        if let Ok(text) = std::fs::read(dir.join("Cache.ini")) {
            for line in String::from_utf8_lossy(&text).lines() {
                if let Some((guid, name)) = line.trim().split_once('=') {
                    let p = dir.join(format!("{}.uxx", guid.trim()));
                    if guid.trim().len() == 32 && p.is_file() {
                        m.insert(name.trim().to_ascii_lowercase(), p);
                    }
                }
            }
        }
        m
    })
}

/// Finds a package file (`KF-Foo.rom`, `Bar.utx`) in the game folders, the retail
/// download cache, then the OpenKF download folder.
pub fn find_package_file(kf_dir: &Path, fname: &str) -> Option<PathBuf> {
    PACKAGE_DIRS
        .iter()
        .map(|d| kf_dir.join(d).join(fname))
        .find(|p| p.is_file())
        .or_else(|| retail_cache(kf_dir).get(&fname.to_ascii_lowercase()).cloned())
        .or_else(|| Some(download_dir(kf_dir).join(fname)).filter(|p| p.is_file()))
}

/// `PACKAGE_FILE_TAG` of UE2 packages.
pub const PACKAGE_TAG: u32 = 0x9E2A_83C1;

/// GUID of a package file as printed by the server in `USES GUID=` (4 little-endian u32, %08X).
pub fn package_guid(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = [0u8; 52];
    f.read_exact(&mut h).ok()?;
    if u32::from_le_bytes([h[0], h[1], h[2], h[3]]) & 0xFFFF_FF00 != PACKAGE_TAG & 0xFFFF_FF00 {
        return None;
    }
    Some(h[36..52].chunks(4).map(|c| format!("{:08X}", u32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect())
}

/// `http://host[:port]/path/` -> (host, port, path prefix ending with '/').
pub fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.trim().strip_prefix("http://")?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().ok()?),
        None => (hostport.to_string(), 80),
    };
    if host.is_empty() {
        return None;
    }
    let mut path = path.to_string();
    if !path.ends_with('/') {
        path.push('/');
    }
    Some((host, port, path))
}

/// Percent-encodes a file name for a URL path.
pub fn url_encode(name: &str) -> String {
    let mut out = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Hard cap for one downloaded file (compressed or not).
pub const MAX_DOWNLOAD: usize = 256 * 1024 * 1024;

/// Minimal HTTP/1.0 GET (FastDL redirect servers are plain HTTP). Follows up to 3 redirects.
pub fn http_get(url: &str, timeout: std::time::Duration) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};
    use std::net::ToSocketAddrs;
    let mut url = url.to_string();
    for _ in 0..4 {
        let rest = url.strip_prefix("http://").ok_or_else(|| format!("unsupported URL {url}"))?;
        let (hostport, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().map_err(|e| e.to_string())?),
            None => (hostport, 80),
        };
        let addr = (host, port).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or("no address")?;
        let mut s = std::net::TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connect {addr}: {e}"))?;
        s.set_read_timeout(Some(timeout)).ok();
        s.set_write_timeout(Some(timeout)).ok();
        let req = format!("GET {path} HTTP/1.0\r\nHost: {hostport}\r\nUser-Agent: Unreal\r\nAccept: */*\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let n = s.read(&mut chunk).map_err(|e| format!("read: {e}"))?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > MAX_DOWNLOAD {
                return Err("file too large".into());
            }
        }
        let end = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or("bad HTTP response")?;
        let head = String::from_utf8_lossy(&buf[..end]).to_string();
        let status: u16 = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or("bad status line")?;
        let header = |k: &str| head.lines().find_map(|l| l.split_once(':').filter(|(n, _)| n.trim().eq_ignore_ascii_case(k)).map(|(_, v)| v.trim().to_string()));
        match status {
            200 => {
                let body = buf.split_off(end + 4);
                if let Some(len) = header("Content-Length").and_then(|v| v.parse::<usize>().ok()) {
                    if body.len() < len {
                        return Err(format!("truncated: {} / {len} bytes", body.len()));
                    }
                }
                return Ok(body);
            }
            301 | 302 | 303 | 307 | 308 => {
                let loc = header("Location").ok_or("redirect without Location")?;
                url = if loc.starts_with("http://") { loc } else { format!("http://{hostport}{loc}") };
            }
            _ => return Err(format!("HTTP {status}")),
        }
    }
    Err("too many redirects".into())
}

/// Decompresses a UE2 `.uz2` file: blocks of [compressed size u32][uncompressed size u32][zlib data].
pub fn decompress_uz2(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut o = 0usize;
    while o < data.len() {
        if o + 8 > data.len() {
            return Err("truncated block header".into());
        }
        let comp = u32::from_le_bytes(data[o..o + 4].try_into().unwrap()) as usize;
        let raw = u32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap()) as usize;
        o += 8;
        if comp > data.len() - o || raw > 1 << 20 || out.len() + raw > MAX_DOWNLOAD {
            return Err(format!("bad block at {o}: {comp} -> {raw}"));
        }
        let mut block = Vec::with_capacity(raw);
        flate2::read::ZlibDecoder::new(&data[o..o + comp]).take(raw as u64).read_to_end(&mut block).map_err(|e| format!("zlib: {e}"))?;
        if block.len() != raw {
            return Err(format!("block size {} != {raw}", block.len()));
        }
        out.extend_from_slice(&block);
        o += comp;
    }
    Ok(out)
}

/// Downloads `fname` from a FastDL redirect (`<base>/<fname>[.uz2]`) into `download_dir`.
/// The package GUID is checked against `expected_guid` when given. Returns the stored path.
pub fn fetch_package(base: &str, compressed: bool, fname: &str, expected_guid: Option<&str>, dest_dir: &Path) -> Result<PathBuf, String> {
    let (host, port, path) = parse_http_url(base).ok_or_else(|| format!("bad redirect URL {base}"))?;
    let timeout = std::time::Duration::from_secs(45);
    // Redirect servers are often slow or rate-limited: one retry per URL.
    let try_get = |name: &str| {
        let url = format!("http://{host}:{port}{path}{}", url_encode(name));
        http_get(&url, timeout).or_else(|e| if e.starts_with("HTTP 4") { Err(e) } else { http_get(&url, timeout) })
    };
    // Servers advertise COMPRESSION=1 but some only host raw files: try both.
    let bytes = if compressed {
        match try_get(&format!("{fname}.uz2")) {
            Ok(b) => decompress_uz2(&b)?,
            Err(e) => try_get(fname).map_err(|e2| format!("{e} / raw: {e2}"))?,
        }
    } else {
        try_get(fname)?
    };
    let mut bytes = bytes;
    if bytes.len() < 52 {
        return Err("downloaded file is too small".into());
    }
    match u32::from_le_bytes(bytes[0..4].try_into().unwrap()) {
        PACKAGE_TAG => {}
        // Some custom maps ship with a tweaked tag (0x9E2A83C2) to stop editor imports;
        // the summary that follows is a regular v128/29 package. Store it normalized.
        t if t & 0xFFFF_FF00 == PACKAGE_TAG & 0xFFFF_FF00 => bytes[0..4].copy_from_slice(&PACKAGE_TAG.to_le_bytes()),
        _ => return Err("downloaded file is not an Unreal package".into()),
    }
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let tmp = dest_dir.join(format!("{fname}.part"));
    let dst = dest_dir.join(fname);
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    if let Some(g) = expected_guid {
        if package_guid(&tmp).as_deref().map(|x| x.eq_ignore_ascii_case(g)) != Some(true) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("GUID mismatch for {fname}"));
        }
    }
    std::fs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
    Ok(dst)
}

#[cfg(test)]
mod fastdl_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_redirect_urls() {
        assert_eq!(parse_http_url("http://203.0.113.5:7819/"), Some(("203.0.113.5".into(), 7819, "/".into())));
        assert_eq!(parse_http_url("http://dl.example.org/kf"), Some(("dl.example.org".into(), 80, "/kf/".into())));
        assert_eq!(parse_http_url("ftp://x/"), None);
    }

    #[test]
    fn encodes_map_names() {
        assert_eq!(url_encode("KF-Space-Airlock[WSB].rom.uz2"), "KF-Space-Airlock%5BWSB%5D.rom.uz2");
    }

    #[test]
    fn uz2_roundtrip() {
        let raw: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let mut file = Vec::new();
        for block in raw.chunks(32768) {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(block).unwrap();
            let c = e.finish().unwrap();
            file.extend_from_slice(&(c.len() as u32).to_le_bytes());
            file.extend_from_slice(&(block.len() as u32).to_le_bytes());
            file.extend_from_slice(&c);
        }
        assert_eq!(decompress_uz2(&file).unwrap(), raw);
        assert!(decompress_uz2(&file[..file.len() - 3]).is_err());
    }
}
