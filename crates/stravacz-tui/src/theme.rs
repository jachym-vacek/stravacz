//! Colors and shared styles (Catppuccin Mocha).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT: Color = Color::Rgb(166, 173, 200);
pub const OVERLAY: Color = Color::Rgb(108, 112, 134);
pub const SURFACE: Color = Color::Rgb(49, 50, 68);
pub const SURFACE_BRIGHT: Color = Color::Rgb(69, 71, 90);
pub const BASE: Color = Color::Rgb(30, 30, 46);

pub const ACCENT: Color = Color::Rgb(137, 180, 250);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const MAUVE: Color = Color::Rgb(203, 166, 247);

#[must_use]
pub fn text() -> Style {
    Style::new().fg(TEXT)
}

#[must_use]
pub fn muted() -> Style {
    Style::new().fg(SUBTEXT)
}

#[must_use]
pub fn faint() -> Style {
    Style::new().fg(OVERLAY)
}

#[must_use]
pub fn border() -> Style {
    Style::new().fg(SURFACE_BRIGHT)
}

#[must_use]
pub fn title() -> Style {
    Style::new().fg(TEXT).add_modifier(Modifier::BOLD)
}

#[must_use]
pub fn selected() -> Style {
    Style::new().bg(SURFACE)
}

/// A key rendered as a little chip, followed by what it does.
#[must_use]
pub fn key_hint<'a>(key: &'a str, action: &'a str) -> [Span<'a>; 3] {
    [
        Span::styled(format!(" {key} "), Style::new().fg(BASE).bg(SUBTEXT)),
        Span::styled(format!(" {action}"), muted()),
        Span::raw("  "),
    ]
}
