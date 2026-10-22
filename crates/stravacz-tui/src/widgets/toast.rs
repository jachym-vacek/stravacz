use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Info,
    Success,
    Warning,
    Error,
}

/// A short-lived notification in the bottom right corner.
#[derive(Debug, Clone)]
pub struct Toast {
    level: Level,
    message: String,
    expires: Instant,
}

impl Toast {
    fn new(level: Level, message: impl Into<String>) -> Self {
        let lifetime = match level {
            Level::Info | Level::Success => Duration::from_secs(3),
            Level::Warning | Level::Error => Duration::from_secs(6),
        };
        Self {
            level,
            message: message.into(),
            expires: Instant::now() + lifetime,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(Level::Info, message)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(Level::Success, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Level::Warning, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Level::Error, message)
    }

    #[must_use]
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires
    }

    /// Draw above the bottom line of `area`, aligned right.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let (icon, color): (&str, Color) = match self.level {
            Level::Info => ("i", theme::ACCENT),
            Level::Success => ("✓", theme::GREEN),
            Level::Warning => ("!", theme::YELLOW),
            Level::Error => ("✗", theme::RED),
        };

        let width = area.width.saturating_sub(4).min(56);
        let text_width = usize::from(width.saturating_sub(6)).max(1);
        let lines = self
            .message
            .chars()
            .count()
            .div_ceil(text_width)
            .clamp(1, 6);
        let height = u16::try_from(lines).unwrap_or(1) + 2;
        if width < 10 || area.height < height + 2 {
            return;
        }
        let toast = Rect {
            x: area.right() - width - 2,
            y: area.bottom() - height - 2,
            width,
            height,
        };

        let paragraph = Paragraph::new(Line::from(vec![
            Span::styled(format!("{icon} "), Style::new().fg(color).bold()),
            Span::styled(self.message.as_str(), theme::text()),
        ]))
        .wrap(Wrap { trim: true })
        .block(
            Block::bordered()
                .border_set(border::ROUNDED)
                .border_style(Style::new().fg(color))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );
        frame.render_widget(Clear, toast);
        frame.render_widget(paragraph, toast);
    }
}
