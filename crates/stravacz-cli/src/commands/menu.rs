use std::fmt::Write as _;
use std::process::ExitCode;

use anstyle::Style;
use anyhow::Result;
use chrono::Local;
use serde_json::json;
use stravacz_core::api::{Day, DayStatus, Meal, Restriction};

use super::{print, print_json};
use crate::auth;
use crate::cli::{AccountArgs, MenuArgs};
use crate::style::{DIM, HEADER, ID, LABEL, OK, TODAY};

/// Long names are not padded beyond this, so prices stay close.
const MAX_NAME_WIDTH: usize = 48;

pub async fn menu(account: &AccountArgs, args: &MenuArgs, json: bool) -> Result<ExitCode> {
    let session = auth::session(account).await?;
    let menu = session.menu().await;
    let currency = session.account().currency.clone();
    auth::finish(session).await;
    let menu = menu?;

    let days: Vec<Day> = menu
        .days()
        .iter()
        .filter(|day| args.all || day.date >= args.from)
        .take(if args.all { usize::MAX } else { args.days })
        .map(|day| Day {
            meals: day
                .meals
                .iter()
                .filter(|meal| !args.ordered || meal.ordered)
                .cloned()
                .collect(),
            ..day.clone()
        })
        .filter(|day| !args.ordered || !day.meals.is_empty())
        .collect();

    if json {
        return print_json(&json!({ "currency": currency, "days": days }));
    }
    if days.is_empty() {
        let what = if args.ordered { "No orders" } else { "No menu" };
        anstream::println!("{what} from {}.", args.from.format("%a %-d %b"));
        return Ok(ExitCode::SUCCESS);
    }

    // Built up front and written at once, so `stravacz menu | head` ends cleanly.
    let mut out = String::new();
    let table = Table::new(&days, &currency);
    let today = Local::now().date_naive();
    for (index, day) in days.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let _ = write!(out, "{HEADER}{}{HEADER:#}", day.date.format("%a %-d %b"));
        if day.date == today {
            let _ = write!(out, " {TODAY}today{TODAY:#}");
        }
        if let Some(note) = day_note(&day.status) {
            let _ = write!(out, "  {DIM}{note}{DIM:#}");
        }
        out.push('\n');

        if day.is_service_day() {
            for meal in &day.meals {
                let _ = writeln!(out, "{}", table.row(meal));
            }
        } else {
            let _ = writeln!(out, "  {DIM}the canteen is closed{DIM:#}");
        }
    }
    print(&out)?;
    Ok(ExitCode::SUCCESS)
}

/// Column widths shared by all rows, so the days line up.
struct Table<'a> {
    currency: &'a str,
    id: usize,
    label: usize,
    name: usize,
}

impl<'a> Table<'a> {
    fn new(days: &[Day], currency: &'a str) -> Self {
        let meals = || days.iter().flat_map(|day| &day.meals);
        let widest = |width: fn(&Meal) -> usize| meals().map(width).max().unwrap_or(0);
        Self {
            currency,
            id: widest(|meal| meal.id.to_string().len()),
            label: widest(|meal| meal.label.chars().count()),
            name: widest(|meal| meal.name.chars().count()).min(MAX_NAME_WIDTH),
        }
    }

    fn row(&self, meal: &Meal) -> String {
        let (icon, icon_style) = if meal.ordered {
            ("✓", OK)
        } else if meal.can_order() {
            ("○", Style::new())
        } else {
            ("·", DIM)
        };
        let name_style = if meal.ordered {
            HEADER
        } else if meal.can_order() {
            Style::new()
        } else {
            DIM
        };
        let price = if meal.price.is_zero() {
            String::new()
        } else {
            format!("{} {}", meal.price, self.currency)
        };
        let padding = " ".repeat(self.name.saturating_sub(meal.name.chars().count()));
        let (id, id_width) = (meal.id, self.id);
        let (label, label_width) = (&meal.label, self.label);

        let tag = tag(meal);

        let mut row = format!(
            "  {icon_style}{icon}{icon_style:#} {ID}{id:>id_width$}{ID:#}  \
             {LABEL}{label:<label_width$}{LABEL:#}  {name_style}{}{name_style:#}",
            meal.name,
        );
        if !price.is_empty() || !tag.is_empty() {
            let _ = write!(row, "{padding}  {price:>11}");
        }
        if !tag.is_empty() {
            let _ = write!(row, "  {DIM}{tag}{DIM:#}");
        }
        row
    }
}

/// Why a meal cannot be changed, if that is worth pointing out.
fn tag(meal: &Meal) -> &'static str {
    match (meal.ordered, &meal.order_restriction) {
        (true, _) if !meal.can_cancel() => "locked",
        (true, _) | (false, Restriction::None) => "",
        (false, Restriction::NotSeparately) => "with lunch",
        (false, Restriction::Closed) => "closed",
        (false, Restriction::NoService | Restriction::Other(_)) => "unavailable",
    }
}

fn day_note(status: &DayStatus) -> Option<&'static str> {
    match status {
        DayStatus::Normal | DayStatus::Other(_) => None,
        DayStatus::NotAutoOrdered => Some("not ordered automatically"),
        DayStatus::NoService => Some("no service"),
        DayStatus::Closed => Some("ordering closed"),
    }
}
