use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;

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

    let filter = EnvFilter::try_from_env("STRAVACZ_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,stravacz_api=debug"));
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_env_filter(filter)
        .init();
    Some(path)
}
