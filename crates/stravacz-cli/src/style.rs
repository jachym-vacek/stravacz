//! Output styles. `anstream` drops them when output is not a terminal or
//! `NO_COLOR` is set.

use anstyle::{AnsiColor, Style};
use clap::builder::Styles;

pub const HEADER: Style = Style::new().bold();
pub const DIM: Style = Style::new().dimmed();
pub const ID: Style = AnsiColor::Cyan.on_default();
pub const LABEL: Style = AnsiColor::Blue.on_default().bold();
pub const TODAY: Style = AnsiColor::Magenta.on_default();
pub const OK: Style = AnsiColor::Green.on_default().bold();
pub const WARN: Style = AnsiColor::Yellow.on_default().bold();
pub const ERROR: Style = AnsiColor::Red.on_default().bold();

/// Colors for `--help`.
#[must_use]
pub fn help() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default().bold())
        .usage(AnsiColor::Green.on_default().bold())
        .literal(AnsiColor::Cyan.on_default().bold())
        .placeholder(AnsiColor::Cyan.on_default())
}
