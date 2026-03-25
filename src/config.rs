use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub calendar_name: Option<String>,
    pub no_browser: Option<bool>,
    pub properties: Option<HashMap<String, Vec<String>>>,
    pub check: Option<HashMap<String, Vec<String>>>,
}

impl Config {
    pub fn load() -> Self {
        let path = match config_path() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Warning: could not determine config path: {e}");
                return Self::default();
            }
        };
        if !path.exists() {
            return Self::default();
        }
        let config: Self = match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
                eprintln!("Warning: failed to parse {}: {e}", path.display());
                Self::default()
            }),
            Err(e) => {
                eprintln!("Warning: failed to read {}: {e}", path.display());
                Self::default()
            }
        };

        if let Some(properties) = &config.properties {
            for (key, values) in properties {
                if values.is_empty() {
                    eprintln!("Warning: property '{key}' in config.toml has an empty list of allowed values");
                }
            }
        }

        if let Some(check) = &config.check {
            let properties = config.properties.as_ref();
            for (_type_name, required_keys) in check {
                for key in required_keys {
                    if !properties.is_some_and(|p| p.contains_key(key)) {
                        eprintln!("Warning: [check] references property '{key}' which is not defined in [properties]");
                    }
                }
            }
        }

        config
    }

    pub fn no_browser(&self) -> bool {
        self.no_browser.unwrap_or(false)
    }
}

pub fn config_dir() -> Result<PathBuf> {
    let mut dir = dirs::home_dir()
        .context("Could not determine home directory")?;
    dir.push(".config");
    dir.push("rscalendar");
    std::fs::create_dir_all(&dir)
        .context("Could not create config directory")?;
    Ok(dir)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn credentials_path() -> Result<PathBuf> {
    let path = config_dir()?.join("credentials.json");
    if !path.exists() {
        eprintln!("Error: credentials.json not found at {}", path.display());
        eprintln!("Download OAuth2 credentials from Google Cloud Console and place them there.");
        std::process::exit(1);
    }
    Ok(path)
}

pub fn token_cache_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("token_cache.json"))
}
