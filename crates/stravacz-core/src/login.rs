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
/// A previously remembered password is removed. If the keyring fails, the
/// account is still saved, just without `remember_password`.
pub fn remember_login(
    store: &ConfigStore,
    mut login: SavedLogin,
    password: Option<&str>,
) -> Result<(), LoginStoreError> {
    let mut config = store.load().unwrap_or_default();

    if let Some(previous) = config.login.take().filter(|l| l.remember_password)
        && let Err(err) = secrets::forget_password(&previous)
    {
        tracing::warn!(%err, "could not remove the previous password");
    }

    let stored = password
        .map(|password| secrets::store_password(&login, password))
        .transpose();
    login.remember_password = matches!(stored, Ok(Some(())));

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
