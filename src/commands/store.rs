use std::path::PathBuf;

use clap::{Parser, Subcommand};
use securestore::{KeySource, SecretsManager};

use crate::{ConfigValue, Configuration, SecretManager};

use super::{get_config_path, get_path};

#[derive(Parser)]
pub struct Store {
    #[arg(short, long)]
    /// Section of the config to extract, for example --path /dev would grab the <cwd>/dev path
    /// from the config
    path: Option<PathBuf>,
    #[arg(long)]
    /// base path of the config to extract, gets merged with --path
    cwd: Option<PathBuf>,
    /// Path to secrets-manager's config file, defaults to
    /// $XDG_CONFIG_DIR/secrets-manager/config.json
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(flatten)]
    key_source: KeyParam,
    #[command(subcommand)]
    command: StoreCommand,
}

#[derive(Subcommand)]
enum StoreCommand {
    Add {
        /// Where to save the encrypted file
        secret_path: PathBuf,
    },
    Get {
        /// Path to the encrypted json store
        secret_path: PathBuf,
    },
}

pub async fn handle_store(cli: Store) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = get_path(cli.cwd, cli.path)?;

    match cli.command {
        StoreCommand::Add { secret_path } => {
            let config_path = get_config_path(cli.config, "config.json")?;
            let config: Configuration<ConfigValue> =
                Configuration::from_path(config_path, cwd.clone())?;
            let key_source = cli.key_source.as_key_source();
            let mut secret_manager = if secret_path.exists() {
                SecretsManager::load(&secret_path, key_source)
            } else {
                SecretsManager::new(key_source)
            }?;
            let secret_data = config.get_values_for_cwd();
            secret_manager.set(
                &cwd.display().to_string(),
                serde_json::to_string(&secret_data)?,
            );
            secret_manager.save_as(&secret_path)?;
            println!("Saved to encrypted file");
        }
        StoreCommand::Get { secret_path } => {
            let secret_config_path = get_config_path(cli.config.clone(), "secret.json")?;
            let key_source = cli.key_source.as_key_source();
            let secret_manager = SecretsManager::load(secret_path, key_source)?;
            let secret_data = secret_manager.get(&cwd.display().to_string())?;
            let secrets = SecretManager::from_config(secret_config_path, PathBuf::new()).await?;
            let mut config = Configuration::new(cwd.clone());
            config.add_from_str(&secret_data, cwd)?;
            let data = secrets.resolve(&config);
            for (key, value) in data {
                println!("{key}: {value}")
            }
        }
    }

    Ok(())
}

#[derive(Debug, clap::Args)]
#[group(required = true, multiple = false)]
pub struct KeyParam {
    /// Path to private key.
    #[clap(short, long)]
    key_path: Option<PathBuf>,
    /// Password.
    #[clap(long)]
    password: Option<String>,
}

impl KeyParam {
    fn as_key_source(&self) -> KeySource<'_> {
        match (self.key_path.as_ref(), self.password.as_ref()) {
            (None, Some(ref password)) => KeySource::Password(password),
            (Some(key_path), None) => KeySource::Path(key_path),
            _ => unreachable!(),
        }
    }
}
