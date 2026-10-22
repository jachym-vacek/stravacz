//! Passwords in the operating system's credential store.
//!
//! These calls block (they talk to the Secret Service, Keychain or Credential
//! Manager), so async frontends should run them off the main thread.

use crate::SavedLogin;

const SERVICE: &str = "strava-cz";

#[derive(Debug, thiserror::Error)]
#[error("system keyring: {0}")]
pub struct SecretError(#[from] keyring::Error);

fn entry(login: &SavedLogin) -> Result<keyring::Entry, SecretError> {
    let user = format!("{}@{}", login.username, login.canteen);
    Ok(keyring::Entry::new(SERVICE, &user)?)
}

/// The stored password, or `None` if there is none.
pub fn load_password(login: &SavedLogin) -> Result<Option<String>, SecretError> {
    match entry(login)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn store_password(login: &SavedLogin, password: &str) -> Result<(), SecretError> {
    Ok(entry(login)?.set_password(password)?)
}

/// Remove the stored password; succeeds if there was none.
pub fn forget_password(login: &SavedLogin) -> Result<(), SecretError> {
    match entry(login)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.into()),
    }
}
