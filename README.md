# stravacz

Terminal clients for [Strava.cz](https://app.strava.cz), the Czech school canteen
ordering system: a full-screen TUI (`stravacz-tui`) and a scriptable CLI (`stravacz`).
Browse the menu, order or cancel lunches, and see your balance without opening a
browser.

> [!IMPORTANT]
> **Unofficial project.** stravacz is an independent, community-made client. It is
> not affiliated with, endorsed by, or supported by VIS Plzeň, s.r.o., the company
> behind Strava.cz, nor with Strava, Inc. (the fitness app). "Strava.cz" is used
> only to say which service this program works with. Do not contact your canteen or
> VIS Plzeň about problems with stravacz; [open an issue](https://github.com/jachym-vacek/stravacz/issues)
> instead.
>
> stravacz talks to an undocumented API that can change or break at any time.
> Always check that your order went through, and use it at your own risk: the
> software comes with no warranty (see [License](#license)).

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
already has one swaps them. If the session expires, pending changes survive
logging back in (which happens automatically with a remembered password).

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
account through `STRAVA_CANTEEN`, `STRAVA_USERNAME` and `STRAVA_PASSWORD`. Keep in
mind that environment variables are readable by other processes of the same user
(e.g. via `/proc/<pid>/environ`), so prefer the keyring where there is one.
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

The toolchain comes from the Nix flake. Enter it with `nix develop` (or direnv),
or prefix single commands with `nix develop --command`:

```sh
nix develop --command cargo test
nix develop --command cargo clippy --all-targets --examples
nix flake check          # what CI runs: build, tests, clippy, docs, formatting, audit

# A fake Strava.cz to click through without a real account:
nix develop --command cargo run -p stravacz-api --example mock_server
STRAVA_BASE_URL=http://127.0.0.1:8555 nix develop --command cargo run -p stravacz-tui
STRAVA_BASE_URL=http://127.0.0.1:8555 nix develop --command cargo run -p stravacz-cli -- menu
```

The same fake canteen is available to tests as `stravacz_api::mock` (feature `mock`).

The API was reverse engineered from the web app, with help from
[strava-cz-python](https://github.com/jsem-nerad/strava-cz-python), only to make
this client interoperate with it. No Strava.cz code or data is included. Requests
identify themselves with the User-Agent `stravacz/<version> (unofficial client; +<repo URL>)`
and are only made when you ask for something: nothing polls in the background.

## License

[MIT](LICENSE)
