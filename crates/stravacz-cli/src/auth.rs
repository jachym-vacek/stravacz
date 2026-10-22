//! Working out who to log in as, and logging in.

use std::io::{self, BufRead, IsTerminal, Write};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use stravacz_core::api::{Client, Credentials, Session};
use stravacz_core::{ConfigStore, SavedLogin, secrets};

use crate::cli::AccountArgs;
use crate::style;

/// Lets scripts log in without a keyring or a prompt.
const PASSWORD_VAR: &str = "STRAVA_PASSWORD";

pub fn client(args: &AccountArgs) -> Result<Client> {
    Ok(match &args.base_url {
        Some(url) => Client::with_base_url(url)?,
        None => Client::new()?,
    })
}

/// The account remembered by `stravacz login`.
pub fn saved_login(store: &ConfigStore) -> Result<Option<SavedLogin>> {
    Ok(store.load()?.login)
}

/// Log in as the account from the arguments, falling back to the remembered one.
///
/// The password comes from `STRAVA_PASSWORD`, the keyring, or a prompt.
pub async fn session(args: &AccountArgs) -> Result<Session> {
    let saved = saved_login(&ConfigStore::new()?)?;
    let pick = |given: &Option<String>, remembered: fn(&SavedLogin) -> &String| {
        given
            .clone()
            .or_else(|| saved.as_ref().map(|login| remembered(login).clone()))
            .context("no account; run `stravacz login` first or pass --canteen and --username")
    };
    let canteen = pick(&args.canteen, |login| &login.canteen)?;
    let username = pick(&args.username, |login| &login.username)?;

    let remembered = saved.filter(|login| {
        login.remember_password && login.canteen == canteen && login.username == username
    });
    let password = match env_password()? {
        Some(password) => password,
        None => match stored_password(remembered).await {
            Some(password) => password,
            None => prompt_password(&username)?,
        },
    };

    let credentials = Credentials::new(canteen, username, password);
    client(args)?
        .login(&credentials)
        .await
        .context("could not log in")
}

/// End the session, without letting a slow server hold up the exit.
pub async fn finish(session: Session) {
    let _ = tokio::time::timeout(Duration::from_secs(2), session.logout()).await;
}

async fn stored_password(login: Option<SavedLogin>) -> Option<String> {
    let login = login?;
    match tokio::task::spawn_blocking(move || secrets::load_password(&login)).await {
        Ok(Ok(password)) => password,
        Ok(Err(err)) => {
            let warn = style::WARN;
            anstream::eprintln!("{warn}warning:{warn:#} {err}");
            None
        }
        Err(_) => None,
    }
}

/// The password from `STRAVA_PASSWORD`, if set.
pub fn env_password() -> Result<Option<String>> {
    match std::env::var(PASSWORD_VAR) {
        Ok(password) if password.is_empty() => bail!("{PASSWORD_VAR} is set but empty"),
        Ok(password) => Ok(Some(password)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => bail!("{PASSWORD_VAR} is not valid UTF-8"),
    }
}

pub fn prompt_password(username: &str) -> Result<String> {
    if !io::stdin().is_terminal() {
        bail!("no password; run `stravacz login` or set {PASSWORD_VAR}");
    }
    let password = rpassword::prompt_password(format!("Password for {username}: "))?;
    if password.is_empty() {
        bail!("no password given");
    }
    Ok(password)
}

/// Ask for a value on the terminal, offering `default`.
pub fn prompt(label: &str, default: Option<&str>) -> Result<String> {
    if !io::stdin().is_terminal() {
        bail!("missing {}; pass it as an argument", label.to_lowercase());
    }
    let mut stderr = io::stderr().lock();
    match default {
        Some(default) => write!(stderr, "{label} [{default}]: ")?,
        None => write!(stderr, "{label}: ")?,
    }
    stderr.flush()?;

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    match (line.trim(), default) {
        ("", Some(default)) => Ok(default.to_owned()),
        ("", None) => bail!("no {} given", label.to_lowercase()),
        (value, _) => Ok(value.to_owned()),
    }
}
