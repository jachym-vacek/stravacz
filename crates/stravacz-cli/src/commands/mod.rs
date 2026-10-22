mod account;
mod change;
mod login;
mod menu;

use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::Result;
use clap::CommandFactory;
use serde::Serialize;

use crate::cli::{Cli, Command};

pub async fn run(cli: Cli) -> Result<ExitCode> {
    let Cli {
        account,
        json,
        command,
    } = cli;

    match command {
        Command::Login { no_save_password } => {
            login::login(&account, !no_save_password, json).await
        }
        Command::Logout => login::logout(json),
        Command::Menu(args) => menu::menu(&account, &args, json).await,
        Command::Order(args) => change::change(&account, &args, true, json).await,
        Command::Cancel(args) => change::change(&account, &args, false, json).await,
        Command::Account => account::account(&account, json).await,
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "stravacz", &mut io::stdout());
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Write `text` to stdout, without styles unless it is a terminal.
fn print(text: &str) -> io::Result<()> {
    let mut stdout = anstream::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()
}

/// Pretty-print `value` as JSON on stdout.
fn print_json(value: &impl Serialize) -> Result<ExitCode> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    print(&text)?;
    Ok(ExitCode::SUCCESS)
}
