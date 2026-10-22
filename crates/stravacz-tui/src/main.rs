//! A terminal client for the Strava.cz canteen ordering system.

mod app;
mod logging;
mod screens;
mod theme;
mod widgets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(path) = logging::init() {
        tracing::info!(path = %path.display(), "starting stravacz-tui");
    }

    let terminal = ratatui::init();
    let result = app::run(terminal).await;
    ratatui::restore();
    result
}
