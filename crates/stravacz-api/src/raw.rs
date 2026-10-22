//! The API's wire format and its conversion into [`crate::model`] types.
//!
//! Field types are inconsistent between canteens and versions (`"1"` vs `1`,
//! `null` vs `""`), so everything is read leniently as text first.

use std::collections::BTreeMap;

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::model::{
    Account, Allergen, Day, DayStatus, Meal, MealId, MealKind, Menu, Money, Restriction,
};

#[derive(Debug, Deserialize)]
pub(crate) struct LoginResponse {
    #[serde(deserialize_with = "text")]
    pub sid: String,
    #[serde(default, deserialize_with = "text")]
    pub s5url: String,
    #[serde(default)]
    pub uzivatel: RawAccount,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct RawAccount {
    #[serde(deserialize_with = "text")]
    konto: String,
    #[serde(deserialize_with = "text")]
    jmeno: String,
    #[serde(deserialize_with = "text")]
    email: String,
    #[serde(deserialize_with = "text")]
    mena: String,
    #[serde(rename = "nazevJidelny", deserialize_with = "text")]
    canteen_name: String,
}

impl RawAccount {
    pub fn into_account(self, username: &str, canteen_number: &str) -> Account {
        Account {
            username: username.to_owned(),
            full_name: non_empty(&self.jmeno).unwrap_or_else(|| username.to_owned()),
            email: non_empty(&self.email),
            canteen_number: canteen_number.to_owned(),
            canteen_name: non_empty(&self.canteen_name),
            balance: amount("konto", &self.konto),
            currency: non_empty(&self.mena).unwrap_or_else(|| "Kč".to_owned()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawMeal {
    #[serde(default, deserialize_with = "text")]
    datum: String,
    #[serde(default, deserialize_with = "text")]
    druh: String,
    #[serde(default, deserialize_with = "text")]
    druh_popis: String,
    #[serde(default, deserialize_with = "text")]
    nazev: String,
    #[serde(default, deserialize_with = "text")]
    cena: String,
    #[serde(default, deserialize_with = "text")]
    pocet: String,
    #[serde(default, deserialize_with = "text")]
    veta: String,
    #[serde(default, rename = "omezeniObj")]
    restrictions: RawRestrictions,
    #[serde(default, deserialize_with = "allergens")]
    alergeny: Vec<Allergen>,
    #[serde(default, rename = "casKonec", deserialize_with = "text")]
    deadline: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawRestrictions {
    /// Applies to the whole day, never to the meal.
    #[serde(deserialize_with = "text")]
    den: String,
    /// May the meal be ordered.
    #[serde(deserialize_with = "text")]
    obj: String,
    /// May an existing order be changed.
    #[serde(deserialize_with = "text")]
    zm: String,
}

/// Turn an `objednavky` response into a [`Menu`].
///
/// The response holds one `tableN` array per day. Rows that are placeholders
/// (days without service, unpublished slots) only contribute their day.
pub(crate) fn parse_menu(response: &Value) -> Menu {
    let mut days: BTreeMap<NaiveDate, Day> = BTreeMap::new();

    let tables = response
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| key.starts_with("table"))
        .filter_map(|(_, rows)| rows.as_array());

    for row in tables.flatten() {
        let raw = match RawMeal::deserialize(row) {
            Ok(raw) => raw,
            Err(err) => {
                tracing::warn!(%err, "skipping malformed menu row");
                continue;
            }
        };
        let Some(date) = parse_date(&raw.datum) else {
            tracing::warn!(datum = raw.datum, "skipping menu row with invalid date");
            continue;
        };

        let status = DayStatus::from_code(&raw.restrictions.den);
        let day = days.entry(date).or_insert_with(|| Day {
            date,
            status: DayStatus::Normal,
            meals: Vec::new(),
        });
        if status != DayStatus::Normal {
            day.status = status;
        }
        if day.status == DayStatus::NoService || is_placeholder(&raw.nazev, &raw.druh_popis) {
            continue;
        }

        if let Ok(id) = raw.veta.trim().parse() {
            day.meals.push(raw.into_meal(MealId(id), date));
        } else {
            tracing::warn!(veta = raw.veta, "skipping menu row with invalid id");
        }
    }

    Menu::new(days.into_values().collect())
}

impl RawMeal {
    fn into_meal(self, id: MealId, date: NaiveDate) -> Meal {
        Meal {
            id,
            date,
            kind: meal_kind(&self.druh, &self.druh_popis),
            name: self.nazev.trim().to_owned(),
            label: self.druh_popis.trim().to_owned(),
            price: amount("cena", &self.cena),
            ordered: self
                .pocet
                .trim()
                .parse::<f64>()
                .is_ok_and(|count| count >= 1.0),
            order_restriction: Restriction::from_code(&self.restrictions.obj),
            cancel_restriction: Restriction::from_code(&self.restrictions.zm),
            allergens: self.alergeny,
            deadline: self.deadline.parse::<NaiveDateTime>().ok(),
        }
    }
}

/// The balance that write endpoints report back in `konto`, if any.
pub(crate) fn balance(response: &Value) -> Option<Money> {
    let text = match response.get("konto")? {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    let balance = Money::parse(&text);
    if balance.is_none() && !text.trim().is_empty() {
        tracing::warn!(konto = text, "unreadable balance");
    }
    balance
}

/// Dates come as `15.09.2026`; the separator varies between versions.
fn parse_date(text: &str) -> Option<NaiveDate> {
    let mut parts = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(str::parse);
    let (day, month, year) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, month, day)
}

fn meal_kind(code: &str, label: &str) -> MealKind {
    let code = code.trim();
    if code.eq_ignore_ascii_case("PO") {
        return MealKind::Soup;
    }
    if code
        .strip_prefix(['O', 'o'])
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    {
        return MealKind::Main;
    }
    let label = label.to_lowercase();
    if ["polévka", "polevka", "soup"]
        .iter()
        .any(|w| label.contains(w))
    {
        MealKind::Soup
    } else if ["oběd", "obed", "lunch"].iter().any(|w| label.contains(w)) {
        MealKind::Main
    } else {
        MealKind::Other
    }
}

/// Unpublished slots carry the slot's label (`Oběd 2`, `Oběd 2 Rad 1`) as name.
fn is_placeholder(name: &str, label: &str) -> bool {
    const GENERIC: &[&str] = &[
        "oběd",
        "obed",
        "polévka",
        "polevka",
        "svačina",
        "svacina",
        "večeře",
        "vecere",
        "snídaně",
        "snidane",
        "přesnídávka",
        "presnidavka",
        "lunch",
        "soup",
        "dinner",
    ];

    let name = name.trim().to_lowercase();
    if name.is_empty() || name == label.trim().to_lowercase() {
        return true;
    }
    let mut words = name.split_whitespace();
    words.next().is_some_and(|first| GENERIC.contains(&first))
        && words.all(|word| word == "rad" || word.chars().all(|c| c.is_ascii_digit()))
}

/// An amount that may be missing (zero), but should never be garbled.
fn amount(field: &'static str, text: &str) -> Money {
    if text.trim().is_empty() {
        return Money::ZERO;
    }
    Money::parse(text).unwrap_or_else(|| {
        tracing::warn!(field, text, "unreadable amount, using 0");
        Money::ZERO
    })
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Accept a string, number, bool or `null` and read it as text.
fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(text) => text,
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// Allergens come as `[["01", "Lepek"], ...]`, sometimes as bare codes, or `null`.
fn allergens<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Allergen>, D::Error> {
    let items = Option::<Vec<Value>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(items
        .into_iter()
        .filter_map(|item| match item {
            Value::Array(pair) => {
                let mut fields = pair.into_iter().map(|v| match v {
                    Value::String(text) => text,
                    other => other.to_string(),
                });
                Some(Allergen {
                    code: fields.next()?,
                    name: fields.next().unwrap_or_default(),
                })
            }
            Value::String(code) => Some(Allergen {
                code,
                name: String::new(),
            }),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dates() {
        assert_eq!(
            parse_date("02.09.2026"),
            NaiveDate::from_ymd_opt(2026, 9, 2)
        );
        assert_eq!(parse_date("2/9/2026"), NaiveDate::from_ymd_opt(2026, 9, 2));
        assert_eq!(parse_date("31.02.2026"), None);
        assert_eq!(parse_date(""), None);
    }

    #[test]
    fn detects_placeholders() {
        assert!(is_placeholder("Oběd 2", "Oběd 2"));
        assert!(is_placeholder("Oběd 2 Rad 1", "Oběd 2"));
        assert!(is_placeholder("  ", "Polévka"));
        assert!(!is_placeholder("Kuřecí řízek, brambory", "Oběd 1"));
        assert!(!is_placeholder("Polévka frankfurtská", "Polévka"));
    }

    #[test]
    fn classifies_meals() {
        assert_eq!(meal_kind("PO", ""), MealKind::Soup);
        assert_eq!(meal_kind("O2", ""), MealKind::Main);
        assert_eq!(meal_kind("X", "Oběd 3"), MealKind::Main);
        assert_eq!(meal_kind("SV", "Svačina"), MealKind::Other);
    }
}
