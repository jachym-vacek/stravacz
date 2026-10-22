//! A command line client for the Strava.cz canteen ordering system.

mod auth;
mod cli;
mod commands;
mod style;

use std::io;
use std::process::ExitCode;

use clap::Parser;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match commands::run(cli).await {
        Ok(code) => code,
        // The reader went away, e.g. `stravacz menu | head`; that is fine.
        Err(err) if is_broken_pipe(&err) => ExitCode::SUCCESS,
        Err(err) => {
            let error = style::ERROR;
            anstream::eprintln!("{error}error:{error:#} {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|err| err.kind() == io::ErrorKind::BrokenPipe)
}
