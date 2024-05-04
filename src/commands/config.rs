use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};

use crate::{Config, KeyRef};
use is_terminal::IsTerminal as _;

use crate::{ConfigValue, Configuration};

use super::{get_path, parse_key_ref};

#[derive(clap::ValueEnum, Default, Clone)]
pub enum Format {
    #[value(name = "env")]
    #[default]
    EnvFile,
    #[value(name = "json")]
    Json,
}

#[derive(Parser)]
pub struct ConfigCLI {
    /// Directory base, defaults to the base name of the current working directory,
    /// e.g if the path is /home/joe/work/test-dir, cwd will be test-dir
    #[arg(long)]
    cwd: Option<PathBuf>,
    /// keys can be in the format path.to.secret.key
    #[command(subcommand)]
    command: ConfigCommands,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Gets all keys for current context
    Get {
        /// if set, it will only return the specified key, if it exists.
        /// Key can be in the form of a `.` separated path
        key: Option<String>,
    },
    /// Sets/adds the specified key to the current context
    /// value can either be `--value <hardcoded value> or --secret <secret key>`
    Set {
        /// Key can be in the form of a `.` separated path
        key: String,
        #[clap(flatten)]
        value: ValueInput,
    },
    /// Deletes the specified key from the current context
    Remove { key: String },
    ///Prints a tree structure of all keys for all bases
    GetAll,
    /// import from env file
    Import { file: PathBuf },
    /// Export config in either json or dotenv format
    Export {
        #[arg(short, long)]
        format: Format,
    },
}

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct ValueInput {
    /// Hardcoded value to set the key to
    #[arg(long)]
    value: Option<String>,
    #[command(flatten)]
    secret: Option<SecretInput>,
}

#[derive(clap::Args)]
struct SecretInput {
    /// Name of the vault that contains the secret
    name: String,
    /// key of the secret, in the format of a `.` separated path
    secret: String,
}

pub async fn handle_config(
    mut config: Config,
    cli: ConfigCLI,
) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        ConfigCommands::Get { key } => {
            let path = get_path(&config, cli.cwd)?;
            let key = key.unwrap_or("".to_string());
            let key_ref = parse_key_ref(key.as_str(), &path)?;
            print_config(&config, &key_ref)?;
        }
        ConfigCommands::Set { key, value } => {
            let path = get_path(&config, cli.cwd)?;
            let key_ref = parse_key_ref(&key, &path)?;
            let value = match (value.value, value.secret) {
                (Some(v), None) => ConfigValue::from_value(v),
                (None, Some(secret)) => ConfigValue::from_secret(secret.name, secret.secret)?,
                _ => unreachable!(),
            };
            let display_key = key_ref.to_string();
            if let Some(replaced) = config.set(key_ref, value)? {
                println!(
                    "{} value set successfully, previous value was {}",
                    display_key, replaced
                );
            } else {
                println!("{} value set successfully", display_key);
            }
        }
        ConfigCommands::Remove { key } => {
            let path = get_path(&config, cli.cwd)?;
            let key_ref = parse_key_ref(&key, &path)?;
            let Some(removed) = config.remove(&key_ref) else {
                return Err(format!("{} not found", key_ref).into());
            };
            config.save().await?;
            println!(
                "{} removed successfully, previous value was {}",
                key_ref, removed
            );
        }
        ConfigCommands::GetAll => {
            println!("{}", config.display());
        }
        ConfigCommands::Import { file } => {
            let path = get_path(&config, cli.cwd)?;
            import_config(config, &path, file).await?;
        }
        ConfigCommands::Export { format } => {
            let path = get_path(&config, cli.cwd)?;
            export_config(&config, &path, &format)?;
        }
    }
    Ok(())
}

pub fn print_config(config: &Config, key: &KeyRef) -> Result<(), Box<dyn std::error::Error>> {
    let Some(value) = config.get(key) else {
        let data = config.get_all(&key.path.join(&key.key));
        if !data.is_empty() {
            for (key, value) in data {
                println!("{}: {}", key, value);
            }
        } else {
            return Err(format!("Missing key {}", key).into());
        }
        return Ok(());
    };
    println!("{}: {}", key.key, value);
    Ok(())
}

pub async fn import_config(
    mut config: Config,
    path: &Path,
    file: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    if file == PathBuf::from("-") {
        if std::io::stdin().is_terminal() {
            println!("Only available in non-interactive terminal");
            ::std::process::exit(2);
        }
        read_from_env(
            BufReader::new(std::io::stdin().lock()),
            path,
            &mut config.config,
        )?;
    } else {
        read_from_env(BufReader::new(File::open(&file)?), path, &mut config.config)?;
    }
    config.save().await?;
    Ok(())
}

pub fn export_config(
    config: &Config,
    path: &Path,
    format: &Format,
) -> Result<(), serde_json::Error> {
    let data = config.get_all(path);
    let result = match format {
        Format::EnvFile => export_as_env(&data),
        Format::Json => serde_json::to_string(&data)?,
    };
    println!("{result}");
    Ok(())
}

fn export_as_env(data: &HashMap<&str, String>) -> String {
    let mut res = String::new();
    for (key, value) in data {
        res.push_str(&format!("{}=\"{}\"\n", key, value));
    }
    res
}

fn read_from_env(
    buf: impl BufRead,
    path: &Path,
    config: &mut Configuration<ConfigValue>,
) -> Result<(), Box<dyn std::error::Error>> {
    buf.lines()
        .filter_map(|line| line.ok())
        .filter_map(|line| {
            let remove_comments = &line[0..line.find("#").unwrap_or(line.len())];
            let (key, value) = remove_comments.split_once('=')?;
            let value = value.replace("\"", "");
            let key = key.to_string();
            Some((key, value))
        })
        .try_for_each(|(key, value)| {
            let key_ref = parse_key_ref(&key, path)?;
            config.set(key_ref, ConfigValue::Value(value));
            Ok(())
        })
}
