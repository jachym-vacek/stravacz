use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// A single-line text field.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    value: String,
    /// Cursor position, in characters.
    cursor: usize,
    masked: bool,
}

impl TextInput {
    /// A field that hides what is typed.
    #[must_use]
    pub fn masked() -> Self {
        Self {
            masked: true,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.len();
    }

    fn len(&self) -> usize {
        self.value.chars().count()
    }

    fn byte_index(&self, chars: usize) -> usize {
        self.value
            .char_indices()
            .nth(chars)
            .map_or(self.value.len(), |(index, _)| index)
    }

    /// Edit the value; returns whether the key was used.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('u') if ctrl => {
                self.value.drain(..self.byte_index(self.cursor));
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let before: String = self.value.chars().take(self.cursor).collect();
                let kept = before.trim_end().rfind(' ').map_or(0, |i| i + 1);
                let removed = before[kept..].chars().count();
                self.value.drain(kept..self.byte_index(self.cursor));
                self.cursor -= removed;
            }
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.len(),
            KeyCode::Char(c) if !ctrl => {
                let index = self.byte_index(self.cursor);
                self.value.insert(index, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.value.remove(self.byte_index(self.cursor));
            }
            KeyCode::Delete if self.cursor < self.len() => {
                self.value.remove(self.byte_index(self.cursor));
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            _ => return false,
        }
        true
    }

    /// Draw the field, scrolled so the cursor stays visible.
    pub fn render(&self, frame: &mut Frame, area: Rect, style: Style, focused: bool) {
        let width = usize::from(area.width.saturating_sub(2)).max(1);
        let offset = self.cursor.saturating_sub(width - 1);
        let visible: String = if self.masked {
            "•".repeat(self.len().saturating_sub(offset).min(width))
        } else {
            self.value.chars().skip(offset).take(width).collect()
        };

        frame.render_widget(
            Paragraph::new(Line::from(format!(" {visible}"))).style(style),
            area,
        );
        if focused {
            let x = u16::try_from(self.cursor - offset + 1).unwrap_or(u16::MAX);
            frame.set_cursor_position(Position::new(area.x.saturating_add(x), area.y));
        }
    }
}
