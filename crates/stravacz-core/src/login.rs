//! Remembering who logs in, across config file and keyring.
//!
//! Like [`secrets`](crate::secrets), these calls block.

use crate::secrets::{self, SecretError};
use crate::{ConfigError, ConfigStore, SavedLogin};

#[derive(Debug, thiserror::Error)]
pub enum LoginStoreError {
    #[error("password not saved: {0}")]
    Secret(#[from] SecretError),
    #[error("settings not saved: {0}")]
    Config(#[from] ConfigError),
}

/// Make `login` the remembered account, storing `password` in the keyring if given.
///
/// A previously remembered password is removed once it is no longer needed.
/// If the keyring fails, the account is still saved, just without
/// `remember_password` (unless an earlier password for the same account is
/// still stored). An unreadable config file is reported, not overwritten.
pub fn remember_login(
    store: &ConfigStore,
    mut login: SavedLogin,
    password: Option<&str>,
) -> Result<(), LoginStoreError> {
    let mut config = store.load()?;
    let previous = config.login.take().filter(|l| l.remember_password);

    // Store the new password before touching the old one, so a failing
    // keyring never leaves the account without a working password.
    let stored = password
        .map(|password| secrets::store_password(&login, password))
        .transpose();
    login.remember_password = matches!(stored, Ok(Some(())));

    if let Some(previous) = previous {
        if !previous.is_same_account(&login) || password.is_none() {
            if let Err(err) = secrets::forget_password(&previous) {
                tracing::warn!(%err, "could not remove the previous password");
            }
        } else if stored.is_err() {
            // The old entry for this account is untouched; keep using it.
            login.remember_password = true;
        }
    }

    config.login = Some(login);
    store.save(&config)?;
    stored.map(drop).map_err(Into::into)
}

/// Forget the remembered account and its password; returns who it was.
pub fn forget_login(store: &ConfigStore) -> Result<Option<SavedLogin>, LoginStoreError> {
    let mut config = store.load()?;
    let Some(login) = config.login.take() else {
        return Ok(None);
    };
    if login.remember_password {
        secrets::forget_password(&login)?;
    }
    store.save(&config)?;
    Ok(Some(login))
}
