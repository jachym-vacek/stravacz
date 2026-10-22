use std::collections::BTreeMap;

use stravacz_api::{Change, Meal, MealId, MealKind, Menu, Money};

use crate::wording::restriction_reason;

/// Order changes the user has picked but not submitted yet.
///
/// The draft enforces the canteen's rules locally, so the user gets immediate
/// feedback instead of a failed submission:
/// - meals past their deadline cannot be changed,
/// - only one main course can be ordered per day; picking another one swaps it.
#[derive(Debug, Clone, Default)]
pub struct OrderDraft {
    /// Desired state of each changed meal. Never contains a no-op.
    desired: BTreeMap<MealId, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToggleError {
    #[error("this meal is no longer on the menu")]
    UnknownMeal,
    #[error("can't order this meal: {0}")]
    CannotOrder(&'static str),
    #[error("can't cancel this meal: {0}")]
    CannotCancel(&'static str),
    #[error("“{0}” is already ordered for that day and can't be changed anymore")]
    Conflict(String),
}

impl OrderDraft {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `meal` will be ordered once the draft is submitted.
    #[must_use]
    pub fn is_ordered(&self, meal: &Meal) -> bool {
        self.desired.get(&meal.id).copied().unwrap_or(meal.ordered)
    }

    /// Whether the draft changes `meal`.
    #[must_use]
    pub fn changes_meal(&self, id: MealId) -> bool {
        self.desired.contains_key(&id)
    }

    /// Flip the meal's ordered state, swapping out a conflicting main course.
    pub fn toggle(&mut self, menu: &Menu, id: MealId) -> Result<(), ToggleError> {
        let meal = menu.meal(id).ok_or(ToggleError::UnknownMeal)?;
        let ordered = !self.is_ordered(meal);
        if !meal.can_set(ordered) {
            return Err(if ordered {
                ToggleError::CannotOrder(
                    restriction_reason(&meal.order_restriction).unwrap_or_default(),
                )
            } else {
                ToggleError::CannotCancel(
                    restriction_reason(&meal.cancel_restriction).unwrap_or_default(),
                )
            });
        }

        let rivals: Vec<&Meal> = if ordered && meal.kind == MealKind::Main {
            menu.day(meal.date)
                .into_iter()
                .flat_map(|day| &day.meals)
                .filter(|m| m.id != id && m.kind == MealKind::Main && self.is_ordered(m))
                .collect()
        } else {
            Vec::new()
        };
        if let Some(stuck) = rivals.iter().find(|rival| !rival.can_set(false)) {
            return Err(ToggleError::Conflict(stuck.name.clone()));
        }

        for rival in rivals {
            self.set(rival, false);
        }
        self.set(meal, ordered);
        Ok(())
    }

    fn set(&mut self, meal: &Meal, ordered: bool) {
        if ordered == meal.ordered {
            self.desired.remove(&meal.id);
        } else {
            self.desired.insert(meal.id, ordered);
        }
    }

    /// The changes to submit.
    #[must_use]
    pub fn changes(&self) -> Vec<Change> {
        self.desired
            .iter()
            .map(|(&meal, &ordered)| Change { meal, ordered })
            .collect()
    }

    /// How much the balance changes once submitted (positive = costs money).
    #[must_use]
    pub fn cost(&self, menu: &Menu) -> Money {
        self.desired
            .iter()
            .filter_map(|(&id, &ordered)| {
                let price = menu.meal(id)?.price;
                Some(if ordered { price } else { -price })
            })
            .sum()
    }

    /// Re-apply the draft on top of a freshly loaded menu, dropping changes that
    /// are no longer possible or already done.
    pub fn rebase(&mut self, menu: &Menu) {
        self.desired.retain(|&id, &mut ordered| {
            menu.meal(id)
                .is_some_and(|meal| meal.ordered != ordered && meal.can_set(ordered))
        });
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.desired.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.desired.is_empty()
    }

    pub fn clear(&mut self) {
        self.desired.clear();
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use stravacz_api::{Day, DayStatus, Restriction};

    use super::*;

    fn meal(id: u32, kind: MealKind, ordered: bool, locked: bool) -> Meal {
        let restriction = if locked {
            Restriction::Closed
        } else {
            Restriction::None
        };
        Meal {
            id: MealId(id),
            date: NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(),
            kind,
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
        Menu::new(vec![Day {
            date: meals[0].date,
            status: DayStatus::Normal,
            meals,
        }])
    }

    #[test]
    fn toggling_twice_is_a_no_op() {
        let menu = menu(vec![meal(1, MealKind::Main, false, false)]);
        let mut draft = OrderDraft::new();

        draft.toggle(&menu, MealId(1)).unwrap();
        assert_eq!(draft.changes(), [Change::order(MealId(1))]);
        assert_eq!(draft.cost(&menu), Money::from_hundredths(5000));

        draft.toggle(&menu, MealId(1)).unwrap();
        assert!(draft.is_empty());
    }

    #[test]
    fn ordering_a_main_swaps_out_the_other() {
        let menu = menu(vec![
            meal(1, MealKind::Main, true, false),
            meal(2, MealKind::Main, false, false),
            meal(3, MealKind::Soup, true, false),
        ]);
        let mut draft = OrderDraft::new();

        draft.toggle(&menu, MealId(2)).unwrap();
        assert_eq!(
            draft.changes(),
            [Change::cancel(MealId(1)), Change::order(MealId(2))]
        );
        assert_eq!(draft.cost(&menu), Money::ZERO);
    }

    #[test]
    fn locked_meals_cannot_change() {
        let menu = menu(vec![
            meal(1, MealKind::Main, true, true),
            meal(2, MealKind::Main, false, false),
        ]);
        let mut draft = OrderDraft::new();

        assert!(matches!(
            draft.toggle(&menu, MealId(1)),
            Err(ToggleError::CannotCancel(_))
        ));
        assert!(matches!(
            draft.toggle(&menu, MealId(2)),
            Err(ToggleError::Conflict(_))
        ));
        assert!(draft.is_empty());
    }

    #[test]
    fn rebase_drops_applied_changes() {
        let mut draft = OrderDraft::new();
        draft
            .toggle(
                &menu(vec![meal(1, MealKind::Main, false, false)]),
                MealId(1),
            )
            .unwrap();

        draft.rebase(&menu(vec![meal(1, MealKind::Main, true, false)]));
        assert!(draft.is_empty());
    }

    #[test]
    fn rebase_drops_impossible_changes() {
        let mut draft = OrderDraft::new();
        draft
            .toggle(
                &menu(vec![
                    meal(1, MealKind::Main, false, false),
                    meal(2, MealKind::Soup, false, false),
                ]),
                MealId(1),
            )
            .unwrap();
        draft
            .toggle(
                &menu(vec![meal(2, MealKind::Soup, false, false)]),
                MealId(2),
            )
            .unwrap();
        assert_eq!(draft.len(), 2);

        // Meal 1 is past its deadline now; meal 2 is still open.
        draft.rebase(&menu(vec![
            meal(1, MealKind::Main, false, true),
            meal(2, MealKind::Soup, false, false),
        ]));
        assert_eq!(draft.changes(), [Change::order(MealId(2))]);

        // Meal 2 is gone from the menu.
        draft.rebase(&menu(vec![meal(1, MealKind::Main, false, true)]));
        assert!(draft.is_empty());
    }

    #[test]
    fn cost_of_a_swap_is_the_price_difference() {
        let mut cheap = meal(1, MealKind::Main, true, false);
        cheap.price = Money::from_hundredths(5000);
        let mut dear = meal(2, MealKind::Main, false, false);
        dear.price = Money::from_hundredths(5850);
        let menu = menu(vec![cheap, dear]);
        let mut draft = OrderDraft::new();

        draft.toggle(&menu, MealId(2)).unwrap();
        assert_eq!(draft.cost(&menu), Money::from_hundredths(850));

        // Swapping back cancels both changes.
        draft.toggle(&menu, MealId(1)).unwrap();
        assert!(draft.is_empty());
        assert_eq!(draft.cost(&menu), Money::ZERO);
    }
}
