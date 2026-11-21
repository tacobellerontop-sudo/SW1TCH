//! The user-facing actions (save, switch, add, ...) shared by the GUI and CLI.
//! Each returns a short message describing what happened.

use crate::store::{self, Store};
use crate::sys;

#[derive(Clone, Copy)]
pub struct Opts {
    /// Start something after the action.
    pub launch: bool,
    /// Start Valorant (true) or just the Riot Client (false).
    pub valorant: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { launch: true, valorant: true }
    }
}

pub fn open_store() -> Result<Store, String> {
    Ok(Store { riot: sys::riot_data_dir()?, root: sys::store_dir()? })
}

pub fn resolve(s: &Store, name: &str) -> Result<String, String> {
    s.resolve(name).ok_or_else(|| format!("no saved account named '{name}'"))
}

fn maybe_launch(opts: Opts) -> Result<(), String> {
    if opts.launch {
        sys::launch(opts.valorant)?;
    }
    Ok(())
}

/// Re-snapshot the active saved account so its tokens stay fresh (TcNo does
/// this before every swap). Riot must already be closed.
fn refresh_current(s: &Store, skip: Option<&str>) -> Result<(), String> {
    if let Some(cur) = s.current() {
        if Some(cur.as_str()) != skip && s.signed_in() {
            s.save(&cur)?;
        }
    }
    Ok(())
}

/// Save the signed-in account as `name`. Relaunches Riot if it was running.
pub fn save(s: &Store, opts: Opts, name: &str) -> Result<String, String> {
    store::validate_name(name)?;
    // Reuse an existing entry's exact spelling so re-saving updates it.
    let name = s.resolve(name).unwrap_or_else(|| name.to_string());
    let was_running = sys::close_riot()?;
    let existed = s.exists(&name);
    s.save(&name)?;
    if was_running {
        maybe_launch(opts)?;
    }
    Ok(format!("{} '{name}'.", if existed { "Updated" } else { "Saved" }))
}

pub fn switch(s: &Store, opts: Opts, name: &str) -> Result<String, String> {
    let name = resolve(s, name)?;
    if s.current().as_deref() == Some(name.as_str()) {
        maybe_launch(opts)?;
        return Ok(format!("'{name}' is already active."));
    }
    sys::close_riot()?;
    refresh_current(s, Some(&name))?;
    s.restore(&name)?;
    maybe_launch(opts)?;
    Ok(format!("Switched to '{name}'."))
}

/// Sign the Riot Client out so another account can be added.
pub fn new(s: &Store, opts: Opts) -> Result<String, String> {
    sys::close_riot()?;
    refresh_current(s, None)?;
    s.clear_live()?;
    maybe_launch(opts)?;
    Ok("Signed out. Log in to the other account (tick 'Stay signed in'), then save it.".into())
}

pub fn remove(s: &Store, name: &str) -> Result<String, String> {
    let name = resolve(s, name)?;
    s.remove(&name)?;
    Ok(format!("Removed '{name}'."))
}

pub fn rename(s: &Store, from: &str, to: &str) -> Result<String, String> {
    let from = resolve(s, from)?;
    s.rename(&from, to)?;
    Ok(format!("Renamed '{from}' to '{to}'."))
}
