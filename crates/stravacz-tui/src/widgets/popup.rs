use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding};

use crate::theme;

/// A rectangle of at most `width`×`height`, centered in `area`.
#[must_use]
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Clear `area` and draw a floating card in it; returns the inner area.
pub fn card(frame: &mut Frame, area: Rect, title: &str, accent: Style) -> Rect {
    let block = Block::bordered()
        .border_set(border::ROUNDED)
        .border_style(accent)
        .title(Line::from(Span::styled(
            format!(" {title} "),
            accent.bold(),
        )))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block.style(theme::text()), area);
    inner
}

const HELP: &[(&str, &str)] = &[
    ("← → / h l", "previous / next day"),
    ("↑ ↓ / j k", "previous / next meal"),
    ("[ ] / PgUp PgDn", "previous / next week"),
    ("t", "jump to today"),
    ("g / G", "first / last day"),
    ("space / enter", "order or cancel meal"),
    ("s", "save pending changes"),
    ("u / esc", "discard pending changes"),
    ("r", "reload the menu"),
    ("L", "log out"),
    ("q / ctrl-c", "quit"),
];

pub fn help(frame: &mut Frame, area: Rect) {
    let height = u16::try_from(HELP.len()).unwrap_or(u16::MAX) + 6;
    let inner = card(
        frame,
        centered(area, 50, height),
        "Keys",
        Style::new().fg(theme::ACCENT),
    );
    let mut lines: Vec<Line> = HELP
        .iter()
        .map(|(key, action)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:>16}  "),
                    Style::new().fg(theme::ACCENT).bold(),
                ),
                Span::styled(*action, theme::text()),
            ])
        })
        .collect();
    lines.push(Line::default());
    lines
        .push(Line::styled("Unofficial, not affiliated with Strava.cz", theme::faint()).centered());
    lines.push(Line::default());
    lines.push(Line::styled("press any key to close", theme::faint()).centered());
    frame.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}

pub fn confirm_quit(frame: &mut Frame, area: Rect, pending: usize) {
    let inner = card(
        frame,
        centered(area, 46, 5),
        "Unsaved changes",
        Style::new().fg(theme::YELLOW),
    );
    let lines = vec![
        Line::styled(
            format!("{pending} pending change(s) will be lost."),
            theme::text(),
        )
        .centered(),
        Line::default(),
        Line::from([theme::key_hint("y", "quit"), theme::key_hint("n", "stay")].concat())
            .centered(),
    ];
    frame.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
}
