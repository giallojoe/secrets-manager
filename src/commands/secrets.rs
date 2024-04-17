use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::SecretManager;

use super::{get_config_path, get_path};

#[derive(Parser)]
pub struct Secret {
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[arg(long)]
    path: Option<PathBuf>,
    #[command(subcommand)]
    command: SecretCommands,
}

#[derive(Subcommand)]
enum SecretCommands {
    Create { secret_name: String },
    Set { key: String, value: String },
    Get { key: Option<String> },
    Remove { key: String },
    GetAll,
}

pub async fn handle_secrets(cli: Secret) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = get_config_path(cli.config, "secret.json")?;
    let path = get_path(Some(PathBuf::from("/")), cli.path)?;
    if config_path.exists() {
        let mut manager = SecretManager::from_config(&config_path, path).await?;
        match cli.command {
            SecretCommands::Create { secret_name } => {
                if secret_name == manager.secret_name() {
                    return Err(format!(
                        "Trying to create a secret that already exists: {}",
                        secret_name
                    )
                    .into());
                }
            }
            SecretCommands::Set { key, value } => {
                let old_value = manager.set_value(key, value);
                manager.save().await?;
                if let Some(old_value) = old_value {
                    println!("Secret value overridden, was {}", old_value);
                } else {
                    println!("Set new value to secret");
                }
            }
            SecretCommands::Get { key } => {
                if let Some(key) = key {
                    let value = manager
                        .get_value(&key)
                        .ok_or_else(|| format!("Missing key {}", key))?;
                    println!("{}: {}", key, value);
                } else {
                    for (key, value) in manager.get_values_for_cwd() {
                        println!("{}: {}", key, value);
                    }
                }
            }
            SecretCommands::Remove { key } => {
                let res = manager.remove_value(&key);
                if let Some(res) = res {
                    println!("`{}` value removed, previous value was `{}`", key, res);
                } else {
                    println!("`{}` not found, nothing to do", key);
                }
                manager.save().await?;
            }
            SecretCommands::GetAll => {
                let data = manager.print_tree();
                println!("{}", data);
            }
        }
    } else {
        match cli.command {
            SecretCommands::Create { secret_name } => {
                SecretManager::create(secret_name.clone(), config_path).await?;
                tracing::info!("Created secret with name: {}", secret_name);
            }
            _ => {
                return Err(
                    format!("No Secret found in config, run `create` command first!").into(),
                );
            }
        }
    }
    Ok(())
}
