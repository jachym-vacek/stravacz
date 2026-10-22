use std::env;
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Mutex;

use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Log to a file, since stdout belongs to the UI. Filter with `STRAVACZ_LOG`.
///
/// Returns the log file's path, if logging could be set up.
pub fn init() -> Option<PathBuf> {
    let dir = stravacz_core::project_dirs()?
        .data_local_dir()
        .to_path_buf();
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join("stravacz.log");
    let file = File::create(&path).ok()?;

    // `Targets` takes `level,target=level,…` like `EnvFilter`, without regexes.
    let filter = env::var("STRAVACZ_LOG")
        .ok()
        .and_then(|directives| directives.parse::<Targets>().ok())
        .unwrap_or_else(|| {
            Targets::new()
                .with_default(Level::INFO)
                .with_target("stravacz_api", Level::DEBUG)
        });
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(Mutex::new(file))
                .with_ansi(false),
        )
        .with(filter)
        .init();
    Some(path)
}
