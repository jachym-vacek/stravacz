use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use serde_json::json;
use stravacz_core::OrderDraft;
use stravacz_core::api::{Meal, MealId, Menu, Session};

use super::print_json;
use crate::auth;
use crate::cli::{AccountArgs, ChangeArgs};
use crate::style::{DIM, ERROR, ID, OK, WARN};

/// Order (`ordered = true`) or cancel the meals in `args`.
pub async fn change(
    account: &AccountArgs,
    args: &ChangeArgs,
    ordered: bool,
    json: bool,
) -> Result<ExitCode> {
    let session = auth::session(account).await?;
    let result = apply(&session, args, ordered, json).await;
    auth::finish(session).await;
    result
}

async fn apply(
    session: &Session,
    args: &ChangeArgs,
    ordered: bool,
    json: bool,
) -> Result<ExitCode> {
    let menu = session.menu().await?;
    let currency = &session.account().currency;

    let ids: Vec<MealId> = args.meals.iter().copied().map(MealId).collect();
    let (draft, unchanged) = plan(&menu, &ids, ordered)?;
    let changes = draft.changes();

    if !json {
        for meal in &unchanged {
            let state = if ordered {
                "already ordered"
            } else {
                "not ordered"
            };
            anstream::println!("  {DIM}={DIM:#} {}  {DIM}({state}){DIM:#}", describe(meal));
        }
        for change in &changes {
            let meal = menu
                .meal(change.meal)
                .context("meal vanished from the menu")?;
            let (sign, sign_style, price) = if change.ordered {
                ("+", OK, meal.price)
            } else {
                ("−", ERROR, -meal.price)
            };
            anstream::println!(
                "  {sign_style}{sign}{sign_style:#} {}  {DIM}{price} {currency}{DIM:#}",
                describe(meal)
            );
        }
    }

    if changes.is_empty() || args.dry_run {
        if json {
            return print_json(&json!({
                "dry_run": args.dry_run,
                "changes": changes,
                "unchanged": unchanged.iter().map(|meal| meal.id).collect::<Vec<_>>(),
            }));
        }
        let note = if changes.is_empty() {
            "Nothing to change."
        } else {
            "Dry run, nothing saved."
        };
        anstream::println!("{DIM}{note}{DIM:#}");
        return Ok(ExitCode::SUCCESS);
    }

    let submission = session.submit(&changes).await?;
    // Unknown (`null`) when the menu could not be reloaded to check.
    let applied = |id: &MealId| {
        submission
            .is_verified()
            .then(|| !submission.rejected.contains(id))
    };

    if json {
        print_json(&json!({
            "dry_run": false,
            "verified": submission.is_verified(),
            "changes": changes
                .iter()
                .map(|change| json!({
                    "meal": change.meal,
                    "ordered": change.ordered,
                    "applied": applied(&change.meal),
                }))
                .collect::<Vec<_>>(),
            "unchanged": unchanged.iter().map(|meal| meal.id).collect::<Vec<_>>(),
            "balance": submission.balance,
        }))?;
    } else {
        let saved = changes.len() - submission.rejected.len();
        if saved > 0 {
            let plural = if saved == 1 { "" } else { "s" };
            anstream::println!("{OK}✓{OK:#} Saved {saved} change{plural}.");
        }
        if !submission.is_verified() {
            anstream::eprintln!(
                "{WARN}warning:{WARN:#} could not reload the menu to check the changes; \
                 see `stravacz menu`"
            );
        }
        if !submission.rejected.is_empty() {
            let ids: Vec<String> = submission.rejected.iter().map(MealId::to_string).collect();
            anstream::eprintln!(
                "{WARN}warning:{WARN:#} the canteen did not apply the change to meal(s) {}",
                ids.join(", ")
            );
        }
        if let Some(balance) = submission.balance {
            anstream::println!("  {DIM}Balance: {balance} {currency}{DIM:#}");
        }
    }

    Ok(if submission.rejected.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Work out the changes, and which meals are already in the wanted state.
/// Repeated ids count once.
fn plan<'m>(menu: &'m Menu, ids: &[MealId], ordered: bool) -> Result<(OrderDraft, Vec<&'m Meal>)> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();

    let meals = ids
        .iter()
        .map(|&id| {
            menu.meal(id)
                .with_context(|| format!("meal {id} is not on the menu; see `stravacz menu`"))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut draft = OrderDraft::new();
    let mut unchanged = Vec::new();
    for &meal in &meals {
        if draft.is_ordered(meal) == ordered {
            unchanged.push(meal);
        } else {
            draft
                .toggle(menu, meal.id)
                .with_context(|| format!("meal {}", meal.id))?;
        }
    }

    // Ordering a lunch swaps out the day's other one, which may be one we asked for.
    if let Some(meal) = meals.iter().find(|meal| draft.is_ordered(meal) != ordered) {
        bail!(
            "meal {} can't be ordered together with another lunch on {}; only one per day",
            meal.id,
            meal.date.format("%a %-d %b")
        );
    }
    Ok((draft, unchanged))
}

fn describe(meal: &Meal) -> String {
    format!(
        "{ID}{}{ID:#}  {}  {}  {}",
        meal.id,
        meal.date.format("%a %-d %b"),
        meal.label,
        meal.name
    )
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use stravacz_core::api::{Change, Day, DayStatus, MealKind, Money, Restriction};

    use super::*;

    /// A main course on day `day` of September 2026.
    fn lunch(id: u32, day: u32, ordered: bool, locked: bool) -> Meal {
        let restriction = if locked {
            Restriction::Closed
        } else {
            Restriction::None
        };
        Meal {
            id: MealId(id),
            date: NaiveDate::from_ymd_opt(2026, 9, day).unwrap(),
            kind: MealKind::Main,
            label: format!("Oběd {id}"),
            name: format!("Meal {id}"),
            price: Money::from_hundredths(5000),
            ordered,
            order_restriction: restriction.clone(),
            cancel_restriction: restriction,
            allergens: Vec::new(),
            deadline: None,
        }
    }

    fn menu(meals: Vec<Meal>) -> Menu {
        let mut days: Vec<Day> = Vec::new();
        for meal in meals {
            match days.iter_mut().find(|day| day.date == meal.date) {
                Some(day) => day.meals.push(meal),
                None => days.push(Day {
                    date: meal.date,
                    status: DayStatus::Normal,
                    meals: vec![meal],
                }),
            }
        }
        Menu::new(days)
    }

    fn ids(ids: &[u32]) -> Vec<MealId> {
        ids.iter().copied().map(MealId).collect()
    }

    fn unchanged_ids(unchanged: &[&Meal]) -> Vec<MealId> {
        unchanged.iter().map(|meal| meal.id).collect()
    }

    #[test]
    fn orders_on_different_days_and_skips_repeats() {
        let menu = menu(vec![lunch(1, 1, false, false), lunch(2, 2, false, false)]);

        let (draft, unchanged) = plan(&menu, &ids(&[2, 1, 2]), true).unwrap();
        assert_eq!(
            draft.changes(),
            [Change::order(MealId(1)), Change::order(MealId(2))]
        );
        assert_eq!(unchanged_ids(&unchanged), []);
    }

    #[test]
    fn reports_meals_already_in_the_wanted_state() {
        let menu = menu(vec![lunch(1, 1, true, false), lunch(2, 2, false, false)]);

        let (draft, unchanged) = plan(&menu, &ids(&[1, 2]), true).unwrap();
        assert_eq!(draft.changes(), [Change::order(MealId(2))]);
        assert_eq!(unchanged_ids(&unchanged), ids(&[1]));

        let (draft, unchanged) = plan(&menu, &ids(&[2]), false).unwrap();
        assert!(draft.is_empty());
        assert_eq!(unchanged_ids(&unchanged), ids(&[2]));
    }

    #[test]
    fn ordering_a_lunch_swaps_out_the_ordered_one() {
        let menu = menu(vec![lunch(1, 1, true, false), lunch(2, 1, false, false)]);

        let (draft, _) = plan(&menu, &ids(&[2]), true).unwrap();
        assert_eq!(
            draft.changes(),
            [Change::cancel(MealId(1)), Change::order(MealId(2))]
        );
    }

    #[test]
    fn refuses_two_lunches_on_one_day() {
        let menu = menu(vec![lunch(1, 1, false, false), lunch(2, 1, false, false)]);

        let err = plan(&menu, &ids(&[1, 2]), true).unwrap_err();
        assert!(err.to_string().contains("only one per day"), "{err}");
    }

    #[test]
    fn refuses_unknown_and_locked_meals() {
        let menu = menu(vec![lunch(1, 1, true, true)]);

        let err = plan(&menu, &ids(&[9]), true).unwrap_err();
        assert!(err.to_string().contains("not on the menu"), "{err}");

        let err = plan(&menu, &ids(&[1]), false).unwrap_err();
        assert!(format!("{err:#}").contains("deadline"), "{err:#}");
    }
}
