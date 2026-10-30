//! Native 64-Bit Steamworks SDK Dynamic Loader (Pillar 12)
//!
//! Provides dynamic runtime loading of `steam_api64.dll` via Win32 `LoadLibraryW`
//! and symbol resolution using `GetProcAddress`. Eliminates link-time dependencies
//! on the Steamworks SDK while exposing a safe, RAII-governed Rust interface.

use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[allow(clippy::upper_case_acronyms)]
type HMODULE = *mut c_void;
#[allow(clippy::upper_case_acronyms)]
type FARPROC = *mut c_void;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(lpLibFileName: *const u16) -> HMODULE;
    fn GetProcAddress(hModule: HMODULE, lpProcName: *const u8) -> FARPROC;
    fn FreeLibrary(hLibModule: HMODULE) -> i32;
    fn GetLastError() -> u32;
}

// Steamworks C ABI Function Pointer Types (cdecl)
type SteamAPIInitFn = unsafe extern "C" fn() -> bool;
type SteamAPIShutdownFn = unsafe extern "C" fn();
type SteamAPIRunCallbacksFn = unsafe extern "C" fn();
type SteamAPIIsSteamRunningFn = unsafe extern "C" fn() -> bool;
type SteamAPIRestartAppIfNecessaryFn = unsafe extern "C" fn(u32) -> bool;

/// Error types encountered during Steamworks 64-bit dynamic loading and initialization.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SteamError {
    #[error("steam_api64.dll was not found in any search path")]
    DllNotFound,

    #[error("LoadLibraryW failed for '{path}': Win32 error code {code}")]
    LoadLibraryFailed { path: PathBuf, code: u32 },

    #[error("Failed to resolve symbol '{0}' in steam_api64.dll")]
    SymbolNotFound(String),

    #[error("Steam client is not currently running")]
    SteamNotRunning,

    #[error("SteamAPI_Init() failed for AppID {0} (check steam_appid.txt and Steam client status)")]
    InitFailed(u32),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Dynamic, safe RAII client for 64-bit Steamworks SDK (`steam_api64.dll`).
pub struct Steam64Client {
    module: HMODULE,
    app_id: u32,
    initialized: bool,
    _fn_init: SteamAPIInitFn,
    fn_shutdown: SteamAPIShutdownFn,
    fn_run_callbacks: SteamAPIRunCallbacksFn,
    fn_is_steam_running: SteamAPIIsSteamRunningFn,
    fn_restart_app: Option<SteamAPIRestartAppIfNecessaryFn>,
}

impl std::fmt::Debug for Steam64Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Steam64Client")
            .field("module", &self.module)
            .field("app_id", &self.app_id)
            .field("initialized", &self.initialized)
            .finish()
    }
}

unsafe impl Send for Steam64Client {}
unsafe impl Sync for Steam64Client {}

impl Steam64Client {
    /// Discovers `steam_api64.dll`, sets up AppID environment variables,
    /// loads the DLL, resolves Steamworks entry points, and calls `SteamAPI_Init`.
    pub fn init(app_id: u32) -> Result<Self, SteamError> {
        // 1. Set environment variables required by Steamworks SDK
        std::env::set_var("SteamAppId", app_id.to_string());
        std::env::set_var("SteamGameId", app_id.to_string());

        // 2. Discover steam_api64.dll location
        let dll_path = Self::find_steam_api64_dll().ok_or(SteamError::DllNotFound)?;

        // 3. Load from resolved path
        Self::load_from_path(&dll_path, app_id)
    }

    /// Loads `steam_api64.dll` from a specific filesystem path, resolves symbols,
    /// and initializes the Steamworks API for the specified AppID.
    pub fn load_from_path(dll_path: &Path, app_id: u32) -> Result<Self, SteamError> {
        // Set environment variables before initialization
        std::env::set_var("SteamAppId", app_id.to_string());
        std::env::set_var("SteamGameId", app_id.to_string());

        let wide_path: Vec<u16> = OsStr::new(dll_path.as_os_str())
            .encode_wide()
            .chain(Some(0))
            .collect();

        let h_module = unsafe { LoadLibraryW(wide_path.as_ptr()) };
        if h_module.is_null() {
            let code = unsafe { GetLastError() };
            return Err(SteamError::LoadLibraryFailed {
                path: dll_path.to_path_buf(),
                code,
            });
        }

        // Helper macro to resolve symbol or free library on failure
        macro_rules! resolve_sym {
            ($sym_name:expr, $fn_type:ty) => {{
                let sym_cstr = concat!($sym_name, "\0");
                let proc_addr = unsafe { GetProcAddress(h_module, sym_cstr.as_ptr()) };
                if proc_addr.is_null() {
                    unsafe { FreeLibrary(h_module) };
                    return Err(SteamError::SymbolNotFound($sym_name.to_string()));
                }
                unsafe { std::mem::transmute::<FARPROC, $fn_type>(proc_addr) }
            }};
        }

        let fn_init: SteamAPIInitFn = resolve_sym!("SteamAPI_Init", SteamAPIInitFn);
        let fn_shutdown: SteamAPIShutdownFn = resolve_sym!("SteamAPI_Shutdown", SteamAPIShutdownFn);
        let fn_run_callbacks: SteamAPIRunCallbacksFn =
            resolve_sym!("SteamAPI_RunCallbacks", SteamAPIRunCallbacksFn);
        let fn_is_steam_running: SteamAPIIsSteamRunningFn =
            resolve_sym!("SteamAPI_IsSteamRunning", SteamAPIIsSteamRunningFn);

        // Optional symbol
        let fn_restart_app: Option<SteamAPIRestartAppIfNecessaryFn> = {
            let proc_addr = unsafe {
                GetProcAddress(h_module, c"SteamAPI_RestartAppIfNecessary".as_ptr() as _)
            };
            if proc_addr.is_null() {
                None
            } else {
                Some(unsafe {
                    std::mem::transmute::<FARPROC, SteamAPIRestartAppIfNecessaryFn>(proc_addr)
                })
            }
        };

        // Check if Steam client process is running
        if !unsafe { fn_is_steam_running() } {
            unsafe { FreeLibrary(h_module) };
            return Err(SteamError::SteamNotRunning);
        }

        // Initialize Steamworks runtime
        let init_ok = unsafe { fn_init() };
        if !init_ok {
            unsafe { FreeLibrary(h_module) };
            return Err(SteamError::InitFailed(app_id));
        }

        Ok(Self {
            module: h_module,
            app_id,
            initialized: true,
            _fn_init: fn_init,
            fn_shutdown,
            fn_run_callbacks,
            fn_is_steam_running,
            fn_restart_app,
        })
    }

    /// Searches for `steam_api64.dll` across multiple potential locations:
    /// 1. `STEAM_API64_PATH` environment variable override
    /// 2. Current working directory
    /// 3. Application binary directory
    /// 4. Killing Floor system directory
    /// 5. Common Steam library installations
    pub fn find_steam_api64_dll() -> Option<PathBuf> {
        // 1. Environment variable override
        if let Ok(env_path) = std::env::var("STEAM_API64_PATH") {
            let p = PathBuf::from(env_path);
            if p.is_file() {
                return Some(p);
            }
        }

        // 2. Current working directory
        let cwd_dll = PathBuf::from("steam_api64.dll");
        if cwd_dll.is_file() {
            return Some(cwd_dll);
        }

        // 3. Application executable directory
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let exe_dll = parent.join("steam_api64.dll");
                if exe_dll.is_file() {
                    return Some(exe_dll);
                }
            }
        }

        // 4. Killing Floor System folder
        let kf_sys = PathBuf::from(r"E:\SteamLibrary\steamapps\common\KillingFloor\System\steam_api64.dll");
        if kf_sys.is_file() {
            return Some(kf_sys);
        }

        // 5. Shared common libraries in SteamLibrary
        let common_search = [
            r"E:\SteamLibrary\steamapps\common\Gunfire Reborn\Gunfire Reborn_Data\Plugins\steam_api64.dll",
            r"C:\Program Files (x86)\Steam\steamapps\common\KillingFloor\System\steam_api64.dll",
        ];
        for candidate in common_search {
            let p = PathBuf::from(candidate);
            if p.is_file() {
                return Some(p);
            }
        }

        None
    }

    /// Dispatches pending Steamworks callbacks.
    /// Should be invoked regularly in the main engine / supervisor frame loop.
    pub fn run_callbacks(&self) {
        if self.initialized {
            unsafe {
                (self.fn_run_callbacks)();
            }
        }
    }

    /// Returns `true` if the Steam client is active and connected.
    pub fn is_connected(&self) -> bool {
        self.initialized && unsafe { (self.fn_is_steam_running)() }
    }

    /// Returns `true` if the Steam client process is detected running.
    pub fn is_steam_running(&self) -> bool {
        unsafe { (self.fn_is_steam_running)() }
    }

    /// Returns the target Steam AppID initialized by this client.
    pub fn app_id(&self) -> u32 {
        self.app_id
    }

    /// Checks if the application was started outside of Steam and restarts via Steam if necessary.
    pub fn restart_app_if_necessary(&self, app_id: u32) -> bool {
        if let Some(restart_fn) = self.fn_restart_app {
            unsafe { restart_fn(app_id) }
        } else {
            false
        }
    }

    /// Explicitly shuts down the Steamworks API session.
    pub fn shutdown(&mut self) {
        if self.initialized {
            unsafe {
                (self.fn_shutdown)();
            }
            self.initialized = false;
        }
    }
}

impl Drop for Steam64Client {
    fn drop(&mut self) {
        self.shutdown();
        if !self.module.is_null() {
            unsafe {
                FreeLibrary(self.module);
            }
            self.module = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steam_error_formatting() {
        let err1 = SteamError::DllNotFound;
        assert_eq!(err1.to_string(), "steam_api64.dll was not found in any search path");

        let err2 = SteamError::LoadLibraryFailed {
            path: PathBuf::from("dummy.dll"),
            code: 126,
        };
        assert!(err2.to_string().contains("126"));

        let err3 = SteamError::SymbolNotFound("SteamAPI_Init".to_string());
        assert!(err3.to_string().contains("SteamAPI_Init"));

        let err4 = SteamError::SteamNotRunning;
        assert_eq!(err4.to_string(), "Steam client is not currently running");

        let err5 = SteamError::InitFailed(1250);
        assert!(err5.to_string().contains("1250"));
    }

    #[test]
    fn test_load_nonexistent_dll() {
        let bad_path = Path::new("non_existent_steamworks_dll_path_12345.dll");
        let result = Steam64Client::load_from_path(bad_path, 1250);
        assert!(result.is_err());
        match result.unwrap_err() {
            SteamError::LoadLibraryFailed { code, .. } => {
                assert!(code > 0, "Win32 error code should be non-zero");
            }
            other => panic!("Expected LoadLibraryFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_env_vars_set() {
        let _ = Steam64Client::init(1250);
        let app_id_env = std::env::var("SteamAppId");
        assert!(app_id_env.is_ok(), "SteamAppId environment variable should be set");
        let game_id_env = std::env::var("SteamGameId");
        assert!(game_id_env.is_ok(), "SteamGameId environment variable should be set");
    }

    #[test]
    fn test_find_steam_api64_dll_or_mock() {
        let discovered = Steam64Client::find_steam_api64_dll();
        if let Some(dll_path) = discovered {
            println!("[Test] Found candidate steam_api64.dll at: {}", dll_path.display());
            assert!(dll_path.exists());
        }
    }
}
