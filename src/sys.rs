//! Process handling and launching. Windows uses a few raw kernel32 calls so the
//! binary needs no crates; other platforms get stubs so the core logic can be
//! built and tested anywhere.

use std::path::PathBuf;

#[cfg_attr(not(windows), allow(dead_code))]
/// Processes closed before touching login files (TcNo's Riot `ExesToEnd`,
/// plus the Valorant game process and the crash handler).
pub const RIOT_EXES: &[&str] = &[
    "VALORANT-Win64-Shipping.exe",
    "VALORANT.exe",
    "LeagueClient.exe",
    "LoR.exe",
    "RiotClientUxRender.exe",
    "RiotClientUx.exe",
    "RiotClientServices.exe",
    "RiotClientCrashHandler.exe",
];

const DEFAULT_CLIENT: &str = r"C:\Riot Games\Riot Client\RiotClientServices.exe";

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// `%LocalAppData%\Riot Games\Riot Client` (override: `SW1TCH_RIOT_DATA`).
pub fn riot_data_dir() -> Result<PathBuf, String> {
    if let Some(p) = env_path("SW1TCH_RIOT_DATA") {
        return Ok(p);
    }
    env_path("LOCALAPPDATA")
        .map(|p| p.join("Riot Games").join("Riot Client"))
        .ok_or_else(|| "LOCALAPPDATA is not set".into())
}

/// Where accounts are stored (override: `SW1TCH_HOME`). Defaults to
/// `%AppData%\sw1tch`, or a `sw1tch-data` folder next to the exe if that
/// folder exists (portable mode).
pub fn store_dir() -> Result<PathBuf, String> {
    if let Some(p) = env_path("SW1TCH_HOME") {
        return Ok(p);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("sw1tch-data"))) {
        if dir.is_dir() {
            return Ok(dir);
        }
    }
    env_path("APPDATA")
        .map(|p| p.join("sw1tch"))
        .ok_or_else(|| "APPDATA is not set".into())
}

/// Path to `RiotClientServices.exe`: `SW1TCH_RIOT_CLIENT`, then the `rc_default`
/// entry in `%ProgramData%\Riot Games\RiotClientInstalls.json`, then the default.
pub fn riot_client_exe() -> PathBuf {
    if let Some(p) = env_path("SW1TCH_RIOT_CLIENT") {
        return p;
    }
    let installs = env_path("ProgramData")
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Riot Games")
        .join("RiotClientInstalls.json");
    if let Ok(json) = std::fs::read_to_string(installs) {
        for key in ["rc_default", "rc_live"] {
            if let Some(p) = json_string(&json, key).map(PathBuf::from).filter(|p| p.is_file()) {
                return p;
            }
        }
    }
    PathBuf::from(DEFAULT_CLIENT)
}

/// Minimal `"key": "value"` lookup; enough for RiotClientInstalls.json.
fn json_string(json: &str, key: &str) -> Option<String> {
    let at = json.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = json[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].replace("\\\\", "\\").replace("\\/", "/"))
}

/// Start the Riot Client, optionally straight into Valorant.
pub fn launch(valorant: bool) -> Result<(), String> {
    let exe = riot_client_exe();
    let mut cmd = std::process::Command::new(&exe);
    if valorant {
        cmd.args(["--launch-product=valorant", "--launch-patchline=live"]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(DETACHED_PROCESS);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start {}: {e} (set SW1TCH_RIOT_CLIENT to its path)", exe.display()))
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const TH32CS_SNAPPROCESS: u32 = 0x2;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const SYNCHRONIZE: u32 = 0x0010_0000;

    #[repr(C)]
    struct ProcessEntry32W {
        size: u32,
        usage: u32,
        pid: u32,
        heap_id: usize,
        module_id: u32,
        threads: u32,
        parent_pid: u32,
        pri_class_base: i32,
        flags: u32,
        exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
        fn Process32FirstW(snap: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snap: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn TerminateProcess(proc_: Handle, code: u32) -> i32;
        fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
        fn CloseHandle(h: Handle) -> i32;
    }

    /// (pid, exe name) of every running process whose name is in `names`.
    fn find(names: &[&str]) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut e: ProcessEntry32W = std::mem::zeroed();
            e.size = std::mem::size_of::<ProcessEntry32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e);
            while ok != 0 {
                let len = e.exe_file.iter().position(|&c| c == 0).unwrap_or(e.exe_file.len());
                let name = String::from_utf16_lossy(&e.exe_file[..len]);
                if names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    out.push((e.pid, name));
                }
                ok = Process32NextW(snap, &mut e);
            }
            CloseHandle(snap);
        }
        out
    }

    pub fn kill(names: &[&str]) -> Result<bool, String> {
        let procs = find(names);
        for (pid, _) in &procs {
            unsafe {
                let h = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, *pid);
                if h.is_null() {
                    continue;
                }
                if TerminateProcess(h, 1) != 0 {
                    WaitForSingleObject(h, 5000);
                }
                CloseHandle(h);
            }
        }
        // Children may respawn while parents die; sweep a couple more times.
        for _ in 0..10 {
            if find(names).is_empty() {
                return Ok(!procs.is_empty());
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
            for (pid, _) in find(names) {
                unsafe {
                    let h = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
                    if !h.is_null() {
                        TerminateProcess(h, 1);
                        WaitForSingleObject(h, 2000);
                        CloseHandle(h);
                    }
                }
            }
        }
        let mut left: Vec<String> = find(names).into_iter().map(|(_, n)| n).collect();
        left.dedup();
        if left.is_empty() {
            Ok(!procs.is_empty())
        } else {
            Err(format!(
                "could not close {} (try running sw1tch as administrator)",
                left.join(", ")
            ))
        }
    }
}

/// Close all Riot processes and wait for them to exit. Returns whether any were running.
#[cfg(windows)]
pub fn close_riot() -> Result<bool, String> {
    let was = win::kill(RIOT_EXES)?;
    if was {
        // Give the file system a moment to release handles.
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(was)
}

#[cfg(not(windows))]
pub fn close_riot() -> Result<bool, String> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_installs_json() {
        let j = r#"{"associated_client":{},"rc_default":"C:/Riot Games/Riot Client/RiotClientServices.exe","rc_live":"x"}"#;
        assert_eq!(
            super::json_string(j, "rc_default").as_deref(),
            Some("C:/Riot Games/Riot Client/RiotClientServices.exe")
        );
        assert_eq!(super::json_string(j, "nope"), None);
    }
}
