# SW1TCH

A tiny account switcher for **Valorant** (and anything else on the Riot Client), written in Rust.

It works the same way as [TcNo Account Switcher](https://github.com/TCNOco/TcNo-Acc-Switcher)'s Riot Games support. It closes the Riot processes, swaps the Riot Client's saved login files for the chosen account's copy, and starts the client again. It does this in a single small `.exe` with no installer, no background process, no GUI framework and no dependencies beyond the Rust standard library.

## Usage

Download `sw1tch.exe` from the latest **build** workflow run (artifact `sw1tch-windows-x64`), or [build it yourself](#building). Put it anywhere.

### First time: save each account

1. Open the Riot Client and sign in with **Stay signed in** ticked.
2. Run `sw1tch save main` (any name works).
3. Run `sw1tch new`. This signs the Riot Client out and reopens it.
4. Sign in to your next account (again with **Stay signed in**), then run `sw1tch save smurf`.

Repeat steps 3 and 4 for every account.

### Switching

```
sw1tch switch smurf
```

This closes Riot and Valorant, restores `smurf`'s login and starts Valorant already signed in.

### Commands

| Command | What it does |
| --- | --- |
| `sw1tch` | Interactive menu. Type a number to switch. Handy for double-clicking the exe. |
| `sw1tch list` | Lists saved accounts. `*` marks the active one. |
| `sw1tch save <name>` | Saves the account that is signed in now. Saving an existing name updates it. |
| `sw1tch switch <name>` | Switches to a saved account (names are case-insensitive) and starts Valorant. |
| `sw1tch new` | Signs the Riot Client out so you can add another account. |
| `sw1tch rename <old> <new>` | Renames a saved account. Quote names that contain spaces. |
| `sw1tch remove <name>` | Deletes a saved account. |
| `sw1tch current` | Prints the active account. |
| `sw1tch launch` | Starts Valorant. |

| Option | Effect |
| --- | --- |
| `--client` | Open the Riot Client only, without starting Valorant. Works with `switch`, `new`, `save` and `launch`. |
| `--no-launch` | Don't start anything afterwards. |

Tip: make a desktop shortcut per account with a target like `C:\Tools\sw1tch.exe switch smurf` for one-click switching.

## How it works

The same model as TcNo's Riot Games platform definition (`Platforms.json`, `"Riot Games"` entry):

| | |
| --- | --- |
| **Processes closed** | `VALORANT-Win64-Shipping.exe`, `VALORANT.exe`, `LeagueClient.exe`, `LoR.exe`, `RiotClientUxRender.exe`, `RiotClientUx.exe`, `RiotClientServices.exe`, `RiotClientCrashHandler.exe` |
| **Login files** (under `%LocalAppData%\Riot Games\Riot Client\`) | `Data\` (holds `RiotGamesPrivateSettings.yaml`, `Cookies`, `Sessions`), `Config\RiotClientSettings.yaml` |
| **Account ID** | A small `Sw1tch-ID.instance` file is written into the Riot Client folder on save (TcNo's `CREATE_ID_FILE` method). It is saved and restored with each account, so the switcher can tell which account is live. |
| **Launch** | `RiotClientServices.exe --launch-product=valorant --launch-patchline=live`. The client path comes from `%ProgramData%\Riot Games\RiotClientInstalls.json`, falling back to `C:\Riot Games\Riot Client\RiotClientServices.exe`. |

On a switch it:

1. Closes every Riot process and waits for them to exit.
2. Re-saves the currently active account (if it is a saved one), so its refreshed tokens are kept. TcNo does the same.
3. Deletes the live login files, then copies the target account's files back.
4. Starts the Riot Client or Valorant.

Saving a **new** name always gives it a new ID, so saving while the wrong account is signed in can never overwrite a different saved account. Copies and deletes retry for a few seconds if Riot still holds a file lock, which was the main cause of failed Riot switches in TcNo.

Accounts are stored in `%AppData%\sw1tch\accounts\<name>\`. For a portable setup, create a folder named `sw1tch-data` next to `sw1tch.exe` and accounts are kept there instead.

### Environment overrides

| Variable | Default |
| --- | --- |
| `SW1TCH_HOME` | `%AppData%\sw1tch` |
| `SW1TCH_RIOT_DATA` | `%LocalAppData%\Riot Games\Riot Client` |
| `SW1TCH_RIOT_CLIENT` | Read from `RiotClientInstalls.json` |

## Notes

- The saved folders contain your Riot session tokens. Anyone who copies them can sign in as you, so treat `%AppData%\sw1tch` like a password.
- Riot sessions expire eventually (and immediately if you change your password or sign out everywhere). If a switch lands on the login screen, sign in again and `sw1tch save <name>` to refresh that account.
- If Riot runs as administrator, sw1tch needs to as well to close it.
- Vanguard (`vgc`) is not touched.

## Building

```
cargo build --release
```

The release profile is tuned for size (`opt-level = "z"`, LTO, `panic = "abort"`, stripped). Windows API calls (process enumeration and termination) are declared directly against `kernel32`, so there are no crates to download.

Cross-compiling from Linux works with the MinGW target (`.cargo/config.toml` sets the linker):

```
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

Tests: `cargo test` runs the snapshot/restore logic on any OS. `tests/smoke.ps1` runs an end-to-end switch on Windows against a fake Riot Client and runs in CI.

## Credits

The switching approach (process list, login file list and ID file method) follows [TcNo Account Switcher](https://github.com/TCNOco/TcNo-Acc-Switcher) by TechNobo. No TcNo code is included. This project is not affiliated with Riot Games or TcNo.
