//! sw1tch: a tiny Valorant / Riot Client account switcher.
//!
//! Same approach as TcNo Account Switcher's Riot Games platform: close the Riot
//! processes, swap the Riot Client's saved-login files for the chosen account's
//! copy, and start the client again.

mod store;
mod sys;

use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use store::Store;

const HELP: &str = "\
sw1tch - lightweight Valorant account switcher

USAGE:
  sw1tch                    interactive menu
  sw1tch list               list saved accounts (* = active)
  sw1tch save <name>        save the account signed in right now as <name>
  sw1tch switch <name>      switch to <name> and start Valorant
  sw1tch new                sign out so you can log in to another account
  sw1tch rename <old> <new> rename a saved account
  sw1tch remove <name>      delete a saved account
  sw1tch launch             start Valorant

OPTIONS:
  --client      open the Riot Client only, don't start Valorant
  --no-launch   don't start anything after save/switch/new

Sign in with 'Stay signed in' ticked before saving an account.";

#[derive(Clone, Copy)]
struct Opts {
    launch: bool,
    valorant: bool,
}

fn main() -> ExitCode {
    let mut opts = Opts { launch: true, valorant: true };
    let mut args = Vec::new();
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--client" => opts.valorant = false,
            "--no-launch" => opts.launch = false,
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            _ => args.push(a),
        }
    }
    let store = match open_store() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let result = match args.first().map(String::as_str) {
        None => return menu(&store, opts),
        Some(cmd) => run(&store, opts, cmd, &args[1..]),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

fn fail(e: String) -> ExitCode {
    eprintln!("error: {e}");
    ExitCode::FAILURE
}

fn open_store() -> Result<Store, String> {
    Ok(Store { riot: sys::riot_data_dir()?, root: sys::store_dir()? })
}

fn run(s: &Store, opts: Opts, cmd: &str, rest: &[String]) -> Result<(), String> {
    // Multi-word names work with or without quotes.
    let name = rest.join(" ");
    let need = |what: &str| -> Result<String, String> {
        if name.is_empty() {
            Err(format!("usage: sw1tch {cmd} <{what}>"))
        } else {
            Ok(name.clone())
        }
    };
    match cmd {
        "list" | "ls" => {
            list(s);
            Ok(())
        }
        "current" => {
            println!("{}", s.current().unwrap_or_else(|| "(none)".into()));
            Ok(())
        }
        "save" => save(s, opts, &need("name")?),
        "switch" | "use" | "sw" => switch(s, opts, &need("name")?),
        "new" | "add" => new(s, opts),
        "remove" | "rm" | "delete" => {
            let n = resolve(s, &need("name")?)?;
            s.remove(&n)?;
            println!("Removed '{n}'.");
            Ok(())
        }
        "rename" | "mv" => {
            let [from, to] = rest else {
                return Err("usage: sw1tch rename <old> <new> (quote names with spaces)".into());
            };
            let from = resolve(s, from)?;
            s.rename(&from, to)?;
            println!("Renamed '{from}' to '{to}'.");
            Ok(())
        }
        "launch" | "start" => sys::launch(opts.valorant),
        _ => Err(format!("unknown command '{cmd}'. Run 'sw1tch help'.")),
    }
}

fn resolve(s: &Store, name: &str) -> Result<String, String> {
    s.resolve(name).ok_or_else(|| format!("no saved account named '{name}'"))
}

fn list(s: &Store) {
    let accounts = s.accounts();
    if accounts.is_empty() {
        println!("No saved accounts. Sign in to the Riot Client, then run: sw1tch save <name>");
        return;
    }
    let cur = s.current();
    for (i, a) in accounts.iter().enumerate() {
        let mark = if cur.as_ref() == Some(a) { "*" } else { " " };
        println!("{mark} {}) {a}", i + 1);
    }
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

fn save(s: &Store, opts: Opts, name: &str) -> Result<(), String> {
    store::validate_name(name)?;
    // Reuse an existing entry's exact spelling so re-saving updates it.
    let name = s.resolve(name).unwrap_or_else(|| name.to_string());
    let was_running = sys::close_riot()?;
    let existed = s.exists(&name);
    s.save(&name)?;
    println!("{} '{name}'.", if existed { "Updated" } else { "Saved" });
    if was_running {
        maybe_launch(opts)?;
    }
    Ok(())
}

fn switch(s: &Store, opts: Opts, name: &str) -> Result<(), String> {
    let name = resolve(s, name)?;
    if s.current().as_deref() == Some(name.as_str()) {
        println!("'{name}' is already active.");
        return maybe_launch(opts);
    }
    sys::close_riot()?;
    refresh_current(s, Some(&name))?;
    s.restore(&name)?;
    println!("Switched to '{name}'.");
    maybe_launch(opts)
}

fn new(s: &Store, opts: Opts) -> Result<(), String> {
    sys::close_riot()?;
    refresh_current(s, None)?;
    s.clear_live()?;
    println!("Signed out. Log in to the other account (tick 'Stay signed in'), then run: sw1tch save <name>");
    maybe_launch(opts)
}

fn prompt(msg: &str) -> Option<String> {
    print!("{msg}");
    io::stdout().flush().ok();
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

fn menu(s: &Store, opts: Opts) -> ExitCode {
    loop {
        println!("\nSW1TCH - Valorant account switcher\n");
        list(s);
        println!("\n [number] switch   s) save current   n) add new   r) remove   q) quit");
        let Some(choice) = prompt("> ") else { return ExitCode::SUCCESS };
        let result = match choice.to_lowercase().as_str() {
            "q" | "quit" | "exit" => return ExitCode::SUCCESS,
            "" => continue,
            "s" => match prompt("Name for the signed-in account: ") {
                Some(n) if !n.is_empty() => save(s, opts, &n),
                _ => continue,
            },
            "n" => new(s, opts),
            "r" => match prompt("Number or name to remove: ").and_then(|n| pick(s, &n)) {
                Some(n) => s.remove(&n).map(|_| println!("Removed '{n}'.")),
                None => Err("no such account".into()),
            },
            c => match pick(s, c) {
                Some(n) => switch(s, opts, &n),
                None => Err(format!("unknown choice '{c}'")),
            },
        };
        if let Err(e) = result {
            eprintln!("error: {e}");
        }
    }
}

/// Accept either a list number or an account name.
fn pick(s: &Store, input: &str) -> Option<String> {
    match input.parse::<usize>() {
        Ok(i) if i >= 1 => s.accounts().get(i - 1).cloned(),
        _ => s.resolve(input),
    }
}
