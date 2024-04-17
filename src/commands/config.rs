use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use is_terminal::IsTerminal as _;

use crate::{ConfigValue, Configuration, SecretManager};

use super::{get_config_path, get_path};

#[derive(Parser)]
pub struct Config {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(long)]
    cwd: Option<PathBuf>,

    #[arg(short, long)]
    path: Option<PathBuf>,
    #[command(subcommand)]
    command: ConfigCommands,
}

#[derive(Subcommand)]
enum ConfigCommands {
    Get {
        key: Option<String>,
    },
    Set {
        key: String,
        #[command(subcommand)]
        value: ValueInput,
    },
    Remove {
        key: String,
    },
    GetAll,
    /// import from env file
    Import {
        file: PathBuf,
    },
    Export {
        #[arg(short, long)]
        format: Format,
    },
}

#[derive(Subcommand)]
enum ValueInput {
    #[command(name = "--value")]
    Value { value: String },
    #[command(name = "--secret")]
    Secret(SecretInput),
}

#[derive(Parser)]
struct SecretInput {
    secret_path: PathBuf,
    secret_key: String,
}

pub async fn handle_config(cli: Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_file_path = get_config_path(cli.config.clone(), "config.json")?;
    let secret_file_path = get_config_path(cli.config, "secret.json")?;
    if !secret_file_path.exists() {
        return Err(String::from("Secret configuration not found, make sure to run secret create with the name of the secret, (if it exists it will not create it again)").into());
    }
    let secrets = SecretManager::from_config(secret_file_path, PathBuf::from("/")).await?;
    let cwd = get_path(cli.cwd, cli.path)?;
    let mut config = Configuration::from_path(&config_file_path, cwd)?;
    match cli.command {
        ConfigCommands::Get { key } => {
            print_config(&config, &secrets, key.as_deref())?;
        }
        ConfigCommands::Set { key, value } => {
            let value = match value {
                ValueInput::Value { value } => ConfigValue::Value(value),
                ValueInput::Secret(s) => ConfigValue::Secret {
                    path: s.secret_path,
                    key: s.secret_key,
                },
            };

            set_config_value(&mut config, &config_file_path, key, value)?;
        }
        ConfigCommands::Remove { key } => {
            remove_config_value(&mut config, &config_file_path, &key)?;
        }
        ConfigCommands::GetAll => {
            println!("{}", config.print_tree());
        }
        ConfigCommands::Import { file } => {
            import_config(&mut config, &config_file_path, file)?;
        }
        ConfigCommands::Export { format } => {
            export_config(&config, &secrets, &format)?;
        }
    }
    Ok(())
}

pub fn print_config(
    config: &Configuration<ConfigValue>,
    secrets: &SecretManager,
    key: Option<&str>,
) -> Result<(), String> {
    if let Some(key) = key {
        let value = config
            .get_value(&key)
            .ok_or_else(|| format!("Missing key {}", key))?;
        let value = secrets
            .resolve_value(value)
            .ok_or_else(|| format!("Missing secret {}", value))?;
        println!("{}: {}", key, value);
    } else {
        let data = secrets.resolve(config);
        for (key, value) in data {
            println!("{}: {}", key, value);
        }
    }
    Ok(())
}

pub fn set_config_value(
    config: &mut Configuration<ConfigValue>,
    path: impl AsRef<Path>,
    key: String,
    value: ConfigValue,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("ConfigValue: {}", value);
    let res = config.set_value(key.clone(), value);
    if let Some(res) = res {
        println!("{} value overridden, previous value was `{}`", key, res);
    } else {
        println!("{} updated successfully", key);
    }
    config.save(&path)?;
    Ok(())
}

pub fn remove_config_value(
    config: &mut Configuration<ConfigValue>,
    path: impl AsRef<Path>,
    key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let res = config.remove_value(&key);
    if let Some(res) = res {
        println!("{} value removed, previous value was `{}`", key, res);
    } else {
        println!("{} not found, nothing to do", key);
    }
    config.save(path)?;
    Ok(())
}

pub fn import_config(
    config: &mut Configuration<ConfigValue>,
    path: impl AsRef<Path>,
    file: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    if file == PathBuf::from("-") {
        if std::io::stdin().is_terminal() {
            println!("Only available in non-interactive terminal");
            ::std::process::exit(2);
        }
        read_from_env(BufReader::new(std::io::stdin().lock()), config);
    } else {
        read_from_env(BufReader::new(File::open(&file)?), config);
    }
    config.save(path)?;
    Ok(())
}

pub fn export_config(
    config: &Configuration<ConfigValue>,
    secrets: &SecretManager,
    format: &Format,
) -> Result<(), serde_json::Error> {
    let data = secrets.resolve(config);
    let result = match format {
        Format::EnvFile => export_as_env(&data),
        Format::Json => serde_json::to_string(&data)?,
    };
    println!("{result}");
    Ok(())
}

#[derive(clap::ValueEnum, Default, Clone)]
pub enum Format {
    #[value(name = "env")]
    #[default]
    EnvFile,
    #[value(name = "json")]
    Json,
}

fn export_as_env(data: &HashMap<&String, &String>) -> String {
    let mut res = String::new();
    for (key, value) in data {
        res.push_str(&format!("{}=\"{}\"\n", key, value));
    }
    res
}

fn read_from_env(buf: impl BufRead, config: &mut Configuration<ConfigValue>) {
    buf.lines()
        .filter_map(|line| line.ok())
        .filter_map(|line| {
            let remove_comments = &line[0..line.find("#").unwrap_or(line.len())];
            let (key, value) = remove_comments.split_once('=')?;
            let value = value.replace("\"", "");
            let key = key.to_string();
            Some((key, value))
        })
        .for_each(|(key, value)| {
            config.set_value(&key, ConfigValue::Value(value));
        });
}
