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

    let mut ids: Vec<MealId> = args.meals.iter().copied().map(MealId).collect();
    ids.sort_unstable();
    ids.dedup();

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
    let applied = |id: &MealId| !submission.rejected.contains(id);

    if json {
        print_json(&json!({
            "dry_run": false,
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
fn plan<'m>(menu: &'m Menu, ids: &[MealId], ordered: bool) -> Result<(OrderDraft, Vec<&'m Meal>)> {
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
