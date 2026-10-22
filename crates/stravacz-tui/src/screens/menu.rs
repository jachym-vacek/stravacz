use chrono::{Datelike, Local, NaiveDate, TimeDelta};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap};
use stravacz_core::OrderDraft;
use stravacz_core::api::{Day, Meal, MealKind, Menu, Money, Session, Submission};
use stravacz_core::wording::{day_note, meal_note};

use crate::app::Action;
use crate::theme;
use crate::widgets::Toast;

const DAYS_WIDTH: u16 = 24;
const DETAILS_HEIGHT: u16 = 7;

#[derive(Debug)]
pub struct MenuScreen {
    session: Session,
    menu: Menu,
    draft: OrderDraft,
    balance: Money,
    days: ListState,
    meals: ListState,
}

impl MenuScreen {
    pub fn new(session: Session, menu: Menu) -> Self {
        let mut screen = Self {
            balance: session.account().balance,
            session,
            menu,
            draft: OrderDraft::new(),
            days: ListState::default(),
            meals: ListState::default(),
        };
        screen.go_to_date(Local::now().date_naive());
        screen
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn draft(&self) -> &OrderDraft {
        &self.draft
    }

    /// Take over pending changes from an earlier session, dropping any that
    /// no longer apply.
    pub fn restore_draft(&mut self, mut draft: OrderDraft) {
        draft.rebase(&self.menu);
        self.draft = draft;
    }

    /// Show a freshly loaded menu, keeping the selection and pending changes.
    pub fn set_menu(&mut self, menu: Menu) {
        let date = self.current_day().map(|day| day.date);
        self.menu = menu;
        self.draft.rebase(&self.menu);
        let meal = self.meals.selected();
        match date {
            Some(date) => self.go_to_date(date),
            None => self.go_to_date(Local::now().date_naive()),
        }
        self.meals.select(meal);
        self.clamp_meal();
    }

    /// Take over the result of a submission and describe it.
    pub fn finish_submit(&mut self, submission: Submission) -> Toast {
        let requested = self.draft.len();
        if let Some(balance) = submission.balance {
            self.balance = balance;
        }
        let Some(menu) = submission.menu else {
            // Saved, so nothing is pending anymore; we just can't show the result.
            self.draft.clear();
            return Toast::warning(
                "Saved, but the menu couldn't be reloaded to check it. Press r to refresh.",
            );
        };
        self.set_menu(menu);

        let rejected = submission.rejected.len();
        if rejected == 0 {
            Toast::success(match requested {
                1 => "Order saved.".to_owned(),
                n => format!("{n} changes saved."),
            })
        } else {
            Toast::warning(format!(
                "The canteen did not apply {rejected} of {requested} changes; they are still pending."
            ))
        }
    }

    fn current_day(&self) -> Option<&Day> {
        self.menu.days().get(self.days.selected()?)
    }

    fn current_meal(&self) -> Option<&Meal> {
        self.current_day()?.meals.get(self.meals.selected()?)
    }

    /// Select `date`, or the first day after it, or the last day.
    fn go_to_date(&mut self, date: NaiveDate) {
        let days = self.menu.days();
        let index = days
            .iter()
            .position(|day| day.date >= date)
            .or_else(|| days.len().checked_sub(1));
        self.select_day(index);
    }

    fn select_day(&mut self, index: Option<usize>) {
        if self.days.selected() != index {
            self.meals.select(Some(0));
        }
        self.days.select(index);
        self.clamp_meal();
    }

    fn move_day(&mut self, by: isize) {
        let Some(last) = self.menu.days().len().checked_sub(1) else {
            return;
        };
        let current = self.days.selected().unwrap_or(0);
        self.select_day(Some(current.saturating_add_signed(by).min(last)));
    }

    fn move_week(&mut self, forward: bool) {
        let Some(date) = self.current_day().map(|day| day.date) else {
            return;
        };
        let target = date + TimeDelta::weeks(if forward { 1 } else { -1 });
        let monday = target - TimeDelta::days(i64::from(target.weekday().num_days_from_monday()));
        self.go_to_date(monday);
    }

    fn move_meal(&mut self, by: isize) {
        let current = self.meals.selected().unwrap_or(0);
        self.meals.select(Some(current.saturating_add_signed(by)));
        self.clamp_meal();
    }

    fn clamp_meal(&mut self) {
        let count = self.current_day().map_or(0, |day| day.meals.len());
        let selected = match count {
            0 => None,
            n => Some(self.meals.selected().unwrap_or(0).min(n - 1)),
        };
        self.meals.select(selected);
    }

    fn toggle(&mut self) -> Option<Action> {
        let id = self.current_meal()?.id;
        match self.draft.toggle(&self.menu, id) {
            Ok(()) => None,
            Err(err) => Some(Action::Notify(Toast::warning(err.to_string()))),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('q') => return Some(Action::Quit),
            KeyCode::Char('?') => return Some(Action::Help),
            KeyCode::Char('s') => return Some(Action::Submit),
            KeyCode::Char('r') => return Some(Action::Refresh),
            KeyCode::Char('L') => return Some(Action::Logout),
            KeyCode::Char(' ') | KeyCode::Enter => return self.toggle(),
            KeyCode::Char('u') | KeyCode::Esc if !self.draft.is_empty() => {
                let count = self.draft.len();
                self.draft.clear();
                return Some(Action::Notify(Toast::info(format!(
                    "Discarded {count} pending change{}.",
                    if count == 1 { "" } else { "s" }
                ))));
            }
            KeyCode::Left | KeyCode::Char('h') => self.move_day(-1),
            KeyCode::Right | KeyCode::Char('l') => self.move_day(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_meal(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_meal(1),
            KeyCode::PageUp | KeyCode::Char('[') => self.move_week(false),
            KeyCode::PageDown | KeyCode::Char(']') => self.move_week(true),
            KeyCode::Home | KeyCode::Char('g') => self.select_day(Some(0)),
            KeyCode::End | KeyCode::Char('G') => {
                self.select_day(self.menu.days().len().checked_sub(1));
            }
            KeyCode::Char('t') => self.go_to_date(Local::now().date_naive()),
            _ => {}
        }
        None
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, busy: Option<&str>) {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let [days, meals] =
            Layout::horizontal([Constraint::Length(DAYS_WIDTH), Constraint::Fill(1)]).areas(body);
        let [meals, details] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(DETAILS_HEIGHT)])
                .areas(meals);

        self.render_header(frame, header);
        self.render_days(frame, days);
        self.render_meals(frame, meals);
        self.render_details(frame, details);
        self.render_footer(frame, footer, busy);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let account = self.session.account();
        let canteen = account
            .canteen_name
            .as_deref()
            .unwrap_or(&account.canteen_number);

        frame.render_widget(
            Line::from(vec![
                Span::styled(
                    " strava.cz ",
                    Style::new().fg(theme::BASE).bg(theme::ACCENT).bold(),
                ),
                Span::styled(format!("  {canteen}"), theme::text()),
            ]),
            area,
        );
        frame.render_widget(
            Line::from(vec![
                Span::styled(format!("{}  ", account.full_name), theme::muted()),
                Span::styled(
                    format!(" {} {} ", self.balance, account.currency),
                    Style::new().fg(theme::BASE).bg(theme::GREEN).bold(),
                ),
            ])
            .right_aligned(),
            area,
        );
    }

    fn render_days(&mut self, frame: &mut Frame, area: Rect) {
        let today = Local::now().date_naive();
        let items: Vec<ListItem> = self
            .menu
            .days()
            .iter()
            .map(|day| {
                let (marker, marker_style) = self.day_marker(day);
                let date_style = if day.is_service_day() {
                    theme::text()
                } else {
                    theme::faint().add_modifier(Modifier::ITALIC)
                };
                let mut spans = vec![
                    Span::styled(format!("{marker} "), marker_style),
                    Span::styled(day.date.format("%a %d %b").to_string(), date_style),
                ];
                if day.date == today {
                    spans.push(Span::styled(" today", Style::new().fg(theme::MAUVE)));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items)
            .block(panel("Days"))
            .highlight_style(theme::selected().bold())
            .highlight_symbol(Span::styled("▌", Style::new().fg(theme::ACCENT)));
        frame.render_stateful_widget(list, area, &mut self.days);
    }

    fn day_marker(&self, day: &Day) -> (&'static str, Style) {
        if !day.is_service_day() {
            return ("–", theme::faint());
        }
        if day
            .meals
            .iter()
            .any(|meal| self.draft.changes_meal(meal.id))
        {
            return ("◆", Style::new().fg(theme::YELLOW));
        }
        let ordered = day
            .meals
            .iter()
            .any(|meal| meal.kind != MealKind::Soup && self.draft.is_ordered(meal));
        if ordered {
            ("●", Style::new().fg(theme::GREEN))
        } else if day.meals.iter().any(Meal::can_order) {
            ("○", theme::muted())
        } else {
            ("○", theme::faint())
        }
    }

    fn render_meals(&mut self, frame: &mut Frame, area: Rect) {
        let Some(day) = self.current_day() else {
            let message = Paragraph::new("No menu has been published yet.")
                .style(theme::muted())
                .centered()
                .block(panel("Menu"));
            frame.render_widget(message, area);
            return;
        };

        let mut block = panel(&day.date.format("%A, %-d %B %Y").to_string());
        if let Some(note) = day_note(&day.status) {
            block = block.title_top(
                Line::styled(format!(" {note} "), Style::new().fg(theme::PEACH)).right_aligned(),
            );
        }

        if !day.is_service_day() {
            let message = Paragraph::new("The canteen is closed on this day.")
                .style(theme::muted())
                .centered()
                .block(block.padding(Padding::top(area.height.saturating_sub(3) / 2)));
            frame.render_widget(message, area);
            return;
        }

        let width = usize::from(block.inner(area).width.saturating_sub(5)).max(10);
        let items: Vec<ListItem> = day
            .meals
            .iter()
            .map(|meal| self.meal_item(meal, width))
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(theme::selected())
            .highlight_symbol(Span::styled("▌", Style::new().fg(theme::ACCENT)));
        frame.render_stateful_widget(list, area, &mut self.meals);
    }

    fn meal_item(&self, meal: &Meal, width: usize) -> ListItem<'static> {
        let pending = self.draft.changes_meal(meal.id);
        let ordered = self.draft.is_ordered(meal);

        let (icon, icon_style, tag, tag_style) = match (ordered, pending, meal_note(meal)) {
            (true, true, _) => (
                "+",
                theme::YELLOW,
                "will order".to_owned(),
                Style::new().fg(theme::YELLOW),
            ),
            (false, true, _) => (
                "−",
                theme::RED,
                "will cancel".to_owned(),
                Style::new().fg(theme::RED),
            ),
            (true, false, Some(note)) => (
                "✓",
                theme::GREEN,
                format!("ordered · {note}"),
                theme::faint(),
            ),
            (true, false, None) => (
                "✓",
                theme::GREEN,
                "ordered".to_owned(),
                Style::new().fg(theme::GREEN),
            ),
            (false, false, Some(note)) => ("·", theme::OVERLAY, note.to_owned(), theme::faint()),
            (false, false, None) => ("○", theme::SUBTEXT, String::new(), theme::faint()),
        };

        let label_color = match meal.kind {
            MealKind::Soup => theme::PEACH,
            MealKind::Main => theme::ACCENT,
            MealKind::Other => theme::MAUVE,
        };
        let mut header = vec![
            Span::styled(format!(" {icon}  "), Style::new().fg(icon_style).bold()),
            Span::styled(meal.label.clone(), Style::new().fg(label_color).bold()),
        ];
        if !meal.price.is_zero() {
            header.push(Span::styled(
                format!("  {} {}", meal.price, self.session.account().currency),
                theme::faint(),
            ));
        }
        if !tag.is_empty() {
            header.push(Span::styled(format!("  {tag}"), tag_style.italic()));
        }

        let mut name_style = if ordered {
            theme::text().bold()
        } else if meal.can_order() {
            theme::text()
        } else {
            theme::muted()
        };
        if pending && !ordered {
            name_style = name_style.add_modifier(Modifier::CROSSED_OUT);
        }

        let mut lines = vec![Line::from(header)];
        lines.extend(
            wrap(&meal.name, width)
                .into_iter()
                .map(|line| Line::styled(format!("    {line}"), name_style)),
        );
        lines.push(Line::default());
        ListItem::new(Text::from(lines))
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let block = panel("Details").padding(Padding::horizontal(1));
        let Some(meal) = self.current_meal() else {
            frame.render_widget(block, area);
            return;
        };

        let field = |name: &'static str| Span::styled(format!("{name:<11}"), theme::faint());
        let deadline = match meal.deadline {
            Some(deadline) => deadline.format("%a %-d %b, %H:%M").to_string(),
            None => "—".to_owned(),
        };
        let allergens = if meal.allergens.is_empty() {
            "none listed".to_owned()
        } else {
            meal.allergens
                .iter()
                .map(|a| match a.name.as_str() {
                    "" => a.code.clone(),
                    name => format!("{} {name}", a.code.trim_start_matches('0')),
                })
                .collect::<Vec<_>>()
                .join(" · ")
        };

        let lines = vec![
            Line::from(vec![
                field("Meal"),
                Span::styled(meal.name.clone(), theme::title()),
            ]),
            Line::from(vec![
                field("Price"),
                Span::styled(
                    format!("{} {}", meal.price, self.session.account().currency),
                    theme::text(),
                ),
            ]),
            Line::from(vec![
                field("Changes by"),
                Span::styled(deadline, theme::text()),
            ]),
            Line::from(vec![
                field("Allergens"),
                Span::styled(allergens, theme::muted()),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
            area,
        );
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect, busy: Option<&str>) {
        let hints = [
            ("←→", "day"),
            ("↑↓", "meal"),
            ("space", "toggle"),
            ("s", "save"),
            ("u", "undo"),
            ("r", "refresh"),
            ("?", "help"),
        ];
        let status = if let Some(busy) = busy {
            Line::styled(format!("{busy} "), Style::new().fg(theme::ACCENT))
        } else if self.draft.is_empty() {
            Line::styled("no pending changes ", theme::faint())
        } else {
            let count = self.draft.len();
            let cost = self.draft.cost(&self.menu);
            let sign = if cost.hundredths() > 0 { "+" } else { "" };
            Line::styled(
                format!(
                    "◆ {count} pending · {sign}{cost} {} ",
                    self.session.account().currency
                ),
                Style::new().fg(theme::YELLOW).bold(),
            )
        };
        let status_width = u16::try_from(status.width()).unwrap_or(u16::MAX);
        let [hints_area, status_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(status_width)]).areas(area);
        frame.render_widget(status, status_area);

        // Show only the hints that fit whole.
        let mut room = usize::from(hints_area.width);
        let spans: Vec<Span> = hints
            .iter()
            .map(|(key, action)| theme::key_hint(key, action))
            .take_while(|hint| {
                let width: usize = hint.iter().map(Span::width).sum();
                room = room.saturating_sub(width);
                room > 0
            })
            .flatten()
            .collect();
        frame.render_widget(Line::from(spans), hints_area);
    }
}

fn panel(title: &str) -> Block<'static> {
    Block::bordered()
        .border_set(border::ROUNDED)
        .border_style(theme::border())
        .title(Span::styled(format!(" {title} "), theme::title()))
}

/// Greedy word wrap; long words are left to overflow.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wraps_words() {
        assert_eq!(wrap("a bb ccc dddd", 6), ["a bb", "ccc", "dddd"]);
        assert_eq!(wrap("", 6), [""]);
    }
}
