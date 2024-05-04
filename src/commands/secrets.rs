use clap::{Parser, Subcommand};

use crate::{secrets::VaultTrait, AwsSecretVault, Config};

#[derive(Parser)]
pub struct VaultCli {
    /// Name of the vault to use. If not specified the default one will be used
    #[arg(name = "--vault")]
    vault_name: Option<String>,
    #[command(subcommand)]
    command: VaultCommands,
}

#[derive(Subcommand)]
enum VaultCommands {
    /// Create a new vault
    Create {
        /// Name of the vault, will be used later to reference this vault
        name: String,
        /// Whether or not to set this vault as the default
        #[arg(long, default_value_t = false)]
        set_default: bool,
        #[command(subcommand)]
        provider: SecretProvider,
    },
    /// Set a secret in the specified vault
    Set {
        /// Key of the secret, in the format of a `.` separated path
        key: String,
        /// Value of the secret
        value: String,
    },
    /// Get a secret in the specified vault
    Get {
        /// Key of the secret, in the format of a `.` separated path
        key: String,
    },
    /// Remove a secret in the specified vault
    Remove {
        /// Key of the secret, in the format of a `.` separated path
        key: String,
    },
    /// Set the specified vault as default
    SetDefault,
    /// Prints a tree with all secrets contained in the specified vault
    GetAll,
}

#[derive(clap::Subcommand)]
enum SecretProvider {
    /// Use AWS secret manager as a provider
    #[command(name = "--aws")]
    AwsSecretManager { secret_name: String },
}

pub async fn handle_secrets(
    mut config: Config,
    cli: VaultCli,
) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        VaultCommands::Create {
            name,
            provider,
            set_default,
        } => {
            handle_create_secret(config, name, provider, set_default).await?;
        }
        update_commands => {
            let vault_name = config.get_vault_name(cli.vault_name.as_deref())?;
            match update_commands {
                VaultCommands::Set { key, value } => {
                    let key_ref = key.parse()?;
                    let replaced = config.set_secret(&vault_name, key_ref, value)?;
                    config.save().await?;
                    if let Some(replaced) = replaced {
                        println!("Set value for {}, previous value was {}", key, replaced);
                    }
                }
                VaultCommands::Get { key } => {
                    let key_ref = key.parse()?;
                    let value = config.get_secret(&vault_name, &key_ref)?;
                    if let Some(value) = value {
                        println!("{value}");
                    } else {
                        let data = config
                            .get_all_secrets(&&vault_name, &key_ref.path.join(key_ref.key))?;
                        if !data.is_empty() {
                            for (key, value) in data {
                                println!("{}: {}", key, value);
                            }
                        } else {
                            Err(format!("Key {} not found", key))?
                        }
                    }
                }
                VaultCommands::Remove { key } => {
                    let key_ref = key.parse()?;
                    let replaced = config.remove_secret(&vault_name, &key_ref)?;
                    if let Some(replaced) = replaced {
                        config.save().await?;
                        println!("Removed {}, value was {}", key, replaced);
                    } else {
                        Err(format!("{} not found", key))?;
                    }
                }
                VaultCommands::SetDefault => {
                    config.set_default_vault(vault_name);
                }
                VaultCommands::GetAll => {
                    println!("{}", config.display_vault(&vault_name)?);
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}

async fn handle_create_secret(
    mut config: Config,
    name: String,
    provider: SecretProvider,
    set_default: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match provider {
        SecretProvider::AwsSecretManager { secret_name } => {
            println!(
                "Creating vault {} with AWS Secrets Manager and secret name {}",
                name, secret_name
            );
            let vault = AwsSecretVault::create(secret_name).await?;
            config
                .add_vault(name.clone(), vault.into_vault_kind())
                .await?;
        }
    }
    if set_default {
        config.set_default_vault(name.clone());
    }
    config.save().await?;
    Ok(())
}
