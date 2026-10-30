// OpenKF - Killing Floor Retail Server Matchmaking & Valve A2S_INFO Protocol
// AppID 1250 (Retail Killing Floor) Steam Master Server Query and Server Info Parser.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// Official Killing Floor (KF1) Retail Steam AppID
pub const KILLING_FLOOR_APP_ID: u32 = 1250;

/// Default master servers queried for server discovery
pub const DEFAULT_MASTER_SERVERS: &[&str] = &[
    "hl2master.steampowered.com:27011",
    "hl1master.steampowered.com:27011",
    "208.64.200.65:27011",
    "208.64.200.52:27011",
    "208.64.200.39:27011",
    "208.64.201.194:27011",
];

/// A2S Protocol Header Constants
pub const A2S_HEADER_SINGLE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
pub const A2S_REQUEST_INFO: u8 = 0x54; // 'T'
pub const A2S_RESPONSE_INFO: u8 = 0x49; // 'I'
pub const A2S_RESPONSE_CHALLENGE: u8 = 0x41; // 'A'
pub const A2S_PAYLOAD_SOURCE_ENGINE: &[u8] = b"Source Engine Query\0";

/// Master Server Query Protocol Constants
pub const MASTER_REQUEST_TYPE: u8 = 0x31; // '1'
pub const MASTER_REGION_ALL: u8 = 0xFF;
pub const MASTER_RESPONSE_HEADER: [u8; 6] = [0xFF, 0xFF, 0xFF, 0xFF, 0x66, 0x0A];

/// Extra Data Flag (EDF) bits for A2S_INFO
pub const EDF_PORT: u8 = 0x80;
pub const EDF_STEAM_ID: u8 = 0x10;
pub const EDF_SPECTATOR_PORT: u8 = 0x40;
pub const EDF_SPECTATOR_NAME: u8 = 0x20;
pub const EDF_KEYWORDS: u8 = 0x01;
pub const EDF_GAME_ID: u8 = 0x04;

/// GameSpy status request (the query protocol KF1 servers answer).
pub const GAMESPY_STATUS_REQUEST: &[u8] = b"\\status\\";

/// KF1 query-port offset: a game server on UDP `game` answers GameSpy
/// status on `game + 10` (default 7707 -> 7717).
pub const KF_QUERY_PORT_OFFSET: u16 = 10;

/// Server details from a GameSpy `\status\` query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameSpyStatus {
    pub hostname: String,
    pub mapname: String,
    pub maptitle: String,
    pub gametype: String,
    pub numplayers: u32,
    pub maxplayers: u32,
    pub gamever: String,
    pub hostport: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum SteamQueryError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Packet too short: received {len} bytes, minimum {min}")]
    PacketTooShort { len: usize, min: usize },

    #[error("Invalid packet header: expected {expected:?}, got {got:?}")]
    InvalidHeader { expected: Vec<u8>, got: Vec<u8> },

    #[error("Invalid packet response type: expected 0x{expected:02X}, got 0x{got:02X}")]
    InvalidResponseType { expected: u8, got: u8 },

    #[error("Server query timed out for {0}")]
    Timeout(SocketAddrV4),

    #[error("All master servers unreachable")]
    MasterServerUnreachable,

    #[error("Failed challenge verification with server {0}")]
    ChallengeFailed(SocketAddrV4),

    #[error("DNS resolution failed: {0}")]
    Dns(String),

    #[error("Steam Web API error: {0}")]
    WebApi(String),
}

/// Server host operating environment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Windows,
    Linux,
    Mac,
    Unknown(u8),
}

impl From<u8> for Environment {
    fn from(b: u8) -> Self {
        match b {
            b'w' | b'W' => Environment::Windows,
            b'l' | b'L' => Environment::Linux,
            b'm' | b'M' | b'o' | b'O' => Environment::Mac,
            other => Environment::Unknown(other),
        }
    }
}

/// Server host architecture / dedicated mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerType {
    Dedicated,
    NonDedicated,
    SourceTvProxy,
    Unknown(u8),
}

impl From<u8> for ServerType {
    fn from(b: u8) -> Self {
        match b {
            b'd' | b'D' => ServerType::Dedicated,
            b'l' | b'L' => ServerType::NonDedicated,
            b'p' | b'P' => ServerType::SourceTvProxy,
            other => ServerType::Unknown(other),
        }
    }
}

/// Parsed metadata for a Killing Floor / Source Engine server
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub addr: Option<SocketAddrV4>,
    pub ping_ms: Option<u32>,
    pub protocol: u8,
    pub name: String,
    pub map: String,
    pub folder: String,
    pub game: String,
    pub app_id: u16,
    pub players: u8,
    pub max_players: u8,
    pub bots: u8,
    pub server_type: ServerType,
    pub environment: Environment,
    pub visibility: bool, // true if password-protected
    pub vac: bool,        // true if VAC secured
    pub version: String,
    pub port: Option<u16>,
    pub steam_id: Option<u64>,
    pub spectator_port: Option<u16>,
    pub spectator_name: Option<String>,
    pub keywords: Option<String>,
    pub game_id: Option<u64>,
}

/// Steam Server Browser & A2S Query Client
pub struct SteamServerQuery {
    pub master_servers: Vec<String>,
    pub timeout: Duration,
    pub max_retries: usize,
}

impl Default for SteamServerQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl SteamServerQuery {
    /// Create a new query instance with default settings
    pub fn new() -> Self {
        Self {
            master_servers: DEFAULT_MASTER_SERVERS.iter().map(|&s| s.to_string()).collect(),
            timeout: Duration::from_millis(1500),
            max_retries: 2,
        }
    }

    /// Customize master servers list
    pub fn with_master_servers(mut self, servers: Vec<String>) -> Self {
        self.master_servers = servers;
        self
    }

    /// Customize timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Construct a master server query packet for a specific region and filter
    pub fn build_master_server_request(region: u8, last_ip_port: &str, filter: &str) -> Vec<u8> {
        let mut packet = Vec::with_capacity(64);
        packet.push(MASTER_REQUEST_TYPE);
        packet.push(region);
        packet.extend_from_slice(last_ip_port.as_bytes());
        packet.push(0x00);
        packet.extend_from_slice(filter.as_bytes());
        packet.push(0x00);
        packet
    }

    /// Parse raw UDP response from Steam Master Server, returning (servers, has_more)
    pub fn parse_master_server_batch(response: &[u8]) -> Result<(Vec<SocketAddrV4>, bool), SteamQueryError> {
        if response.len() < 6 {
            return Err(SteamQueryError::PacketTooShort {
                len: response.len(),
                min: 6,
            });
        }

        if !response.starts_with(&MASTER_RESPONSE_HEADER) {
            return Err(SteamQueryError::InvalidHeader {
                expected: MASTER_RESPONSE_HEADER.to_vec(),
                got: response[..6.min(response.len())].to_vec(),
            });
        }

        let mut addrs = Vec::new();
        let mut cursor = 6;
        let mut has_more = true;

        while cursor + 6 <= response.len() {
            let ip_bytes = [
                response[cursor],
                response[cursor + 1],
                response[cursor + 2],
                response[cursor + 3],
            ];
            let port = u16::from_be_bytes([response[cursor + 4], response[cursor + 5]]);
            cursor += 6;

            let ip = Ipv4Addr::from(ip_bytes);
            if ip.is_unspecified() && port == 0 {
                // End of server list marker 0.0.0.0:0
                has_more = false;
                break;
            }

            addrs.push(SocketAddrV4::new(ip, port));
        }

        Ok((addrs, has_more))
    }

    /// Parse raw UDP response from Steam Master Server
    pub fn parse_master_server_response(response: &[u8]) -> Result<Vec<SocketAddrV4>, SteamQueryError> {
        Self::parse_master_server_batch(response).map(|(addrs, _)| addrs)
    }

    /// Construct an A2S_INFO query packet, optionally including anti-amplification challenge
    pub fn build_a2s_info_request(challenge: Option<u32>) -> Vec<u8> {
        let mut packet = Vec::with_capacity(32);
        packet.extend_from_slice(&A2S_HEADER_SINGLE);
        packet.push(A2S_REQUEST_INFO);
        packet.extend_from_slice(A2S_PAYLOAD_SOURCE_ENGINE);
        if let Some(c) = challenge {
            packet.extend_from_slice(&c.to_le_bytes());
        }
        packet
    }

    /// Parse A2S_INFO response packet according to Valve Server Query Protocol
    pub fn parse_a2s_info(response: &[u8]) -> Result<ServerInfo, SteamQueryError> {
        let mut cursor = 0;

        // Verify standard single-packet header (0xFFFFFFFF) if present
        if response.starts_with(&A2S_HEADER_SINGLE) {
            cursor += 4;
        }

        if cursor >= response.len() {
            return Err(SteamQueryError::PacketTooShort {
                len: response.len(),
                min: cursor + 1,
            });
        }

        let response_type = response[cursor];
        cursor += 1;

        if response_type != A2S_RESPONSE_INFO {
            return Err(SteamQueryError::InvalidResponseType {
                expected: A2S_RESPONSE_INFO,
                got: response_type,
            });
        }

        if cursor >= response.len() {
            return Err(SteamQueryError::PacketTooShort {
                len: response.len(),
                min: cursor + 1,
            });
        }

        let protocol = response[cursor];
        cursor += 1;

        let name = read_cstring(response, &mut cursor)?;
        let map = read_cstring(response, &mut cursor)?;
        let folder = read_cstring(response, &mut cursor)?;
        let game = read_cstring(response, &mut cursor)?;

        if cursor + 2 > response.len() {
            return Err(SteamQueryError::PacketTooShort {
                len: response.len(),
                min: cursor + 2,
            });
        }
        let app_id = u16::from_le_bytes([response[cursor], response[cursor + 1]]);
        cursor += 2;

        if cursor + 7 > response.len() {
            return Err(SteamQueryError::PacketTooShort {
                len: response.len(),
                min: cursor + 7,
            });
        }

        let players = response[cursor];
        cursor += 1;
        let max_players = response[cursor];
        cursor += 1;
        let bots = response[cursor];
        cursor += 1;
        let server_type = ServerType::from(response[cursor]);
        cursor += 1;
        let environment = Environment::from(response[cursor]);
        cursor += 1;
        let visibility = response[cursor] != 0;
        cursor += 1;
        let vac = response[cursor] != 0;
        cursor += 1;

        let version = read_cstring(response, &mut cursor)?;

        let mut port = None;
        let mut steam_id = None;
        let mut spectator_port = None;
        let mut spectator_name = None;
        let mut keywords = None;
        let mut game_id = None;

        // Extra Data Flags (EDF)
        if cursor < response.len() {
            let edf = response[cursor];
            cursor += 1;

            if (edf & EDF_PORT) != 0 && cursor + 2 <= response.len() {
                port = Some(u16::from_le_bytes([response[cursor], response[cursor + 1]]));
                cursor += 2;
            }

            if (edf & EDF_STEAM_ID) != 0 && cursor + 8 <= response.len() {
                steam_id = Some(u64::from_le_bytes(
                    response[cursor..cursor + 8].try_into().unwrap(),
                ));
                cursor += 8;
            }

            if (edf & EDF_SPECTATOR_PORT) != 0 && cursor + 2 <= response.len() {
                spectator_port = Some(u16::from_le_bytes([response[cursor], response[cursor + 1]]));
                cursor += 2;
            }

            if (edf & EDF_SPECTATOR_NAME) != 0 {
                spectator_name = read_cstring(response, &mut cursor).ok();
            }

            if (edf & EDF_KEYWORDS) != 0 {
                keywords = read_cstring(response, &mut cursor).ok();
            }

            if (edf & EDF_GAME_ID) != 0 && cursor + 8 <= response.len() {
                game_id = Some(u64::from_le_bytes(
                    response[cursor..cursor + 8].try_into().unwrap(),
                ));
            }
        }

        Ok(ServerInfo {
            addr: None,
            ping_ms: None,
            protocol,
            name,
            map,
            folder,
            game,
            app_id,
            players,
            max_players,
            bots,
            server_type,
            environment,
            visibility,
            vac,
            version,
            port,
            steam_id,
            spectator_port,
            spectator_name,
            keywords,
            game_id,
        })
    }

    /// Query a single server with A2S_INFO over UDP, handling S2C_CHALLENGE handshakes
    pub fn query_server_info(&self, addr: SocketAddrV4) -> Result<ServerInfo, SteamQueryError> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(self.timeout))?;
        socket.set_write_timeout(Some(self.timeout))?;

        let mut buf = [0u8; 4096];
        let mut challenge = None;

        let start_time = Instant::now();

        for _ in 0..=self.max_retries {
            let req = Self::build_a2s_info_request(challenge);
            socket.send_to(&req, addr)?;

            let (amt, _src) = match socket.recv_from(&mut buf) {
                Ok(res) => res,
                Err(e)
                    if e.kind() == io::ErrorKind::TimedOut
                        || e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::ConnectionReset =>
                {
                    return Err(SteamQueryError::Timeout(addr));
                }
                Err(e) => return Err(SteamQueryError::Io(e)),
            };

            let received = &buf[..amt];

            // Check if server replied with S2C_CHALLENGE (0xFFFFFFFF 0x41 <4-byte challenge>)
            if received.len() >= 9
                && received.starts_with(&A2S_HEADER_SINGLE)
                && received[4] == A2S_RESPONSE_CHALLENGE
            {
                challenge = Some(u32::from_le_bytes(received[5..9].try_into().unwrap()));
                continue;
            }

            let mut info = Self::parse_a2s_info(received)?;
            info.addr = Some(addr);
            info.ping_ms = Some(start_time.elapsed().as_millis() as u32);
            return Ok(info);
        }

        Err(SteamQueryError::ChallengeFailed(addr))
    }

    /// Query a Steam Master Server for list of active game servers matching the given AppID
    pub fn query_master_server(&self, master_addr: &str, app_id: u32) -> Result<Vec<SocketAddrV4>, SteamQueryError> {
        let addrs: Vec<SocketAddr> = master_addr.to_socket_addrs()?.collect();
        let target_addr = match addrs.into_iter().next() {
            Some(a) => a,
            None => return Err(SteamQueryError::MasterServerUnreachable),
        };

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(self.timeout))?;
        socket.set_write_timeout(Some(self.timeout))?;

        let filter = format!(r"\appid\{}", app_id);
        let mut last_ip_port = "0.0.0.0:0".to_string();
        let mut all_servers = Vec::new();
        let mut buf = [0u8; 4096];

        // Paginate responses (limit to 10 batches to avoid endless loops)
        for _ in 0..10 {
            let req = Self::build_master_server_request(MASTER_REGION_ALL, &last_ip_port, &filter);
            socket.send_to(&req, target_addr)?;

            let (amt, _) = match socket.recv_from(&mut buf) {
                Ok(res) => res,
                Err(e)
                    if e.kind() == io::ErrorKind::TimedOut
                        || e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::ConnectionReset =>
                {
                    break;
                }
                Err(e) => return Err(SteamQueryError::Io(e)),
            };

            let (batch, has_more) = Self::parse_master_server_batch(&buf[..amt])?;
            if batch.is_empty() {
                break;
            }

            if let Some(last) = batch.last() {
                last_ip_port = format!("{}:{}", last.ip(), last.port());
            }

            all_servers.extend(batch);

            if !has_more {
                break;
            }
        }

        Ok(all_servers)
    }

    /// Query master servers and fetch live metadata for all retrieved servers
    pub fn query_internet_servers(&self, app_id: u32) -> Result<Vec<ServerInfo>, SteamQueryError> {
        let mut server_addrs = Vec::new();
        let mut last_err = None;

        for master in &self.master_servers {
            match self.query_master_server(master, app_id) {
                Ok(addrs) if !addrs.is_empty() => {
                    server_addrs = addrs;
                    break;
                }
                Ok(addrs) => {
                    server_addrs = addrs;
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        if server_addrs.is_empty() {
            if let Some(err) = last_err {
                return Err(err);
            }
        }

        let mut results = Vec::new();
        for addr in server_addrs {
            if let Ok(info) = self.query_server_info(addr) {
                results.push(info);
            }
        }

        Ok(results)
    }

    /// Parses a GameSpy `\status\` reply (`\key\value\...`) into server details.
    /// This is the query protocol KF1 dedicated servers actually answer (A2S
    /// is not implemented by the retail server). Verified live against UCC.
    pub fn parse_gamespy_status(packet: &[u8]) -> Result<GameSpyStatus, SteamQueryError> {
        let text = String::from_utf8_lossy(packet);
        let parts: Vec<&str> = text.split('\\').collect();
        let mut st = GameSpyStatus::default();
        let mut pairs = 0usize;
        // Skip the leading empty segment (`\key\value...` starts with `\`).
        let mut it = parts.into_iter().peekable();
        if it.peek() == Some(&"") {
            it.next();
        }
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            if k.is_empty() {
                continue;
            }
            pairs += 1;
            match k {
                "hostname" => st.hostname = v.to_string(),
                "mapname" => st.mapname = v.to_string(),
                "maptitle" => st.maptitle = v.to_string(),
                "gametype" => st.gametype = v.to_string(),
                "numplayers" => st.numplayers = v.parse().unwrap_or(0),
                "maxplayers" => st.maxplayers = v.parse().unwrap_or(0),
                "gamever" => st.gamever = v.to_string(),
                "hostport" => st.hostport = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        if pairs == 0 || (st.hostname.is_empty() && st.mapname.is_empty()) {
            return Err(SteamQueryError::InvalidHeader {
                expected: b"\\hostname\\...".to_vec(),
                got: packet.iter().copied().take(32).collect(),
            });
        }
        Ok(st)
    }

    /// Queries a KF1 server's GameSpy status. `game_addr` is the GAME port
    /// (what clients connect to); the query goes to `game + 10`, falling back
    /// to the given port itself (covers pasted query ports).
    pub fn query_gamespy_status(&self, game_addr: SocketAddrV4) -> Result<GameSpyStatus, SteamQueryError> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(self.timeout))?;
        socket.set_write_timeout(Some(self.timeout))?;
        let mut buf = [0u8; 4096];
        let targets = [
            SocketAddrV4::new(*game_addr.ip(), game_addr.port().wrapping_add(KF_QUERY_PORT_OFFSET)),
            game_addr,
        ];
        for t in targets {
            let _ = socket.send_to(GAMESPY_STATUS_REQUEST, t);
            match socket.recv_from(&mut buf) {
                Ok((n, _)) => {
                    if let Ok(st) = Self::parse_gamespy_status(&buf[..n]) {
                        return Ok(st);
                    }
                }
                Err(_) => continue,
            }
        }
        Err(SteamQueryError::Timeout(game_addr))
    }

    /// Internet server list via the Steam Web API
    /// (`IGameServersService/GetServerList`, needs a free key from
    /// steamcommunity.com/dev/apikey). Returns game addresses; query each
    /// with [`SteamServerQuery::query_gamespy_status`] for details.
    /// NOTE: the API returns the GAME port (`addr` field).
    pub fn fetch_steam_web_list(api_key: &str, app_id: u32, limit: u32) -> Result<Vec<SocketAddrV4>, SteamQueryError> {
        let url = format!(
            "https://api.steampowered.com/IGameServersService/GetServerList/v1/?key={api_key}&filter=%5Cappid%5C{app_id}&limit={limit}"
        );
        // SChannel TLS via an explicit agent (ureq 2.x never picks native-tls
        // as the default connector for the `ureq::get` shortcuts).
        let connector = std::sync::Arc::new(ureq::native_tls::TlsConnector::new().map_err(|e| SteamQueryError::WebApi(e.to_string()))?);
        let agent = ureq::builder().tls_connector(connector).build();
        let body = agent
            .get(&url)
            .timeout(Duration::from_secs(15))
            .call()
            .map_err(|e| match &e {
                ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => {
                    SteamQueryError::WebApi(format!("{e} (invalid API key?)"))
                }
                _ => SteamQueryError::WebApi(e.to_string()),
            })?
            .into_string()
            .map_err(|e| SteamQueryError::WebApi(e.to_string()))?;
        Self::parse_steam_web_list(&body)
    }

    /// Parses a `GetServerList` JSON body into game addresses.
    pub fn parse_steam_web_list(body: &str) -> Result<Vec<SocketAddrV4>, SteamQueryError> {
        let v: serde_json::Value = serde_json::from_str(body).map_err(|e| SteamQueryError::WebApi(format!("bad JSON: {e}")))?;
        let mut out = Vec::new();
        if let Some(arr) = v.pointer("/response/servers").and_then(|s| s.as_array()) {
            for s in arr {
                if let Some(addr) = s.get("addr").and_then(|a| a.as_str()) {
                    if let Ok(SocketAddr::V4(v4)) = addr.parse::<SocketAddr>() {
                        out.push(v4);
                    } else if let Ok(ip) = addr.parse::<Ipv4Addr>() {
                        out.push(SocketAddrV4::new(ip, 7707));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Discovers KF1 servers on the LAN: broadcasts `\status\` to `query_port`
    /// and collects replies until `listen` elapses. Returns (game addr, status).
    pub fn scan_lan(&self, query_port: u16, listen: Duration) -> Vec<(SocketAddrV4, GameSpyStatus)> {
        let mut out = Vec::new();
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else { return out };
        if socket.set_broadcast(true).is_err() {
            return out;
        }
        let _ = socket.set_read_timeout(Some(Duration::from_millis(250)));
        let bcast = SocketAddrV4::new(Ipv4Addr::BROADCAST, query_port);
        let _ = socket.send_to(GAMESPY_STATUS_REQUEST, bcast);
        let deadline = Instant::now() + listen;
        let mut buf = [0u8; 4096];
        while Instant::now() < deadline {
            match socket.recv_from(&mut buf) {
                Ok((n, SocketAddr::V4(from))) => {
                    if let Ok(st) = Self::parse_gamespy_status(&buf[..n]) {
                        let game = SocketAddrV4::new(*from.ip(), from.port().saturating_sub(KF_QUERY_PORT_OFFSET));
                        if !out.iter().any(|(a, _): &(SocketAddrV4, GameSpyStatus)| *a == game) {
                            out.push((game, st));
                        }
                    }
                }
                _ => continue,
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

/// Helper function to read null-terminated C-style string
fn read_cstring(data: &[u8], cursor: &mut usize) -> Result<String, SteamQueryError> {
    let start = *cursor;
    while *cursor < data.len() && data[*cursor] != 0 {
        *cursor += 1;
    }
    if *cursor >= data.len() {
        return Err(SteamQueryError::PacketTooShort {
            len: data.len(),
            min: *cursor + 1,
        });
    }
    let slice = &data[start..*cursor];
    *cursor += 1; // Skip the null byte
    Ok(String::from_utf8_lossy(slice).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_a2s_info_request_no_challenge() {
        let req = SteamServerQuery::build_a2s_info_request(None);
        assert_eq!(&req[0..4], &A2S_HEADER_SINGLE);
        assert_eq!(req[4], A2S_REQUEST_INFO);
        assert_eq!(&req[5..], b"Source Engine Query\0");
    }

    #[test]
    fn test_build_a2s_info_request_with_challenge() {
        let challenge: u32 = 0x12345678;
        let req = SteamServerQuery::build_a2s_info_request(Some(challenge));
        assert_eq!(&req[0..4], &A2S_HEADER_SINGLE);
        assert_eq!(req[4], A2S_REQUEST_INFO);
        assert_eq!(&req[5..25], b"Source Engine Query\0");
        assert_eq!(&req[25..29], &challenge.to_le_bytes());
    }

    #[test]
    fn test_build_master_server_request() {
        let req = SteamServerQuery::build_master_server_request(MASTER_REGION_ALL, "0.0.0.0:0", r"\appid\1250");
        assert_eq!(req[0], MASTER_REQUEST_TYPE);
        assert_eq!(req[1], MASTER_REGION_ALL);
        assert!(req.windows(b"0.0.0.0:0\0".len()).any(|w| w == b"0.0.0.0:0\0"));
        assert!(req.windows(b"\\appid\\1250\0".len()).any(|w| w == b"\\appid\\1250\0"));
    }

    #[test]
    fn test_parse_master_server_response() {
        let mut packet = Vec::new();
        packet.extend_from_slice(&MASTER_RESPONSE_HEADER);
        // Server 1: 185.97.253.250:7707 (port 7707 = 0x1E1B)
        packet.extend_from_slice(&[185, 97, 253, 250, 0x1E, 0x1B]);
        // Server 2: 57.129.47.100:7708 (port 7708 = 0x1E1C)
        packet.extend_from_slice(&[57, 129, 47, 100, 0x1E, 0x1C]);
        // Terminator: 0.0.0.0:0
        packet.extend_from_slice(&[0, 0, 0, 0, 0, 0]);

        let addrs = SteamServerQuery::parse_master_server_response(&packet).unwrap();
        assert_eq!(addrs.len(), 2);
        assert_eq!(addrs[0], SocketAddrV4::new(Ipv4Addr::new(185, 97, 253, 250), 7707));
        assert_eq!(addrs[1], SocketAddrV4::new(Ipv4Addr::new(57, 129, 47, 100), 7708));
    }

    #[test]
    fn test_parse_master_server_response_invalid_header() {
        let packet = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        let res = SteamServerQuery::parse_master_server_response(&packet);
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_a2s_info_retail_killing_floor() {
        let mut packet = Vec::new();
        packet.extend_from_slice(&A2S_HEADER_SINGLE);
        packet.push(A2S_RESPONSE_INFO); // 'I'
        packet.push(17); // Protocol version
        packet.extend_from_slice(b"WS-Gaming.eu | KF1 Retail v1065 [Hard]\0");
        packet.extend_from_slice(b"KF-BioticsLab\0");
        packet.extend_from_slice(b"killingfloor\0");
        packet.extend_from_slice(b"Killing Floor\0");
        packet.extend_from_slice(&1250u16.to_le_bytes()); // AppID 1250
        packet.push(4); // Players
        packet.push(6); // Max Players
        packet.push(0); // Bots
        packet.push(b'd'); // ServerType: Dedicated
        packet.push(b'w'); // Environment: Windows
        packet.push(0); // Visibility: Public
        packet.push(1); // VAC: Secured
        packet.extend_from_slice(b"1065\0"); // Version

        let info = SteamServerQuery::parse_a2s_info(&packet).unwrap();
        assert_eq!(info.protocol, 17);
        assert_eq!(info.name, "WS-Gaming.eu | KF1 Retail v1065 [Hard]");
        assert_eq!(info.map, "KF-BioticsLab");
        assert_eq!(info.folder, "killingfloor");
        assert_eq!(info.game, "Killing Floor");
        assert_eq!(info.app_id, 1250);
        assert_eq!(info.players, 4);
        assert_eq!(info.max_players, 6);
        assert_eq!(info.bots, 0);
        assert_eq!(info.server_type, ServerType::Dedicated);
        assert_eq!(info.environment, Environment::Windows);
        assert!(!info.visibility);
        assert!(info.vac);
        assert_eq!(info.version, "1065");
        assert!(info.port.is_none());
    }

    #[test]
    fn test_parse_a2s_info_with_edf() {
        let mut packet = Vec::new();
        packet.extend_from_slice(&A2S_HEADER_SINGLE);
        packet.push(A2S_RESPONSE_INFO);
        packet.push(17);
        packet.extend_from_slice(b"Killing Floor Server\0");
        packet.extend_from_slice(b"KF-WestLondon\0");
        packet.extend_from_slice(b"killingfloor\0");
        packet.extend_from_slice(b"Killing Floor\0");
        packet.extend_from_slice(&1250u16.to_le_bytes());
        packet.push(2);
        packet.push(6);
        packet.push(1);
        packet.push(b'l'); // Listen server
        packet.push(b'l'); // Linux
        packet.push(1); // Password protected
        packet.push(0); // No VAC
        packet.extend_from_slice(b"1065\0");

        // EDF flags: Port (0x80) | SteamID (0x10) | Keywords (0x01)
        let edf = EDF_PORT | EDF_STEAM_ID | EDF_KEYWORDS;
        packet.push(edf);
        packet.extend_from_slice(&7707u16.to_le_bytes()); // Port 7707
        let steam_id = 90071996842377216u64;
        packet.extend_from_slice(&steam_id.to_le_bytes()); // SteamID
        packet.extend_from_slice(b"normal,perks_enabled\0"); // Keywords

        let info = SteamServerQuery::parse_a2s_info(&packet).unwrap();
        assert_eq!(info.server_type, ServerType::NonDedicated);
        assert_eq!(info.environment, Environment::Linux);
        assert!(info.visibility);
        assert!(!info.vac);
        assert_eq!(info.port, Some(7707));
        assert_eq!(info.steam_id, Some(steam_id));
        assert_eq!(info.keywords, Some("normal,perks_enabled".to_string()));
    }

    #[test]
    fn test_parse_a2s_info_truncated() {
        let packet = vec![0xFF, 0xFF, 0xFF, 0xFF, A2S_RESPONSE_INFO, 17];
        let res = SteamServerQuery::parse_a2s_info(&packet);
        assert!(res.is_err());
    }

    #[test]
    fn test_e2e_mock_server_query_with_challenge() {
        // Spin up a mock game server on 127.0.0.1
        let server_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = match server_socket.local_addr().unwrap() {
            SocketAddr::V4(a) => a,
            _ => unreachable!(),
        };

        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 1024];

            // 1. Receive initial query without challenge
            let (len, client_addr) = server_socket.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[0..4], &A2S_HEADER_SINGLE);
            assert_eq!(buf[4], A2S_REQUEST_INFO);
            assert_eq!(len, 25); // No challenge appended

            // 2. Respond with S2C_CHALLENGE
            let challenge: u32 = 0xDEADBEEF;
            let mut challenge_pkt = Vec::new();
            challenge_pkt.extend_from_slice(&A2S_HEADER_SINGLE);
            challenge_pkt.push(A2S_RESPONSE_CHALLENGE);
            challenge_pkt.extend_from_slice(&challenge.to_le_bytes());
            server_socket.send_to(&challenge_pkt, client_addr).unwrap();

            // 3. Receive second query WITH challenge
            let (len2, client_addr2) = server_socket.recv_from(&mut buf).unwrap();
            assert_eq!(len2, 29); // 25 + 4 bytes challenge
            assert_eq!(&buf[25..29], &challenge.to_le_bytes());

            // 4. Respond with valid A2S_INFO packet
            let mut info_pkt = Vec::new();
            info_pkt.extend_from_slice(&A2S_HEADER_SINGLE);
            info_pkt.push(A2S_RESPONSE_INFO);
            info_pkt.push(17);
            info_pkt.extend_from_slice(b"Mock KF Server\0");
            info_pkt.extend_from_slice(b"KF-Manor\0");
            info_pkt.extend_from_slice(b"killingfloor\0");
            info_pkt.extend_from_slice(b"Killing Floor\0");
            info_pkt.extend_from_slice(&1250u16.to_le_bytes());
            info_pkt.push(5);
            info_pkt.push(6);
            info_pkt.push(0);
            info_pkt.push(b'd');
            info_pkt.push(b'w');
            info_pkt.push(0);
            info_pkt.push(1);
            info_pkt.extend_from_slice(b"1065\0");

            server_socket.send_to(&info_pkt, client_addr2).unwrap();
        });

        let query = SteamServerQuery::new().with_timeout(Duration::from_secs(2));
        let info = query.query_server_info(server_addr).expect("Query should succeed");

        assert_eq!(info.name, "Mock KF Server");
        assert_eq!(info.map, "KF-Manor");
        assert_eq!(info.players, 5);
        assert_eq!(info.max_players, 6);
        assert_eq!(info.app_id, 1250);
        assert!(info.ping_ms.is_some());
        assert_eq!(info.addr, Some(server_addr));

        handle.join().unwrap();
    }

    #[test]
    fn test_e2e_mock_master_server_and_query_internet() {
        // Spin up a mock game server on 127.0.0.1
        let game_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let game_addr = match game_socket.local_addr().unwrap() {
            SocketAddr::V4(a) => a,
            _ => unreachable!(),
        };

        // Spin up a mock master server on 127.0.0.1
        let master_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let master_addr = master_socket.local_addr().unwrap().to_string();

        let master_handle = std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            let (_len, client_addr) = master_socket.recv_from(&mut buf).unwrap();

            let mut resp = Vec::new();
            resp.extend_from_slice(&MASTER_RESPONSE_HEADER);
            // Include our game server address
            resp.extend_from_slice(&game_addr.ip().octets());
            resp.extend_from_slice(&game_addr.port().to_be_bytes());
            // Terminator
            resp.extend_from_slice(&[0, 0, 0, 0, 0, 0]);

            master_socket.send_to(&resp, client_addr).unwrap();
        });

        let game_handle = std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            let (_len, client_addr) = game_socket.recv_from(&mut buf).unwrap();

            let mut info_pkt = Vec::new();
            info_pkt.extend_from_slice(&A2S_HEADER_SINGLE);
            info_pkt.push(A2S_RESPONSE_INFO);
            info_pkt.push(17);
            info_pkt.extend_from_slice(b"Local Dev Killing Floor Server\0");
            info_pkt.extend_from_slice(b"KF-Offices\0");
            info_pkt.extend_from_slice(b"killingfloor\0");
            info_pkt.extend_from_slice(b"Killing Floor\0");
            info_pkt.extend_from_slice(&1250u16.to_le_bytes());
            info_pkt.push(1);
            info_pkt.push(6);
            info_pkt.push(0);
            info_pkt.push(b'd');
            info_pkt.push(b'w');
            info_pkt.push(0);
            info_pkt.push(1);
            info_pkt.extend_from_slice(b"1065\0");

            game_socket.send_to(&info_pkt, client_addr).unwrap();
        });

        let query = SteamServerQuery::new()
            .with_master_servers(vec![master_addr])
            .with_timeout(Duration::from_secs(2));

        let servers = query.query_internet_servers(KILLING_FLOOR_APP_ID).expect("Master query should succeed");
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Local Dev Killing Floor Server");
        assert_eq!(servers[0].map, "KF-Offices");
        assert_eq!(servers[0].players, 1);
        assert_eq!(servers[0].app_id, 1250);

        master_handle.join().unwrap();
        game_handle.join().unwrap();
    }

    #[test]
    fn test_parse_gamespy_status_live_shape() {
        // Shape captured live from UCC (`\status\` on the query port).
        let packet = b"\\gamename\\redorchestra\\gamever\\3339\\minnetver\\3180\\location\\0\\hostname\\Killing Floor Server\\hostport\\7707\\maptitle\\KF-Labo Biotics\\mapname\\KF-BioticsLab\\gametype\\KFGameType\\numplayers\\2\\maxplayers\\6\\gamemode\\openplaying\\difficulty\\2\\";
        let st = SteamServerQuery::parse_gamespy_status(packet).unwrap();
        assert_eq!(st.hostname, "Killing Floor Server");
        assert_eq!(st.mapname, "KF-BioticsLab");
        assert_eq!(st.maptitle, "KF-Labo Biotics");
        assert_eq!(st.gametype, "KFGameType");
        assert_eq!(st.numplayers, 2);
        assert_eq!(st.maxplayers, 6);
        assert_eq!(st.gamever, "3339");
        assert_eq!(st.hostport, 7707);
    }

    #[test]
    fn test_parse_gamespy_status_rejects_garbage() {
        assert!(SteamServerQuery::parse_gamespy_status(b"").is_err());
        assert!(SteamServerQuery::parse_gamespy_status(b"\xff\xff\xff\xffT").is_err());
        assert!(SteamServerQuery::parse_gamespy_status(b"\\foo\\bar\\").is_err());
    }

    #[test]
    fn test_e2e_mock_gamespy_status_query() {
        // Mock KF1 query port: answers `\status\` with a fixed reply.
        let server_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = match server_socket.local_addr().unwrap() {
            SocketAddr::V4(a) => a,
            _ => unreachable!(),
        };
        let reply = b"\\gamename\\redorchestra\\hostname\\Mock Server\\mapname\\KF-WestLondon\\gametype\\KFGameType\\numplayers\\1\\maxplayers\\6\\gamever\\3339\\hostport\\7707\\".to_vec();
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            // May receive up to two probes (game+10 guess, then as-given fallback).
            for _ in 0..2 {
                let Ok((n, from)) = server_socket.recv_from(&mut buf) else { break };
                if &buf[..n] == GAMESPY_STATUS_REQUEST {
                    server_socket.send_to(&reply, from).unwrap();
                    break;
                }
            }
        });

        // Query the mock's exact port: the game+10 guess times out fast, then
        // the as-given fallback succeeds.
        let query = SteamServerQuery::new().with_timeout(Duration::from_millis(300));
        let st = query.query_gamespy_status(server_addr).expect("mock query should succeed");
        assert_eq!(st.hostname, "Mock Server");
        assert_eq!(st.mapname, "KF-WestLondon");
        assert_eq!((st.numplayers, st.maxplayers), (1, 6));
        handle.join().unwrap();
    }

    #[test]
    fn test_gamespy_status_timeout_is_error() {
        // Nothing listens on this high loopback port: must time out, not hang.
        let query = SteamServerQuery::new().with_timeout(Duration::from_millis(200));
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 59999);
        assert!(matches!(query.query_gamespy_status(addr), Err(SteamQueryError::Timeout(_))));
    }

    #[test]
    fn test_parse_steam_web_list() {
        let body = r#"{"response":{"servers":[
            {"addr":"1.2.3.4:7707","gameport":7707,"steamid":"9001","name":"A","appid":1250,"gamedir":"KillingFloor","version":"1065","product":"KillingFloor","region":3,"players":2,"max_players":6,"bots":0,"map":"KF-WestLondon","secure":true,"dedicated":true,"os":"w","gametype":"KFGameType"},
            {"addr":"5.6.7.8","gameport":7707,"steamid":"9002","name":"B","appid":1250,"gamedir":"KillingFloor","version":"1065","product":"KillingFloor","region":3,"players":0,"max_players":6,"bots":0,"map":"KF-Farm","secure":true,"dedicated":true,"os":"l","gametype":"KFGameType"},
            {"addr":"not-an-address","gameport":0,"steamid":"9003"}
        ]}}"#;
        let list = SteamServerQuery::parse_steam_web_list(body).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 7707));
        // Bare IP defaults to the KF game port.
        assert_eq!(list[1], SocketAddrV4::new(Ipv4Addr::new(5, 6, 7, 8), 7707));
    }

    #[test]
    fn test_parse_steam_web_list_rejects_garbage() {
        assert!(SteamServerQuery::parse_steam_web_list("not json").is_err());
        // Valid JSON, no servers -> empty list (not an error).
        assert!(SteamServerQuery::parse_steam_web_list(r#"{"response":{}}"#).unwrap().is_empty());
    }
}
