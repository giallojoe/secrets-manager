mod config;
mod secrets;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub use config::*;
use platform_dirs::AppDirs;
pub use secrets::*;
use serde::Deserialize;

use crate::{
    secrets::VaultTrait, AwsSecretVault, Config, ConfigFileData, ConfigValue, Configuration, KeyRef,
};

pub fn parse_key_ref(key: &str, path: &Path) -> Result<KeyRef, Box<dyn std::error::Error>> {
    let mut res: KeyRef = key.parse()?;
    res.path = PathBuf::from("/").join(path).join(res.path);
    Ok(res)
}

pub fn get_config_path(
    config_dir: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = match (config_dir, config_path) {
        (Some(dir), Some(file_path)) => dir.join(file_path),
        (Some(dir), None) => dir.join(PathBuf::from("config.json")),
        (None, file_path) => {
            let config_dir = get_default_config_dir()?;
            config_dir.join(file_path.unwrap_or_else(|| PathBuf::from("config.json")))
        }
    };
    Ok(path)
}

pub fn init_config(config_file: impl AsRef<Path>) -> Result<(), std::io::Error> {
    if config_file.as_ref().exists() {
        Ok(())
    } else {
        std::fs::create_dir_all(
            config_file
                .as_ref()
                .parent()
                .expect("config file should have a parent dir"),
        )?;
        let file = std::fs::File::create(config_file.as_ref())?;
        serde_json::to_writer_pretty(file, &ConfigFileData::default())?;
        Ok(())
    }
}

fn get_default_config_dir() -> Result<PathBuf, String> {
    let app_dir = AppDirs::new(Some("secrets-manager"), true)
        .ok_or_else(|| String::from("Cannot find config base path"))?;
    Ok(app_dir.config_dir)
}

pub fn get_path(config: &Config, base: Option<PathBuf>) -> Result<PathBuf, std::io::Error> {
    let cwd = base.map_or_else(|| std::env::current_dir(), Ok)?;
    let path = &config.context;
    let path = if path.is_absolute() {
        path.strip_prefix("/").unwrap()
    } else {
        path
    };
    let cwd = PathBuf::from("/")
        .join(PathBuf::from(cwd.file_name().unwrap_or_default()))
        .join(path);
    Ok(cwd)
}

pub async fn handle_config_migration(
    old_config: &Path,
    new_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_file = std::fs::File::open(old_config)?;
    let secret_file = std::fs::File::open(
        old_config
            .parent()
            .expect("config_file to have a parent directory")
            .join("secret.json"),
    )?;
    let configuration: Configuration<OldConfigValue> = serde_json::from_reader(config_file)?;
    let mut new_config = Configuration::new();
    let kind: HashMap<String, String> = serde_json::from_reader(secret_file)?;
    let secret_name = kind.get("name").expect("Secret name to be present");
    configuration
        .keys(PathBuf::from("/"))
        .try_for_each(|key| -> Option<()> {
            let value = configuration.get(&key)?;
            let new_value = match value {
                OldConfigValue::Value(v) => ConfigValue::from_value(v.clone()),
                OldConfigValue::Secret { key, path } => ConfigValue::Secret(
                    secret_name.clone(),
                    KeyRef {
                        key: key.clone(),
                        path: path.to_path_buf(),
                    },
                ),
            };
            new_config.set(key, new_value);
            Some(())
        })
        .ok_or_else(|| format!("Could not migrate data, some keys are missing"))?;
    let vault = AwsSecretVault::create(secret_name.to_string()).await?;
    let mut vaults = HashMap::new();
    vaults.insert(
        secret_name.to_string(),
        Box::new(vault) as Box<dyn VaultTrait>,
    );
    let config = Config {
        path: new_path.clone(),
        config: new_config,
        vaults,
        default_vault: Some(secret_name.to_string()),
        context: PathBuf::new(),
        updated: Vec::new(),
    };
    config.save().await?;
    println!(
        "Configuration successfully migrated to {}",
        new_path.display()
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OldConfigValue {
    Secret { key: String, path: PathBuf },
    Value(String),
}
