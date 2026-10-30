//! Steam auth blob without a capture: talks to the local Steam client through
//! `steamclient64.dll` (64-bit; the game's own steam_api.dll is 32-bit).
//!
//! Same call as KF's client: `ISteamUser017::InitiateGameConnection(blob, 2048,
//! serverSteamID, serverIP, serverPort, bSecure)`; the 2048-byte buffer is sent
//! as 16 `STEAMCLIENTBLOB SIZE=<n> CHUNK=<i> BLOB=<hex of 128 bytes>` messages.

/// SteamID64 of the logged-in account, from `HKCU\Software\Valve\Steam\ActiveProcess\ActiveUser`.
pub fn steam_id_from_account(account: u32) -> u64 {
    76_561_197_960_265_728 + account as u64
}

/// Builds the 16 control messages from a raw 2048-byte buffer.
pub fn blob_messages(buf: &[u8], size: usize) -> Vec<String> {
    (0..16)
        .map(|i| {
            let chunk = buf.get(i * 128..(i + 1) * 128).unwrap_or(&[]);
            let hex: String = chunk.iter().map(|b| format!("{b:02x}")).collect();
            format!("STEAMCLIENTBLOB SIZE={size} CHUNK={i} BLOB={hex}")
        })
        .collect()
}

#[cfg(windows)]
mod imp {
    use std::ffi::{c_char, c_void, CString};

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExA(name: *const c_char, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }
    const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;

    type CreateInterfaceFn = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;

    unsafe fn vfn(obj: *mut c_void, index: usize) -> *const c_void {
        let vtable = *(obj as *const *const *const c_void);
        *vtable.add(index)
    }

    /// Live Steam session (pipe + user) used to produce auth blobs.
    pub struct SteamSession {
        user: *mut c_void,
        pub account: u32,
    }
    unsafe impl Send for SteamSession {}

    fn reg_query(key: &str, value: &str) -> Option<String> {
        let out = std::process::Command::new("reg").args(["query", key, "/v", value]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        text.lines().find(|l| l.contains(value)).and_then(|l| l.split_whitespace().nth(2).map(|s| {
            // REG_SZ may contain spaces: take everything after the type column.
            let idx = l.find(s).unwrap_or(0);
            l[idx..].trim().to_string()
        }))
    }

    impl SteamSession {
        pub fn open(app_id: u32) -> Result<Self, String> {
            std::env::set_var("SteamAppId", app_id.to_string());
            std::env::set_var("SteamGameId", app_id.to_string());
            let dir = std::env::var("OPENKF_STEAM_DIR")
                .ok()
                .or_else(|| reg_query(r"HKCU\Software\Valve\Steam", "SteamPath"))
                .ok_or("Steam path not found (HKCU\\Software\\Valve\\Steam\\SteamPath)")?;
            let account = reg_query(r"HKCU\Software\Valve\Steam\ActiveProcess", "ActiveUser")
                .and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok())
                .filter(|&a| a != 0)
                .ok_or("no Steam user logged in (ActiveUser = 0)")?;
            let dll = format!("{}\\steamclient64.dll", dir.replace('/', "\\"));
            unsafe {
                let path = CString::new(dll.clone()).unwrap();
                let h = LoadLibraryExA(path.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH);
                if h.is_null() {
                    return Err(format!("cannot load {dll}"));
                }
                let ci = GetProcAddress(h, c"CreateInterface".as_ptr());
                if ci.is_null() {
                    return Err("CreateInterface not exported".into());
                }
                let create: CreateInterfaceFn = std::mem::transmute(ci);
                let client = create(c"SteamClient012".as_ptr(), std::ptr::null_mut());
                if client.is_null() {
                    return Err("SteamClient012 unavailable".into());
                }
                // ISteamClient012: 0 CreateSteamPipe, 2 ConnectToGlobalUser, 5 GetISteamUser.
                let create_pipe: unsafe extern "system" fn(*mut c_void) -> i32 = std::mem::transmute(vfn(client, 0));
                let connect: unsafe extern "system" fn(*mut c_void, i32) -> i32 = std::mem::transmute(vfn(client, 2));
                let get_user: unsafe extern "system" fn(*mut c_void, i32, i32, *const c_char) -> *mut c_void = std::mem::transmute(vfn(client, 5));
                let pipe = create_pipe(client);
                if pipe == 0 {
                    return Err("CreateSteamPipe failed (is Steam running?)".into());
                }
                let huser = connect(client, pipe);
                if huser == 0 {
                    return Err("ConnectToGlobalUser failed".into());
                }
                let user = get_user(client, huser, pipe, c"SteamUser017".as_ptr());
                if user.is_null() {
                    return Err("SteamUser017 unavailable".into());
                }
                Ok(Self { user, account })
            }
        }

        pub fn steam_id(&self) -> u64 {
            super::steam_id_from_account(self.account)
        }

        /// `InitiateGameConnection` → (2048-byte buffer, used size).
        pub fn initiate(&self, server_steam_id: u64, ip: u32, port: u16, secure: bool) -> Result<(Vec<u8>, usize), String> {
            let mut buf = vec![0u8; 2048];
            unsafe {
                // ISteamUser017: 3 InitiateGameConnection.
                let f: unsafe extern "system" fn(*mut c_void, *mut c_void, i32, u64, u32, u16, bool) -> i32 = std::mem::transmute(vfn(self.user, 3));
                let n = f(self.user, buf.as_mut_ptr() as _, 2048, server_steam_id, ip, port, secure);
                if n <= 0 {
                    return Err(format!("InitiateGameConnection returned {n}"));
                }
                Ok((buf, n as usize))
            }
        }

        pub fn terminate(&self, ip: u32, port: u16) {
            unsafe {
                let f: unsafe extern "system" fn(*mut c_void, u32, u16) = std::mem::transmute(vfn(self.user, 4));
                f(self.user, ip, port);
            }
        }
    }
}

#[cfg(windows)]
pub use imp::SteamSession;

#[cfg(not(windows))]
pub struct SteamSession {
    pub account: u32,
}
#[cfg(not(windows))]
impl SteamSession {
    pub fn open(_app_id: u32) -> Result<Self, String> {
        Err("Steam is only available on Windows".into())
    }
    pub fn steam_id(&self) -> u64 {
        steam_id_from_account(self.account)
    }
    pub fn initiate(&self, _s: u64, _ip: u32, _port: u16, _secure: bool) -> Result<(Vec<u8>, usize), String> {
        Err("Steam is only available on Windows".into())
    }
    pub fn terminate(&self, _ip: u32, _port: u16) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steam_id_and_chunks() {
        assert_eq!(steam_id_from_account(0x308a295), 76_561_198_011_163_285);
        let mut b = vec![0u8; 2048];
        b[0] = 0x14;
        let m = blob_messages(&b, 242);
        assert_eq!(m.len(), 16);
        assert!(m[0].starts_with("STEAMCLIENTBLOB SIZE=242 CHUNK=0 BLOB=14000000"));
        assert_eq!(m[15].len(), "STEAMCLIENTBLOB SIZE=242 CHUNK=15 BLOB=".len() + 256);
    }
}
