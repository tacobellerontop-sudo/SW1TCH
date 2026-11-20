//! sw1tch-cli: command-line front end for the sw1tch Valorant account switcher.
//! The GUI (`sw1tch.exe`) and this CLI share the same core and saved accounts.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use sw1tch::ops::{self, Opts};
use sw1tch::store::Store;
use sw1tch::sys;

const HELP: &str = "\
sw1tch-cli - lightweight Valorant account switcher

USAGE:
  sw1tch-cli                    interactive menu
  sw1tch-cli list               list saved accounts (* = active)
  sw1tch-cli save <name>        save the account signed in right now as <name>
  sw1tch-cli switch <name>      switch to <name> and start Valorant
  sw1tch-cli new                sign out so you can log in to another account
  sw1tch-cli rename <old> <new> rename a saved account
  sw1tch-cli remove <name>      delete a saved account
  sw1tch-cli launch             start Valorant

OPTIONS:
  --client      open the Riot Client only, don't start Valorant
  --no-launch   don't start anything after save/switch/new

Sign in with 'Stay signed in' ticked before saving an account.";

fn main() -> ExitCode {
    let mut opts = Opts::default();
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
    let store = match ops::open_store() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let result = match args.first().map(String::as_str) {
        None => return menu(&store, opts),
        Some(cmd) => run(&store, opts, cmd, &args[1..]),
    };
    match result {
        Ok(msg) => {
            if !msg.is_empty() {
                println!("{msg}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn fail(e: String) -> ExitCode {
    eprintln!("error: {e}");
    ExitCode::FAILURE
}

fn run(s: &Store, opts: Opts, cmd: &str, rest: &[String]) -> Result<String, String> {
    // Multi-word names work with or without quotes.
    let name = rest.join(" ");
    let need = |what: &str| -> Result<String, String> {
        if name.is_empty() {
            Err(format!("usage: sw1tch-cli {cmd} <{what}>"))
        } else {
            Ok(name.clone())
        }
    };
    match cmd {
        "list" | "ls" => {
            list(s);
            Ok(String::new())
        }
        "current" => Ok(s.current().unwrap_or_else(|| "(none)".into())),
        "save" => ops::save(s, opts, &need("name")?),
        "switch" | "use" | "sw" => ops::switch(s, opts, &need("name")?),
        "new" | "add" => ops::new(s, opts),
        "remove" | "rm" | "delete" => ops::remove(s, &need("name")?),
        "rename" | "mv" => {
            let [from, to] = rest else {
                return Err("usage: sw1tch-cli rename <old> <new> (quote names with spaces)".into());
            };
            ops::rename(s, from, to)
        }
        "launch" | "start" => sys::launch(opts.valorant).map(|_| String::new()),
        _ => Err(format!("unknown command '{cmd}'. Run 'sw1tch-cli help'.")),
    }
}

fn list(s: &Store) {
    let accounts = s.accounts();
    if accounts.is_empty() {
        println!("No saved accounts. Sign in to the Riot Client, then run: sw1tch-cli save <name>");
        return;
    }
    let cur = s.current();
    for (i, a) in accounts.iter().enumerate() {
        let mark = if cur.as_ref() == Some(a) { "*" } else { " " };
        println!("{mark} {}) {a}", i + 1);
    }
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
                Some(n) if !n.is_empty() => ops::save(s, opts, &n),
                _ => continue,
            },
            "n" => ops::new(s, opts),
            "r" => match prompt("Number or name to remove: ").and_then(|n| pick(s, &n)) {
                Some(n) => ops::remove(s, &n),
                None => Err("no such account".into()),
            },
            c => match pick(s, c) {
                Some(n) => ops::switch(s, opts, &n),
                None => Err(format!("unknown choice '{c}'")),
            },
        };
        match result {
            Ok(msg) => println!("{msg}"),
            Err(e) => eprintln!("error: {e}"),
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
