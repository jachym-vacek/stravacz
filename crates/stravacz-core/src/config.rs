use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Platform specific directories for config, data and logs.
#[must_use]
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("cz", "strava", "stravacz")
}

/// Persistent, non-secret settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The last account that logged in successfully.
    pub login: Option<SavedLogin>,
}

/// Who to log in as. The password lives in the system keyring, never here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedLogin {
    pub canteen: String,
    pub username: String,
    /// Whether the password is stored in the keyring.
    #[serde(default)]
    pub remember_password: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not determine the config directory")]
    NoConfigDir,
    #[error("could not access {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid config file {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("could not serialize the config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Reads and writes [`Config`] as TOML.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    /// The store at the platform's config location,
    /// e.g. `~/.config/stravacz/config.toml`.
    pub fn new() -> Result<Self, ConfigError> {
        let dirs = project_dirs().ok_or(ConfigError::NoConfigDir)?;
        Ok(Self::at(dirs.config_dir().join("config.toml")))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the config; a missing file yields the defaults.
    pub fn load(&self) -> Result<Config, ConfigError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Parse {
                path: self.path.clone(),
                source,
            }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(source) => Err(self.io_error(source)),
        }
    }

    pub fn save(&self, config: &Config) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(config)?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|source| self.io_error(source))?;
        }
        fs::write(&self.path, text).map_err(|source| self.io_error(source))
    }

    fn io_error(&self, source: io::Error) -> ConfigError {
        ConfigError::Io {
            path: self.path.clone(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let dir = std::env::temp_dir().join(format!("stravacz-core-test-{}", std::process::id()));
        let store = ConfigStore::at(dir.join("nested/config.toml"));

        assert_eq!(store.load().unwrap(), Config::default());

        let config = Config {
            login: Some(SavedLogin {
                canteen: "1234".into(),
                username: "jan.novak".into(),
                remember_password: true,
            }),
        };
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap(), config);

        fs::remove_dir_all(dir).unwrap();
    }
}
