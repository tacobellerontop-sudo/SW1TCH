//! Snapshot / restore of the Riot Client login files.
//!
//! This mirrors the "basic platform" model used by TcNo Account Switcher for
//! Riot Games: a fixed list of login paths under the Riot Client's local data
//! folder is copied into a per-account folder on save, and on switch the live
//! paths are deleted and the saved copy is put back. A small ID file inside the
//! Riot Client folder tells us which saved account is currently live.
//!
//! Nothing here touches processes; callers close the Riot Client first.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

/// Paths (relative to `%LocalAppData%\Riot Games\Riot Client`) that make up a
/// signed-in session. `Data` holds `RiotGamesPrivateSettings.yaml` /
/// `RiotClientPrivateSettings.yaml`, `Cookies` and `Sessions`.
pub const LOGIN_ITEMS: &[&str] = &["Data", "Config/RiotClientSettings.yaml", ID_FILE];

/// Marker file we drop in the live Riot Client folder (TcNo's CREATE_ID_FILE).
pub const ID_FILE: &str = "Sw1tch-ID.instance";

/// Files whose presence means the Riot Client has a stored login.
const SESSION_FILES: &[&str] = &[
    "Data/RiotGamesPrivateSettings.yaml",
    "Data/RiotClientPrivateSettings.yaml",
];

pub struct Store {
    /// Live Riot Client data folder.
    pub riot: PathBuf,
    /// Where accounts are kept: `<root>/accounts/<name>/...`.
    pub root: PathBuf,
}

impl Store {
    pub fn accounts_dir(&self) -> PathBuf {
        self.root.join("accounts")
    }

    fn account_dir(&self, name: &str) -> PathBuf {
        self.accounts_dir().join(name)
    }

    /// Saved account names, sorted case-insensitively.
    pub fn accounts(&self) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(self.accounts_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.'))
            .collect();
        out.sort_by_key(|n| n.to_lowercase());
        out
    }

    pub fn exists(&self, name: &str) -> bool {
        self.account_dir(name).is_dir()
    }

    /// Look up a saved account by exact name, falling back to a case-insensitive match.
    pub fn resolve(&self, name: &str) -> Option<String> {
        let all = self.accounts();
        all.iter()
            .find(|a| *a == name)
            .or_else(|| all.iter().find(|a| a.eq_ignore_ascii_case(name)))
            .cloned()
    }

    fn account_id(&self, name: &str) -> Option<String> {
        read_id(&self.account_dir(name).join(ID_FILE))
    }

    /// Name of the saved account that is currently live, if any.
    pub fn current(&self) -> Option<String> {
        let live = read_id(&self.riot.join(ID_FILE))?;
        self.accounts()
            .into_iter()
            .find(|a| self.account_id(a).as_deref() == Some(live.as_str()))
    }

    pub fn signed_in(&self) -> bool {
        SESSION_FILES.iter().any(|f| self.riot.join(f).is_file())
    }

    /// Copy the live login into the account `name` (creating or replacing it).
    pub fn save(&self, name: &str) -> Result<(), String> {
        validate_name(name)?;
        if !self.signed_in() {
            return Err("the Riot Client has no stored login. Sign in with \
                        'Stay signed in' ticked, then save."
                .into());
        }
        // Re-saving an existing name keeps its ID; a new name always gets a
        // fresh one so it can never overwrite a different saved account.
        let id = self.account_id(name).unwrap_or_else(random_id);
        let id_path = self.riot.join(ID_FILE);
        retry(|| std::fs::write(&id_path, &id)).map_err(|e| ioerr(&id_path, e))?;

        let dir = self.account_dir(name);
        let tmp = self.accounts_dir().join(format!(".{name}.tmp"));
        let _ = remove(&tmp);
        for item in LOGIN_ITEMS {
            let src = self.riot.join(item);
            if src.exists() {
                copy(&src, &tmp.join(item)).map_err(|e| ioerr(&src, e))?;
            }
        }
        if dir.exists() {
            remove(&dir).map_err(|e| ioerr(&dir, e))?;
        }
        std::fs::rename(&tmp, &dir).map_err(|e| ioerr(&dir, e))
    }

    /// Delete every live login path (signs the Riot Client out).
    pub fn clear_live(&self) -> Result<(), String> {
        for item in LOGIN_ITEMS {
            let p = self.riot.join(item);
            remove(&p).map_err(|e| ioerr(&p, e))?;
        }
        Ok(())
    }

    /// Replace the live login with the saved account `name`.
    pub fn restore(&self, name: &str) -> Result<(), String> {
        let dir = self.account_dir(name);
        if !dir.is_dir() {
            return Err(format!("no saved account named '{name}'"));
        }
        self.clear_live()?;
        for item in LOGIN_ITEMS {
            let src = dir.join(item);
            if src.exists() {
                copy(&src, &self.riot.join(item)).map_err(|e| ioerr(&src, e))?;
            }
        }
        Ok(())
    }

    pub fn remove(&self, name: &str) -> Result<(), String> {
        let dir = self.account_dir(name);
        if !dir.is_dir() {
            return Err(format!("no saved account named '{name}'"));
        }
        remove(&dir).map_err(|e| ioerr(&dir, e))
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        validate_name(to)?;
        if !self.exists(from) {
            return Err(format!("no saved account named '{from}'"));
        }
        if self.exists(to) && !from.eq_ignore_ascii_case(to) {
            return Err(format!("an account named '{to}' already exists"));
        }
        std::fs::rename(self.account_dir(from), self.account_dir(to))
            .map_err(|e| ioerr(&self.account_dir(to), e))
    }
}

pub fn validate_name(name: &str) -> Result<(), String> {
    const BAD: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if name.trim().is_empty() || name != name.trim() {
        return Err("account name must not be empty or start/end with spaces".into());
    }
    if name.starts_with('.') || name.ends_with('.') || name.len() > 64 {
        return Err("account name must be under 64 characters and not start or end with '.'".into());
    }
    if name.chars().any(|c| BAD.contains(&c) || c.is_control()) {
        return Err("account name must not contain < > : \" / \\ | ? *".into());
    }
    Ok(())
}

fn read_id(p: &Path) -> Option<String> {
    let s = std::fs::read_to_string(p).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn random_id() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    format!("{:016x}", h.finish())
}

fn ioerr(p: &Path, e: io::Error) -> String {
    format!("{}: {e}", p.display())
}

/// Riot sometimes holds files for a moment after its processes exit, so file
/// operations get a few retries (TcNo's main Riot failure mode was exactly this).
fn retry<T>(mut f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut tries = 0;
    loop {
        match f() {
            Err(e) if tries < 15 && e.kind() != io::ErrorKind::NotFound => {
                tries += 1;
                sleep(Duration::from_millis(200));
            }
            r => return r,
        }
    }
}

fn remove(p: &Path) -> io::Result<()> {
    let r = match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => retry(|| std::fs::remove_dir_all(p)),
        Ok(_) => retry(|| std::fs::remove_file(p)),
        Err(e) => Err(e),
    };
    match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

fn copy(src: &Path, dst: &Path) -> io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        retry(|| std::fs::copy(src, dst)).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup(tag: &str) -> Store {
        let base = std::env::temp_dir().join(format!("sw1tch-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let s = Store { riot: base.join("riot"), root: base.join("store") };
        fs::create_dir_all(&s.riot).unwrap();
        s
    }

    fn login(s: &Store, who: &str) {
        let _ = fs::remove_dir_all(s.riot.join("Data"));
        fs::create_dir_all(s.riot.join("Data/Cookies")).unwrap();
        fs::create_dir_all(s.riot.join("Config")).unwrap();
        fs::write(s.riot.join("Data/RiotGamesPrivateSettings.yaml"), who).unwrap();
        fs::write(s.riot.join("Data/Cookies/c"), who).unwrap();
        fs::write(s.riot.join("Config/RiotClientSettings.yaml"), who).unwrap();
    }

    fn live(s: &Store) -> String {
        fs::read_to_string(s.riot.join("Data/RiotGamesPrivateSettings.yaml")).unwrap()
    }

    #[test]
    fn save_switch_roundtrip() {
        let s = setup("roundtrip");
        assert!(s.save("main").is_err(), "not signed in yet");

        login(&s, "A");
        s.save("main").unwrap();
        assert_eq!(s.current().as_deref(), Some("main"));

        s.clear_live().unwrap();
        assert!(!s.signed_in());
        assert_eq!(s.current(), None);
        login(&s, "B");
        fs::write(s.riot.join("Data/extra"), "junk").unwrap();
        s.save("smurf").unwrap();
        assert_eq!(s.accounts(), vec!["main", "smurf"]);
        assert_eq!(s.current().as_deref(), Some("smurf"));

        s.restore("main").unwrap();
        assert_eq!(live(&s), "A");
        assert!(!s.riot.join("Data/extra").exists(), "old session files are cleared");
        assert_eq!(fs::read_to_string(s.riot.join("Data/Cookies/c")).unwrap(), "A");
        assert_eq!(s.current().as_deref(), Some("main"));

        s.restore("smurf").unwrap();
        assert_eq!(live(&s), "B");
        assert_eq!(s.current().as_deref(), Some("smurf"));
    }

    #[test]
    fn new_name_never_clobbers_other_account() {
        let s = setup("clobber");
        login(&s, "A");
        s.save("main").unwrap();
        // Signed into B without clearing first, saved under a new name.
        login(&s, "B");
        s.save("alt").unwrap();
        s.restore("main").unwrap();
        assert_eq!(live(&s), "A");
        assert_eq!(s.current().as_deref(), Some("main"));
    }

    #[test]
    fn resave_rename_remove() {
        let s = setup("misc");
        login(&s, "A");
        s.save("main").unwrap();
        login(&s, "A2");
        s.save("main").unwrap();
        assert_eq!(s.accounts(), vec!["main"]);
        assert_eq!(s.current().as_deref(), Some("main"));
        s.rename("main", "Main2").unwrap();
        assert_eq!(s.current().as_deref(), Some("Main2"));
        assert_eq!(s.resolve("main2").as_deref(), Some("Main2"));
        s.remove("Main2").unwrap();
        assert!(s.accounts().is_empty());
    }

    #[test]
    fn names() {
        assert!(validate_name("Main #1").is_ok());
        for bad in ["", " x", "a/b", "a:b", ".x", "x."] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
    }
}
