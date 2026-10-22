//! Application logic shared by every Strava.cz frontend.
//!
//! Nothing here knows how it is displayed: a terminal or a graphical UI can
//! both drive an [`OrderDraft`], persist a [`Config`] and keep passwords in
//! the system [`secrets`] store.

mod config;
mod draft;
mod login;
pub mod secrets;
pub mod wording;

pub use config::{Config, ConfigError, ConfigStore, SavedLogin, project_dirs};
pub use draft::{OrderDraft, ToggleError};
pub use login::{LoginStoreError, forget_login, remember_login};
pub use stravacz_api as api;
