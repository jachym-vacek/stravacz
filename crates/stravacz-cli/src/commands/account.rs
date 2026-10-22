use std::process::ExitCode;

use anyhow::Result;

use super::print_json;
use crate::auth;
use crate::cli::AccountArgs;
use crate::style::{DIM, HEADER, OK};

pub async fn account(args: &AccountArgs, json: bool) -> Result<ExitCode> {
    let session = auth::session(args).await?;
    let account = session.account().clone();
    auth::finish(session).await;

    if json {
        return print_json(&account);
    }
    let field = |name: &str| format!("{DIM}{name:<9}{DIM:#}");
    anstream::println!("{HEADER}{}{HEADER:#}", account.full_name);
    anstream::println!(
        "{} {} {DIM}({}){DIM:#}",
        field("canteen"),
        account.canteen_name.as_deref().unwrap_or("—"),
        account.canteen_number
    );
    anstream::println!("{} {}", field("username"), account.username);
    if let Some(email) = &account.email {
        anstream::println!("{} {email}", field("email"));
    }
    anstream::println!(
        "{} {OK}{} {}{OK:#}",
        field("balance"),
        account.balance,
        account.currency
    );
    Ok(ExitCode::SUCCESS)
}
