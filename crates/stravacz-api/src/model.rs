//! Domain types, decoupled from the API's wire format.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub};

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Serialize, Serializer};

/// Identifies a meal on the menu (the API calls it `veta`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct MealId(pub u32);

impl fmt::Display for MealId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An amount of money, stored exactly in hundredths of the currency unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_hundredths(hundredths: i64) -> Self {
        Self(hundredths)
    }

    #[must_use]
    pub const fn hundredths(self) -> i64 {
        self.0
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Parse amounts as the API sends them: `"512.50"`, `"50"`, `"-3,5"`.
    ///
    /// A leading `+` and spaces as thousands separators (`"1 234,50"`) are
    /// accepted too.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(&text)),
        };
        let (whole, fraction) = digits.split_once(['.', ',']).unwrap_or((digits, ""));
        if whole.is_empty() && fraction.is_empty() {
            return None;
        }
        if ![whole, fraction]
            .iter()
            .all(|part| part.chars().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        let whole: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().ok()?
        };
        // Only the first two decimals matter; pad "5" to "50".
        let cents: i64 = format!("{fraction:0<2}")[..2].parse().ok()?;
        let value = whole.checked_mul(100)?.checked_add(cents)?;
        Some(Self(if negative { -value } else { value }))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

/// Serialized as a plain number, e.g. `512.5`.
impl Serialize for Money {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[allow(clippy::cast_precision_loss)] // Exact for any realistic balance.
        serializer.serialize_f64(self.0 as f64 / 100.0)
    }
}

impl Add for Money {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Money {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Neg for Money {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

/// The logged-in account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    pub canteen_number: String,
    pub canteen_name: Option<String>,
    pub balance: Money,
    pub currency: String,
}

/// What kind of item a meal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MealKind {
    Soup,
    /// A main course. Usually only one can be ordered per day.
    Main,
    Other,
}

/// Why a meal cannot be ordered or cancelled.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Restriction {
    /// The action is allowed.
    None,
    /// The deadline has passed.
    Closed,
    /// The item comes with another one and cannot be ordered on its own.
    NotSeparately,
    /// The canteen does not cook on this day.
    NoService,
    /// A code this crate does not know.
    Other(String),
}

impl Restriction {
    pub(crate) fn from_code(code: &str) -> Self {
        match code.trim().to_ascii_uppercase().as_str() {
            "" => Self::None,
            "C" => Self::Closed,
            "I" => Self::NotSeparately,
            "V" => Self::NoService,
            other => Self::Other(other.to_owned()),
        }
    }

    #[must_use]
    pub fn is_allowed(&self) -> bool {
        *self == Self::None
    }

    /// A human readable reason, or `None` if the action is allowed.
    #[must_use]
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Closed => Some("the deadline has passed"),
            Self::NotSeparately => Some("it cannot be ordered separately"),
            Self::NoService => Some("the canteen is closed that day"),
            Self::Other(_) => Some("the canteen does not allow it"),
        }
    }
}

/// What the canteen says about a whole day.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayStatus {
    Normal,
    /// The standing order skips this day; meals can still be ordered by hand.
    NotAutoOrdered,
    /// The canteen does not cook.
    NoService,
    /// Ordering for this day is closed.
    Closed,
    Other(String),
}

impl DayStatus {
    pub(crate) fn from_code(code: &str) -> Self {
        match code.trim().to_ascii_uppercase().as_str() {
            "" => Self::Normal,
            "T" => Self::NotAutoOrdered,
            "VP" => Self::NoService,
            "CO" => Self::Closed,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// An allergen, e.g. `01` – *Obiloviny obsahující lepek*.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Allergen {
    pub code: String,
    pub name: String,
}

/// One item on the menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Meal {
    pub id: MealId,
    pub date: NaiveDate,
    pub kind: MealKind,
    /// The canteen's label for the slot, e.g. `Oběd 1` or `Polévka`.
    pub label: String,
    pub name: String,
    pub price: Money,
    pub ordered: bool,
    pub order_restriction: Restriction,
    pub cancel_restriction: Restriction,
    pub allergens: Vec<Allergen>,
    /// Until when the order can be changed.
    pub deadline: Option<NaiveDateTime>,
}

impl Meal {
    #[must_use]
    pub fn can_order(&self) -> bool {
        self.order_restriction.is_allowed()
    }

    #[must_use]
    pub fn can_cancel(&self) -> bool {
        self.cancel_restriction.is_allowed()
    }

    /// Whether the canteen allows changing this meal to `ordered`.
    #[must_use]
    pub fn can_set(&self, ordered: bool) -> bool {
        ordered == self.ordered
            || if ordered {
                self.can_order()
            } else {
                self.can_cancel()
            }
    }
}

/// A day on the menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    pub status: DayStatus,
    pub meals: Vec<Meal>,
}

impl Day {
    #[must_use]
    pub fn is_service_day(&self) -> bool {
        self.status != DayStatus::NoService && !self.meals.is_empty()
    }
}

/// The published menu, sorted by date.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Menu {
    days: Vec<Day>,
}

impl Menu {
    #[must_use]
    pub fn new(mut days: Vec<Day>) -> Self {
        days.sort_by_key(|day| day.date);
        Self { days }
    }

    #[must_use]
    pub fn days(&self) -> &[Day] {
        &self.days
    }

    #[must_use]
    pub fn day(&self, date: NaiveDate) -> Option<&Day> {
        self.days
            .binary_search_by_key(&date, |day| day.date)
            .ok()
            .map(|index| &self.days[index])
    }

    pub fn meals(&self) -> impl Iterator<Item = &Meal> {
        self.days.iter().flat_map(|day| &day.meals)
    }

    #[must_use]
    pub fn meal(&self, id: MealId) -> Option<&Meal> {
        self.meals().find(|meal| meal.id == id)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }
}

/// A requested change to an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Change {
    pub meal: MealId,
    pub ordered: bool,
}

impl Change {
    #[must_use]
    pub const fn order(meal: MealId) -> Self {
        Self {
            meal,
            ordered: true,
        }
    }

    #[must_use]
    pub const fn cancel(meal: MealId) -> Self {
        Self {
            meal,
            ordered: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_money() {
        assert_eq!(Money::parse("512.50"), Some(Money(51250)));
        assert_eq!(Money::parse("50"), Some(Money(5000)));
        assert_eq!(Money::parse("-3,5"), Some(Money(-350)));
        assert_eq!(Money::parse(" 0 "), Some(Money::ZERO));
        assert_eq!(Money::parse("1.999"), Some(Money(199)));
        assert_eq!(Money::parse("1 234,50"), Some(Money(123_450)));
        assert_eq!(Money::parse("1\u{a0}234"), Some(Money(123_400)));
        assert_eq!(Money::parse("+5"), Some(Money(500)));
        assert_eq!(Money::parse("--5"), None);
        assert_eq!(Money::parse("+-5"), None);
        assert_eq!(Money::parse(""), None);
        assert_eq!(Money::parse("abc"), None);
    }

    #[test]
    fn formats_money() {
        assert_eq!(Money(51250).to_string(), "512.50");
        assert_eq!(Money(-5).to_string(), "-0.05");
    }
}
