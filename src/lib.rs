pub mod commands;
mod config;
mod secrets;
use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
};

pub use config::Configuration;
pub use secrets::AwsSecretVault;
use secrets::{VaultKind, VaultTrait};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRef {
    path: PathBuf,
    key: String,
}

impl Display for KeyRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dotted: String = self
            .path
            .display()
            .to_string()
            .replace(std::path::MAIN_SEPARATOR, ".");
        dotted = dotted
            .strip_prefix(".")
            .map(|v| v.to_string())
            .unwrap_or_else(|| dotted.to_string());
        write!(f, "{}.{}", dotted, self.key)
    }
}

pub struct Config {
    path: PathBuf,
    config: Configuration<ConfigValue>,
    vaults: HashMap<String, Box<dyn VaultTrait>>,
    default_vault: Option<String>,
    context: PathBuf,
    updated: Vec<String>,
}

impl Config {
    pub async fn load(path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let res = if !path.exists() {
            ConfigFileData::default()
        } else {
            let res = serde_json::from_reader(std::fs::File::open(&path)?).map_err(|e| {
                format!("Failed to parse config file.\nif you used a previous version of secrets-manager, run `secrets-manager config migrate`\n {}", e)
            })?;
            res
        };
        let len = res.secrets.len();
        let mut vaults = HashMap::with_capacity(len);
        for (name, kind) in res.secrets.into_iter() {
            vaults.insert(name, kind.into_vault().await?);
        }
        Ok(Self {
            path,
            default_vault: res.default_secret,
            config: res.config,
            vaults,
            context: res.context,
            updated: Vec::new(),
        })
    }

    pub fn get(&self, key_ref: &KeyRef) -> Option<&str> {
        let value = self.config.get(&key_ref);
        let value = if let Some(value) = value {
            match value {
                ConfigValue::Secret(name, key_ref) => self.resolve_secret(name, key_ref),
                ConfigValue::Value(value) => Some(value.as_str()),
            }
        } else {
            return None;
        };
        value
    }

    fn resolve_secret(&self, name: &str, key_ref: &KeyRef) -> Option<&str> {
        let Some(vault) = self.vaults.get(name) else {
            return None;
        };
        vault.get().get(&key_ref).map(|v| v.as_str())
    }

    pub fn get_all(&self, key: &Path) -> HashMap<&str, String> {
        self.config
            .get_all(key)
            .into_iter()
            .map(|(key, v)| {
                let value: String = match v {
                    ConfigValue::Secret(ref name, ref key_ref) => self
                        .resolve_secret(name, key_ref)
                        .map(|v| v.to_string())
                        .unwrap_or(v.to_string()),
                    ConfigValue::Value(v) => v.to_owned(),
                };
                (key.as_str(), value)
            })
            .collect()
    }

    pub fn set(
        &mut self,
        key_ref: KeyRef,
        value: ConfigValue,
    ) -> Result<Option<ConfigValue>, ConfigError> {
        let value = match value {
            ConfigValue::Value(_) => value,
            ConfigValue::Secret(ref name, ref secret_ref)
                if self.get_secret(name, secret_ref)?.is_none() =>
            {
                return Err(ConfigError::SecretNotFound(
                    name.to_string(),
                    secret_ref.to_string(),
                ));
            }
            v => v,
        };
        let res = self.config.set(key_ref, value);
        Ok(res)
    }

    pub fn remove(&mut self, key_ref: &KeyRef) -> Option<ConfigValue> {
        self.config.remove(key_ref)
    }

    pub async fn save(mut self) -> Result<(), Box<dyn std::error::Error>> {
        for name in self.updated {
            self.vaults
                .get_mut(&name)
                .expect("Vault exists if it was updated")
                .save()
                .await?;
        }
        let mut secrets = HashMap::new();
        for (name, v) in self.vaults {
            let kind = v.into_vault_kind();
            secrets.insert(name, kind);
        }
        let data = ConfigFileData {
            context: self.context,
            config: self.config,
            secrets,
            default_secret: self.default_vault,
        };
        let file = std::fs::File::create(&self.path)?;
        serde_json::to_writer_pretty(file, &data)?;
        Ok(())
    }

    pub fn get_vault_name(&self, name: Option<&str>) -> Result<String, ConfigError> {
        let name = self
            .default_vault
            .as_deref()
            .or(name)
            .ok_or(ConfigError::VaultNotSpecified)?;
        Ok(name.to_string())
    }
    pub fn set_default_vault(&mut self, name: String) {
        self.default_vault = Some(name);
    }
    pub fn set_current_context(&mut self, name: PathBuf) {
        self.context = name;
    }
    pub fn get_current_context(&self) -> &Path {
        &self.context
    }

    pub fn set_secret(
        &mut self,
        name: &str,
        key: KeyRef,
        value: String,
    ) -> Result<Option<String>, ConfigError> {
        let vault = self.get_vault_mut(name)?;
        let replaced = vault.get_mut().set(key, value);
        Ok(replaced)
    }

    pub fn remove_secret(
        &mut self,
        name: &str,
        key: &KeyRef,
    ) -> Result<Option<String>, ConfigError> {
        let vault = self.get_vault_mut(name)?;
        let removed = vault.get_mut().remove(key);
        Ok(removed)
    }
    pub fn get_secret(&self, name: &str, key_ref: &KeyRef) -> Result<Option<&str>, ConfigError> {
        let vault = self
            .vaults
            .get(name)
            .ok_or_else(|| ConfigError::VaultNotFound(name.to_string()))?;
        let res = vault.get().get(key_ref);
        Ok(res.map(|x| x.as_str()))
    }

    pub fn get_all_secrets(
        &self,
        name: &str,
        path: &Path,
    ) -> Result<HashMap<&String, &String>, ConfigError> {
        let vault = self.get_vault(name)?;
        let res = vault.get().get_all(path);
        Ok(res)
    }

    pub async fn add_vault(&mut self, name: String, vault: VaultKind) -> Result<(), ConfigError> {
        if self.vault_exists(&name) {
            return Err(ConfigError::VaultAlreadyExists);
        }
        let vault = vault.into_vault().await?;
        self.vaults.insert(name.clone(), vault);
        self.updated.push(name);
        Ok(())
    }
    pub fn display(&self) -> String {
        self.config.display()
    }

    pub fn display_vault(&self, name: &str) -> Result<String, ConfigError> {
        let vault = self.get_vault(name)?;
        Ok(vault.get().display())
    }

    pub fn vault_exists(&self, name: &str) -> bool {
        self.vaults.contains_key(name)
    }
    fn get_vault(&self, name: &str) -> Result<&Box<dyn VaultTrait>, ConfigError> {
        let vault = self
            .vaults
            .get(name)
            .ok_or_else(|| ConfigError::VaultNotFound(name.to_string()))?;
        Ok(vault)
    }

    fn get_vault_mut(&mut self, name: &str) -> Result<&mut Box<dyn VaultTrait>, ConfigError> {
        let vault = self
            .vaults
            .get_mut(name)
            .ok_or_else(|| ConfigError::VaultNotFound(name.to_string()))?;
        Ok(vault)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFileData {
    config: Configuration<ConfigValue>,
    #[serde(default)]
    context: PathBuf,
    default_secret: Option<String>,
    secrets: HashMap<String, secrets::VaultKind>,
}

impl std::str::FromStr for KeyRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts: Vec<_> = s.split('.').collect();
        parts.insert(0, "/");
        let key = parts
            .pop()
            .ok_or_else(|| String::from("key cannot be empty"))?
            .to_string();
        let path = PathBuf::from_iter(parts);
        Ok(KeyRef { path, key })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Vault {0} not found")]
    VaultNotFound(String),
    #[error("Secret {0} not found for vault {1}")]
    SecretNotFound(String, String),
    #[error("vault name not specified, either pass --vault or set a default vault with `secrets-manager vault use`")]
    VaultNotSpecified,
    #[error("Vault already exists!")]
    VaultAlreadyExists,
    #[error(transparent)]
    Encoding(#[from] serde_json::Error),
    #[error(transparent)]
    VaultError(#[from] secrets::VaultError),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    Secret(String, KeyRef),
    Value(String),
}

impl ConfigValue {
    pub fn from_value(v: String) -> Self {
        ConfigValue::Value(v)
    }

    pub fn from_secret(name: String, v: String) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(ConfigValue::Secret(name, v.parse()?))
    }
}

impl Default for ConfigValue {
    fn default() -> Self {
        ConfigValue::Value(String::new())
    }
}

impl Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigValue::Secret(name, v) => {
                write!(f, "secret [{}::{}]", name, v.path.join(&v.key).display())
            }
            ConfigValue::Value(v) => write!(f, "{}", v),
        }
    }
}
