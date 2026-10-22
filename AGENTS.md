# AGENTS.md

Guidance for AI coding agents working in this repository.

## Toolchain

Rust is provided by the Nix flake only; nothing is installed globally. Run cargo through the dev shell (`nix develop --command cargo …`), never `nix-shell -p`. Flakes only see git-tracked files, so `git add` new files before running any `nix` command or they will be missing.

```sh
nix develop --command cargo build
nix develop --command cargo test                          # whole workspace
nix develop --command cargo test -p stravacz-core draft::   # one crate / test filter
nix develop --command cargo test -p stravacz-api --test client submit_stages_saves_and_verifies
nix develop --command cargo clippy --all-targets --examples   # must be warning-free
nix develop --command cargo fmt --all
nix flake check                                           # what CI runs: build + tests, clippy (-D warnings), fmt
```

Workspace lints (root `Cargo.toml`) enable `clippy::pedantic` and forbid `unsafe`; keep clippy at zero warnings.

## Running without a real account

`stravacz_api::mock` (feature `mock`) is a stateful fake Strava.cz (accepts any login, remembers orders in memory); the `mock_server` example serves it on `127.0.0.1:8555`. Point either frontend at it with `STRAVA_BASE_URL`. Use a throwaway `XDG_CONFIG_HOME`/`XDG_DATA_HOME` and `STRAVA_PASSWORD` (CLI) or untick "Remember password" (TUI) so the user's real config and system keyring are not touched:

```sh
cargo run -p stravacz-api --example mock_server
XDG_CONFIG_HOME=$(mktemp -d) STRAVA_BASE_URL=http://127.0.0.1:8555 STRAVA_PASSWORD=x cargo run -p stravacz-cli -- menu
STRAVA_BASE_URL=http://127.0.0.1:8555 cargo run -p stravacz-tui
```

The TUI can be driven and inspected headlessly with `tmux new-session -d` + `send-keys` + `capture-pane`.

Tests use the same mock: the CLI tests (`crates/stravacz-cli/tests/cli.rs`) run the binary against `mock::start()`, and the TUI tests build an `App` with `App::with` and feed it keys and `Msg`s, no terminal needed. `mock::expire_session` simulates an expired session.

## Architecture

Layered workspace; frontends must not talk HTTP or touch config/keyring directly:

- **`stravacz-api`** — async client for the *undocumented* Strava.cz JSON API (reverse engineered from the web app and https://github.com/jsem-nerad/strava-cz-python). `raw.rs` is the only place that knows the wire format; `model.rs` holds the public domain types (`Menu`, `Day`, `Meal`, `Money` in exact hundredths, …).
- **`stravacz-core`** — frontend-agnostic logic: `OrderDraft` (pending changes + canteen rules), `ConfigStore` (TOML at `~/.config/stravacz/config.toml`), `secrets` (keyring, service `strava-cz`, user `username@canteen`), `remember_login`/`forget_login`, and `wording` (user-facing text for API states, shared so both frontends say the same thing). Re-exports the API as `stravacz_core::api`; new frontends (e.g. a GUI) depend only on this crate.
- **`stravacz-tui`** (bin `stravacz-tui`) and **`stravacz-cli`** (bin `stravacz`) — the two frontends. They share the remembered account.

### API quirks (stravacz-api)

- Every call is a POST to `/api/<endpoint>` with a JSON body sent as `text/plain`. Errors come as HTTP 555 with `{"state":"error","number":N,"message":…}`; code 20 → `Error::Authentication`, 35 → `Error::InsufficientBalance`.
- Field types vary between canteens (`"1"` vs `1`, `null` vs `""`), so raw structs read everything leniently as text and convert afterwards. Bad rows are skipped with a `tracing::warn!`, never fatal.
- The menu (`objednavky`) is `table0…tableN`, one per day. In `omezeniObj`, `obj`/`zm` decide whether a meal can be ordered/cancelled; `den` describes the whole day only and must never be used for meal orderability. Rows of no-service days and unpublished slots (name == slot label, e.g. `Oběd 2`) are dropped, keeping the day.
- The menu request sends the session URL as `s5url`; write endpoints send it as `url`.
- Ordering is transactional: `Session::submit` stages each change with `pridejJidloS5` (cancellations first), commits with `saveOrders`, calls `nactiVlastnostiPA` to discard staged changes on failure, then refetches the menu and reports changes that did not apply in `Submission::rejected`. Once `saveOrders` succeeds, submit never fails: if the refetch fails, `Submission::menu` is `None` (unverified).

### Ordering rules (stravacz-core `OrderDraft`)

Both frontends edit orders only through `OrderDraft`: meals past their deadline cannot change, and ordering a main course swaps out the other main ordered that day. After loading a new menu, call `rebase` to drop changes that were applied or became impossible.

### TUI structure

Screens (`screens/login.rs`, `screens/menu.rs`) handle keys and return an `Action`. `app.rs` performs actions; network work runs in `tokio::spawn`ed tasks that report back over an mpsc channel as `Msg`, so the UI never blocks. Only one task runs at a time (`App::busy`); logout and quitting wait for it. When the session expires, the pending `OrderDraft` is kept (`Resume`) and restored once the same account logs in again. Keyring calls are blocking and go through `spawn_blocking`. Colors live in `theme.rs` (Catppuccin Mocha).

### CLI structure

clap derive in `cli.rs`; one module per command in `commands/`. Password lookup order: `STRAVA_PASSWORD` → keyring → interactive prompt (fails cleanly without a TTY). Every command supports `--json`. Styled output uses `anstream`/`anstyle` (stripped automatically when not a terminal). Long output is built as a string and written with `commands::print`, and `main` treats a broken pipe as success so `stravacz menu | head` works.
