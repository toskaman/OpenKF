//! In-viewport server browser overlay (windowed mode, `F2`).
//!
//! Lists favorites + recents + LAN servers with live GameSpy details, rendered
//! through the HUD text block (6 lines visible, the list scrolls). Queries run
//! on worker threads; picking a server restarts the client onto it.

use std::sync::mpsc::{self, Receiver, Sender};

/// One row: address, source tag, optional live details line + details-pane line.
#[derive(Debug, Clone)]
pub struct Row {
    pub addr: String,
    pub tag: String,
    pub info: Option<String>,
    pub detail: Option<String>,
}

enum Msg {
    Details(Vec<(String, Option<(String, String)>)>),
    Lan(Vec<(String, String, String)>),
}

pub struct BrowserUi {
    rows: Vec<Row>,
    cursor: usize,
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    loading: bool,
    scanning: bool,
}

impl BrowserUi {
    pub fn open() -> Self {
        let favs = super::browser::favorites();
        let rec = super::browser::recent();
        let mut rows = Vec::new();
        for e in &favs {
            rows.push(Row { addr: e.addr.clone(), tag: format!("fav {}", e.name), info: None, detail: None });
        }
        for e in rec.iter().filter(|e| !favs.iter().any(|f| f.addr == e.addr)) {
            rows.push(Row { addr: e.addr.clone(), tag: "recent".into(), info: None, detail: None });
        }
        let (tx, rx) = mpsc::channel();
        let mut ui = Self { rows, cursor: 0, rx, tx, loading: true, scanning: false };
        ui.query_details();
        ui.lan_scan();
        ui
    }

    fn query_details(&self) {
        let tx = self.tx.clone();
        let addrs: Vec<String> = self.rows.iter().map(|r| r.addr.clone()).collect();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            std::thread::scope(|s| {
                let hs: Vec<_> = addrs.iter().map(|a| s.spawn(|| (a.clone(), super::browser::details(a)))).collect();
                for h in hs {
                    let (addr, st) = h.join().unwrap();
                    out.push((addr, st.map(|st| super::browser::detail_lines(&st))));
                }
            });
            let _ = tx.send(Msg::Details(out));
        });
    }

    /// Pump worker results (non-blocking). Call every frame while open.
    pub fn pump(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Details(v) => {
                    for (addr, both) in v {
                        if let Some(r) = self.rows.iter_mut().find(|r| r.addr == addr) {
                            match both {
                                Some((info, detail)) => {
                                    r.info = Some(info);
                                    r.detail = Some(detail);
                                }
                                None => {
                                    r.info = Some("(offline)".into());
                                    r.detail = None;
                                }
                            }
                        }
                    }
                    self.loading = false;
                }
                Msg::Lan(v) => {
                    for (addr, info, detail) in v {
                        if !self.rows.iter().any(|r| r.addr == addr) {
                            self.rows.push(Row { addr, tag: "lan".into(), info: Some(info), detail: Some(detail) });
                        }
                    }
                    self.scanning = false;
                }
            }
        }
    }

    pub fn lan_scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let found = super::browser::lan()
                .into_iter()
                .map(|(a, st)| {
                    let (info, detail) = super::browser::detail_lines(&st);
                    (a, info, detail)
                })
                .collect();
            let _ = tx.send(Msg::Lan(found));
        });
    }

    /// Move the cursor (clamped).
    pub fn mov(&mut self, dy: i32) {
        if self.rows.is_empty() {
            return;
        }
        self.cursor = (self.cursor as i32 + dy).clamp(0, self.rows.len() as i32 - 1) as usize;
    }

    pub fn selected(&self) -> Option<&str> {
        self.rows.get(self.cursor).map(|r| r.addr.as_str())
    }

    /// 6 HUD lines: title, 4 scrolling rows, footer.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!("SERVERS ({}) - UP/DN SELECT - ENTER JOIN - L RESCAN - F2 CLOSE", self.rows.len())];
        let max_start = self.rows.len().saturating_sub(4);
        let start = self.cursor.saturating_sub(3).min(max_start);
        for (i, r) in self.rows.iter().enumerate().skip(start).take(4) {
            let mark = if i == self.cursor { ">" } else { " " };
            let info = r.info.as_deref().unwrap_or(if self.loading { "querying..." } else { "" });
            let mut line = format!("{mark} {} [{}] {}", r.addr, r.tag, info);
            line.truncate(80);
            out.push(line);
        }
        while out.len() < 5 {
            out.push(String::new());
        }
        // Footer doubles as the details pane: status while busy, otherwise the
        // selected server's full details (gametype, map title, version).
        let mut footer = if self.scanning {
            "scanning LAN...".to_string()
        } else if self.loading {
            "querying...".to_string()
        } else {
            match self.rows.get(self.cursor).and_then(|r| r.detail.clone()) {
                Some(d) => format!("> {d}"),
                None => "ready".into(),
            }
        };
        footer.truncate(80);
        out.push(footer);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui(n: usize) -> BrowserUi {
        let (tx, rx) = mpsc::channel();
        BrowserUi {
            rows: (0..n).map(|i| Row { addr: format!("10.0.0.{i}:7707"), tag: "recent".into(), info: Some("H | M | 1/6".into()), detail: Some(format!("KFGame | Manor | 1/6 | v3339")) }).collect(),
            cursor: 0,
            rx,
            tx,
            loading: false,
            scanning: false,
        }
    }

    #[test]
    fn lines_always_six() {
        for n in [0, 1, 4, 9] {
            let lines = ui(n).lines();
            assert_eq!(lines.len(), 6, "n={n}");
            assert!(lines.iter().all(|l| l.len() <= 80), "truncate to HUD width");
        }
    }

    #[test]
    fn cursor_moves_and_scrolls() {
        let mut u = ui(9);
        assert!(u.lines()[1].starts_with('>'));
        u.mov(5);
        assert_eq!(u.selected(), Some("10.0.0.5:7707"));
        // Cursor row visible and marked after scrolling.
        assert!(u.lines().iter().any(|l| l.starts_with("> 10.0.0.5")));
        u.mov(100);
        assert_eq!(u.selected(), Some("10.0.0.8:7707"));
        u.mov(-100);
        assert_eq!(u.selected(), Some("10.0.0.0:7707"));
    }

    #[test]
    fn pump_merges_details_and_lan() {
        let mut u = ui(1);
        u.rows[0].info = None;
        u.rows[0].detail = None;
        u.loading = true;
        let payload = vec![("10.0.0.0:7707".to_string(), Some(("H | M | 2/6".to_string(), "G | T | 2/6".to_string())))];
        u.tx.send(Msg::Details(payload)).unwrap();
        u.scanning = true;
        u.tx.send(Msg::Lan(vec![("10.0.0.9:7707".into(), "L | M | 0/6".into(), "G | T | 0/6".into())])).unwrap();
        u.pump();
        assert_eq!(u.rows[0].info.as_deref(), Some("H | M | 2/6"));
        assert_eq!(u.rows[0].detail.as_deref(), Some("G | T | 2/6"));
        assert!(!u.loading && !u.scanning);
        assert_eq!(u.rows.len(), 2);
        assert_eq!(u.rows[1].tag, "lan");
        assert_eq!(u.rows[1].detail.as_deref(), Some("G | T | 0/6"));
    }

    #[test]
    fn footer_is_details_pane_when_ready() {
        let u = ui(2);
        let lines = u.lines();
        assert_eq!(lines.len(), 6);
        assert!(lines[5].starts_with("> KFGame"), "footer: {}", lines[5]);
    }

    #[test]
    fn footer_shows_status_while_busy() {
        let (tx, rx) = mpsc::channel();
        let mut u = BrowserUi { rows: vec![], cursor: 0, rx, tx, loading: true, scanning: false };
        assert_eq!(u.lines()[5], "querying...");
        u.loading = false;
        u.scanning = true;
        assert_eq!(u.lines()[5], "scanning LAN...");
        u.scanning = false;
        assert_eq!(u.lines()[5], "ready");
    }
}
