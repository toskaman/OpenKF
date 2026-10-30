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
    /// in-flight file chunks keyed by filename
    inflight: HashMap<String, Vec<u8>>,
    expected: HashMap<String, u64>,
    #[allow(dead_code)]
    kf_dir: PathBuf,
}

impl DownloadManager {
    pub fn new(kf_dir: &Path) -> Self {
        Self { redirect_url: None, inflight: HashMap::new(), expected: HashMap::new(), kf_dir: kf_dir.to_path_buf() }
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
