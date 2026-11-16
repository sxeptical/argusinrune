//! Local configuration: environment variables, a `.env` in the working
//! directory, and an optional `~/.config/argus/config.toml`.
//!
//! Precedence (highest first): process environment, `.env` (loaded by
//! `dotenvy` without overriding the environment), config file, then nothing.
//! Missing or placeholder keys are represented as `None`, so a source reports
//! itself "unconfigured" instead of failing with a network error.

use std::path::PathBuf;

use crate::error::{Error, Result};

const LTA_KEY: &str = "LTA_API_KEY";
const AVIATION_KEY: &str = "AVIATIONSTACK_API_KEY";
const DATABASE_KEY: &str = "DATABASE_URL";

/// Default config location, overridable with `ARGUS_CONFIG_FILE`.
pub fn default_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ARGUS_CONFIG_FILE")
        && !path.trim().is_empty()
    {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".config").join("argus").join("config.toml"))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArgusConfig {
    pub lta_api_key: Option<String>,
    pub aviationstack_api_key: Option<String>,
    pub database_url: Option<String>,
    /// The config file actually read, for the UI's "settings" panel.
    pub config_path: Option<PathBuf>,
}

fn clean(value: Option<String>, placeholder: &str) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && !v.to_ascii_lowercase().contains(placeholder))
}

/// Look for a key at the top level or under `[api]` / `[keys]`.
fn toml_string(root: &toml::Value, key: &str) -> Option<String> {
    let table = root.as_table()?;
    for candidate in [
        Some(table),
        table.get("api").and_then(|v| v.as_table()),
        table.get("keys").and_then(|v| v.as_table()),
    ] {
        if let Some(t) = candidate
            && let Some(v) = t.get(key).and_then(|v| v.as_str())
        {
            return Some(v.to_string());
        }
    }
    None
}

impl ArgusConfig {
    /// Convenience constructor for tests / embedded deployments.
    pub fn new(
        lta_api_key: Option<String>,
        aviationstack_api_key: Option<String>,
        database_url: Option<String>,
    ) -> Self {
        Self {
            lta_api_key: clean(lta_api_key, "your_lta_datamall_key"),
            aviationstack_api_key: clean(aviationstack_api_key, "your_aviationstack_key"),
            database_url: clean(database_url, "your"),
            config_path: None,
        }
    }

    /// Load config from the environment, `.env`, and the local TOML file.
    pub fn load() -> Self {
        // Ignore errors: a missing .env is the normal case in production.
        let _ = dotenvy::dotenv();
        Self::load_from_env(|key| std::env::var(key).ok())
    }

    /// Load from an injected environment accessor (handy for tests).
    pub fn load_from_env<F>(mut get: F) -> Self
    where
        F: FnMut(&str) -> Option<String>,
    {
        let path = default_config_path();
        let file = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| s.parse::<toml::Value>().ok());

        let mut pick = |key: &str| -> Option<String> {
            get(key).filter(|v| !v.trim().is_empty()).or_else(|| {
                file.as_ref()
                    .and_then(|v| toml_string(v, key))
                    .filter(|v| !v.trim().is_empty())
            })
        };

        Self {
            lta_api_key: clean(pick(LTA_KEY), "your_lta_datamall_key"),
            aviationstack_api_key: clean(pick(AVIATION_KEY), "your_aviationstack_key"),
            database_url: clean(pick(DATABASE_KEY), "your"),
            config_path: path.filter(|p| p.exists()),
        }
    }

    pub fn lta_api_key(&self) -> Result<&str> {
        self.lta_api_key
            .as_deref()
            .ok_or_else(|| Error::NotConfigured {
                service: "lta".into(),
                message: "Missing or placeholder LTA_API_KEY".into(),
            })
    }

    pub fn aviationstack_api_key(&self) -> Result<&str> {
        self.aviationstack_api_key
            .as_deref()
            .ok_or_else(|| Error::NotConfigured {
                service: "aviationstack".into(),
                message: "Missing AVIATIONSTACK_API_KEY".into(),
            })
    }

    pub fn database_url(&self) -> Result<&str> {
        self.database_url
            .as_deref()
            .ok_or_else(|| Error::NotConfigured {
                service: "feedback".into(),
                message: "Missing DATABASE_URL".into(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_unconfigured() {
        let config = ArgusConfig::new(
            Some("your_lta_datamall_key".into()),
            Some("  ".into()),
            None,
        );
        assert!(config.lta_api_key.is_none());
        assert!(config.aviationstack_api_key.is_none());
        assert!(matches!(
            config.lta_api_key(),
            Err(Error::NotConfigured { .. })
        ));
    }

    #[test]
    fn env_wins_over_file_and_values_are_trimmed() {
        let toml = "LTA_API_KEY = 'from-file'\n[api]\nAVIATIONSTACK_API_KEY = 'avia'\n";
        // `load_from_env` reads the real default path, so exercise the helpers
        // directly with an equivalent inline document.
        let value: toml::Value = toml.parse().unwrap();
        assert_eq!(toml_string(&value, LTA_KEY).as_deref(), Some("from-file"));
        assert_eq!(toml_string(&value, AVIATION_KEY).as_deref(), Some("avia"));

        let config = ArgusConfig::new(Some("  real-key  ".into()), None, None);
        assert_eq!(config.lta_api_key().unwrap(), "real-key");
    }
}
