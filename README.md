# stravacz

Terminal clients for [Strava.cz](https://app.strava.cz), the Czech school canteen
ordering system: a full-screen TUI (`stravacz-tui`) and a scriptable CLI (`stravacz`).
Browse the menu, order or cancel lunches, and see your balance without opening a
browser.

## TUI

```sh
cargo run -p stravacz-tui    # or: nix run
```

Log in with your canteen number, username and password. With *Remember password*
checked, the password goes to the system keyring (Secret Service / Keychain /
Credential Manager) and you are logged in automatically next time; the canteen
number and username are stored in `~/.config/stravacz/config.toml`.

| Key                | Action                       |
| ------------------ | ---------------------------- |
| `←` `→` / `h` `l`  | previous / next day          |
| `↑` `↓` / `j` `k`  | previous / next meal         |
| `[` `]` / PgUp PgDn | previous / next week        |
| `t`, `g`, `G`      | today, first day, last day   |
| `space` / `enter`  | order or cancel the meal     |
| `s`                | save pending changes         |
| `u` / `esc`        | discard pending changes      |
| `r`                | reload the menu              |
| `L`                | log out                      |
| `?`                | help                         |
| `q` / `ctrl-c`     | quit                         |

Changes are staged locally first (shown in yellow) and sent together with `s`,
which mirrors how the web app works. Picking a different lunch on a day that
already has one swaps them.

Logs go to `~/.local/share/stravacz/stravacz.log`; set `STRAVACZ_LOG=debug`
for more detail.

## CLI

```console
$ stravacz login                 # asks for canteen, username, password; remembers them
$ stravacz menu                  # the next 5 days; also: stravacz menu tomorrow -n 10
$ stravacz menu --ordered        # just what you ordered
$ stravacz order 115 120         # meal ids from `stravacz menu`
$ stravacz cancel 115 --dry-run  # show what would change
$ stravacz balance
$ stravacz logout                # forget the account and its password
```

Every command takes `--json` for scripting. Without a keyring (cron, CI), pass the
account through `STRAVA_CANTEEN`, `STRAVA_USERNAME` and `STRAVA_PASSWORD`.
Shell completions: `stravacz completions zsh > _stravacz` (also bash, fish, …).

The CLI and the TUI share the remembered account.

## Workspace

| Crate           | Purpose                                                              |
| --------------- | -------------------------------------------------------------------- |
| `stravacz-api`  | Async client for the (undocumented) Strava.cz JSON API, typed models |
| `stravacz-core` | Frontend-agnostic logic: order drafts, config, keyring               |
| `stravacz-tui`  | The ratatui frontend (`stravacz-tui` binary)                         |
| `stravacz-cli`  | The command line frontend (`stravacz` binary)                        |

A different frontend (e.g. a GUI) only needs to depend on `stravacz-core`, which
re-exports `stravacz-api` as `stravacz_core::api`.

## Development

```sh
nix develop                                       # toolchain + rust-analyzer
cargo test
cargo clippy --all-targets

# A fake Strava.cz to click through without a real account:
cargo run -p stravacz-api --example mock_server
STRAVA_BASE_URL=http://127.0.0.1:8555 cargo run -p stravacz-tui
STRAVA_BASE_URL=http://127.0.0.1:8555 cargo run -p stravacz-cli -- menu
```

The API was reverse engineered from the web app, with help from
[strava-cz-python](https://github.com/jsem-nerad/strava-cz-python).
