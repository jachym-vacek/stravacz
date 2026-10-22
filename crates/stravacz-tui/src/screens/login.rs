use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph};
use stravacz_core::SavedLogin;
use stravacz_core::api::Credentials;

use crate::app::Action;
use crate::theme;
use crate::widgets::{TextInput, popup};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Canteen,
    Username,
    Password,
    Remember,
    Submit,
}

impl Field {
    const ALL: [Self; 5] = [
        Self::Canteen,
        Self::Username,
        Self::Password,
        Self::Remember,
        Self::Submit,
    ];

    fn offset(self, by: isize) -> Self {
        let index = Self::ALL.iter().position(|&f| f == self).unwrap_or(0);
        let next = (index.cast_signed() + by).rem_euclid(Self::ALL.len().cast_signed());
        Self::ALL[next.cast_unsigned()]
    }
}

#[derive(Debug)]
pub struct LoginScreen {
    canteen: TextInput,
    username: TextInput,
    password: TextInput,
    remember: bool,
    focus: Field,
    error: Option<String>,
}

impl LoginScreen {
    pub fn new(saved: Option<&SavedLogin>) -> Self {
        let mut screen = Self {
            canteen: TextInput::default(),
            username: TextInput::default(),
            password: TextInput::masked(),
            remember: true,
            focus: Field::Canteen,
            error: None,
        };
        if let Some(saved) = saved {
            screen.canteen.set_value(&saved.canteen);
            screen.username.set_value(&saved.username);
            screen.remember = saved.remember_password;
            screen.focus = Field::Password;
        }
        screen
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    /// Fill in a stored password and try to log in with it.
    pub fn autofill(&mut self, password: String) -> Option<Action> {
        self.password.set_value(password);
        self.submit()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc => return Some(Action::Quit),
            KeyCode::Tab | KeyCode::Down => self.focus = self.focus.offset(1),
            KeyCode::BackTab | KeyCode::Up => self.focus = self.focus.offset(-1),
            KeyCode::Char(' ') if self.focus == Field::Remember => {
                self.remember = !self.remember;
            }
            KeyCode::Enter => return self.submit(),
            KeyCode::Char(' ') if self.focus == Field::Submit => return self.submit(),
            _ => {
                if let Some(input) = self.focused_input() {
                    input.handle_key(key);
                }
            }
        }
        None
    }

    fn focused_input(&mut self) -> Option<&mut TextInput> {
        match self.focus {
            Field::Canteen => Some(&mut self.canteen),
            Field::Username => Some(&mut self.username),
            Field::Password => Some(&mut self.password),
            Field::Remember | Field::Submit => None,
        }
    }

    fn submit(&mut self) -> Option<Action> {
        let missing = [
            (Field::Canteen, &self.canteen, "canteen number"),
            (Field::Username, &self.username, "username"),
            (Field::Password, &self.password, "password"),
        ]
        .into_iter()
        .find(|(_, input, _)| input.value().trim().is_empty());

        if let Some((field, _, name)) = missing {
            self.focus = field;
            self.error = Some(format!("Please enter your {name}."));
            return None;
        }

        self.error = None;
        Some(Action::Login {
            credentials: Credentials::new(
                self.canteen.value().trim(),
                self.username.value().trim(),
                self.password.value(),
            ),
            remember: self.remember,
        })
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, busy: Option<&str>) {
        let card = popup::centered(area, 52, 21);
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(theme::border())
            .padding(Padding::new(4, 4, 1, 1));
        let inner = block.inner(card);
        frame.render_widget(block, card);

        let [
            title,
            subtitle,
            _,
            canteen,
            _,
            username,
            _,
            password,
            _,
            remember,
            _,
            submit,
            _,
            status,
        ] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);

        frame.render_widget(
            Line::from(vec![
                Span::styled("strava", Style::new().fg(theme::ACCENT).bold()),
                Span::styled(".cz", theme::muted().bold()),
            ])
            .centered(),
            title,
        );
        frame.render_widget(
            Line::styled("Canteen orders in your terminal", theme::faint()).centered(),
            subtitle,
        );

        self.render_field(
            frame,
            canteen,
            Field::Canteen,
            "Canteen number",
            &self.canteen,
        );
        self.render_field(frame, username, Field::Username, "Username", &self.username);
        self.render_field(frame, password, Field::Password, "Password", &self.password);

        let focused = self.focus == Field::Remember;
        let (mark, mark_style) = if self.remember {
            ("✓", Style::new().fg(theme::GREEN).bold())
        } else {
            (" ", Style::new())
        };
        frame.render_widget(
            Line::from(vec![
                Span::styled("[", theme::muted()),
                Span::styled(mark, mark_style),
                Span::styled("] ", theme::muted()),
                Span::styled(
                    "Remember password",
                    if focused {
                        theme::title().fg(theme::ACCENT)
                    } else {
                        theme::text()
                    },
                ),
                Span::styled("  (system keyring)", theme::faint()),
            ]),
            remember,
        );

        let button = if self.focus == Field::Submit {
            Style::new().fg(theme::BASE).bg(theme::ACCENT).bold()
        } else {
            Style::new().fg(theme::TEXT).bg(theme::SURFACE_BRIGHT)
        };
        frame.render_widget(
            Line::from(Span::styled("   Sign in   ", button)).centered(),
            submit,
        );

        let status_line = match (busy, &self.error) {
            (Some(busy), _) => Line::styled(busy, Style::new().fg(theme::ACCENT)),
            (None, Some(error)) => Line::styled(error.as_str(), Style::new().fg(theme::RED)),
            (None, None) => Line::from(theme::key_hint("tab", "next").to_vec()),
        };
        frame.render_widget(Paragraph::new(status_line.centered()), status);
    }

    fn render_field(
        &self,
        frame: &mut Frame,
        area: Rect,
        field: Field,
        label: &str,
        input: &TextInput,
    ) {
        let focused = self.focus == field;
        let [label_area, input_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

        let (label_style, input_style) = if focused {
            (
                Style::new().fg(theme::ACCENT).bold(),
                Style::new().fg(theme::TEXT).bg(theme::SURFACE_BRIGHT),
            )
        } else {
            (
                theme::muted(),
                Style::new().fg(theme::TEXT).bg(theme::SURFACE),
            )
        };
        frame.render_widget(Line::styled(label, label_style), label_area);
        input.render(frame, input_area, input_style, focused);
    }
}
