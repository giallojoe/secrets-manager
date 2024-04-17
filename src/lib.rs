pub mod commands;
mod config;
mod secrets;
use std::{fmt::Display, path::PathBuf};

pub use config::Configuration;
pub use secrets::SecretManager;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    Secret { path: PathBuf, key: String },
    Value(String),
}

impl Default for ConfigValue {
    fn default() -> Self {
        ConfigValue::Value(String::new())
    }
}

impl Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigValue::Secret { path, key } => write!(f, "secret:{}", path.join(key).display()),
            ConfigValue::Value(v) => write!(f, "{}", v),
        }
    }
}
