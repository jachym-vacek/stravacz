//! How API states are put into words, so every frontend says the same thing.

use stravacz_api::{DayStatus, Meal, Restriction};

/// Why the canteen does not allow an action, or `None` if it does.
#[must_use]
pub fn restriction_reason(restriction: &Restriction) -> Option<&'static str> {
    match restriction {
        Restriction::None => None,
        Restriction::Closed => Some("the deadline has passed"),
        Restriction::NotSeparately => Some("it cannot be ordered separately"),
        Restriction::NoService => Some("the canteen is closed that day"),
        Restriction::Other(_) => Some("the canteen does not allow it"),
    }
}

/// A short note about the day, if there is anything unusual about it.
#[must_use]
pub fn day_note(status: &DayStatus) -> Option<&'static str> {
    match status {
        DayStatus::Normal | DayStatus::Other(_) => None,
        DayStatus::NotAutoOrdered => Some("not ordered automatically"),
        DayStatus::NoService => Some("no service"),
        DayStatus::Closed => Some("ordering closed"),
    }
}

/// A short tag saying why the meal cannot be changed from its current state,
/// if that is worth pointing out.
#[must_use]
pub fn meal_note(meal: &Meal) -> Option<&'static str> {
    if meal.ordered {
        return (!meal.can_cancel()).then_some("locked");
    }
    match &meal.order_restriction {
        Restriction::None => None,
        Restriction::NotSeparately => Some("comes with lunch"),
        Restriction::Closed => Some("closed"),
        Restriction::NoService | Restriction::Other(_) => Some("unavailable"),
    }
}
