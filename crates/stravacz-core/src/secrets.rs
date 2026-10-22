//! Passwords in the operating system's credential store.
//!
//! These calls block (they talk to the Secret Service, Keychain or Credential
//! Manager), so async frontends should run them off the main thread.

use crate::SavedLogin;

const SERVICE: &str = "strava-cz";

use std::sync::Arc;

use keyring_core::{CredentialStore, Entry, Error};

#[derive(Debug, thiserror::Error)]
#[error("system keyring: {0}")]
pub struct SecretError(#[from] Error);

fn entry(login: &SavedLogin) -> Result<Entry, SecretError> {
    let user = format!("{}@{}", login.username, login.canteen);
    Ok(store()?.build(SERVICE, &user, None)?)
}

/// The platform's credential store (the same ones the `keyring` crate uses).
fn store() -> Result<Arc<CredentialStore>, Error> {
    #[cfg(target_os = "macos")]
    return Ok(apple_native_keyring_store::keychain::Store::new()?);
    #[cfg(target_os = "windows")]
    return Ok(windows_native_keyring_store::Store::new()?);
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    return Ok(dbus_secret_service_keyring_store::Store::new()?);
    #[allow(unreachable_code)]
    Err(Error::NoDefaultStore)
}

/// The stored password, or `None` if there is none.
pub fn load_password(login: &SavedLogin) -> Result<Option<String>, SecretError> {
    match entry(login)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn store_password(login: &SavedLogin, password: &str) -> Result<(), SecretError> {
    Ok(entry(login)?.set_password(password)?)
}

/// Remove the stored password; succeeds if there was none.
pub fn forget_password(login: &SavedLogin) -> Result<(), SecretError> {
    match entry(login)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(err) => Err(err.into()),
    }
}
