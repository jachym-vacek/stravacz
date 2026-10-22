//! Async client for the [Strava.cz](https://app.strava.cz) canteen ordering system.
//!
//! Strava.cz has no public API; this crate speaks the same JSON endpoints the
//! official web app uses. It is UI-agnostic, so any frontend can build on it.
//!
//! ```no_run
//! # async fn example() -> stravacz_api::Result<()> {
//! use stravacz_api::{Change, Client, Credentials};
//!
//! let client = Client::new()?;
//! let session = client
//!     .login(&Credentials::new("1234", "jan.novak", "secret"))
//!     .await?;
//!
//! let menu = session.menu().await?;
//! if let Some(meal) = menu.meals().find(|meal| meal.can_order()) {
//!     session.submit(&[Change::order(meal.id)]).await?;
//! }
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod model;
mod raw;

#[cfg(feature = "mock")]
pub mod mock;

pub use client::{Client, Credentials, DEFAULT_BASE_URL, Session, Submission};
pub use error::{Error, Result};
pub use model::{
    Account, Allergen, Change, Day, DayStatus, Meal, MealId, MealKind, Menu, Money, Restriction,
};
