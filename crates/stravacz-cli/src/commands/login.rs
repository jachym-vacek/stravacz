use std::process::ExitCode;

use anyhow::{Context, Result};
use serde_json::json;
use stravacz_core::api::Credentials;
use stravacz_core::{ConfigStore, SavedLogin, forget_login, remember_login};

use super::print_json;
use crate::auth;
use crate::cli::AccountArgs;
use crate::style::{DIM, OK, WARN};

pub async fn login(args: &AccountArgs, save_password: bool, json: bool) -> Result<ExitCode> {
    let store = ConfigStore::new()?;
    let saved = auth::saved_login(&store)?;

    let canteen = match &args.canteen {
        Some(canteen) => canteen.clone(),
        None => auth::prompt(
            "Canteen number",
            saved.as_ref().map(|login| login.canteen.as_str()),
        )?,
    };
    let username = match &args.username {
        Some(username) => username.clone(),
        None => auth::prompt(
            "Username",
            saved.as_ref().map(|login| login.username.as_str()),
        )?,
    };
    let password = match std::env::var("STRAVA_PASSWORD") {
        Ok(password) => password,
        Err(_) => auth::prompt_password(&username)?,
    };

    let credentials = Credentials::new(canteen, username, password);
    let session = auth::client(args)?
        .login(&credentials)
        .await
        .context("could not log in")?;

    let login = SavedLogin {
        canteen: credentials.canteen,
        username: credentials.username,
        remember_password: save_password,
    };
    let password = save_password.then_some(credentials.password);
    let saved =
        tokio::task::spawn_blocking(move || remember_login(&store, login, password.as_deref()))
            .await?;

    let account = session.account().clone();
    auth::finish(session).await;

    if json {
        return print_json(
            &json!({ "account": account, "password_saved": saved.is_ok() && save_password }),
        );
    }
    let canteen = account
        .canteen_name
        .as_deref()
        .unwrap_or(&account.canteen_number);
    anstream::println!(
        "{OK}✓{OK:#} Logged in as {} {DIM}({canteen}){DIM:#}",
        account.full_name
    );
    match saved {
        Ok(()) if save_password => {
            anstream::println!("  {DIM}Password saved in the system keyring.{DIM:#}");
        }
        Ok(()) => {}
        Err(err) => anstream::eprintln!("{WARN}warning:{WARN:#} {err}"),
    }
    Ok(ExitCode::SUCCESS)
}

pub fn logout(json: bool) -> Result<ExitCode> {
    let forgotten = forget_login(&ConfigStore::new()?)?;
    if json {
        return print_json(&json!({ "forgotten": forgotten.is_some() }));
    }
    match forgotten {
        Some(login) => anstream::println!(
            "{OK}✓{OK:#} Forgot {} {DIM}(canteen {}){DIM:#}",
            login.username,
            login.canteen
        ),
        None => anstream::println!("No account was remembered."),
    }
    Ok(ExitCode::SUCCESS)
}
