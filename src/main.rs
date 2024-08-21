use std::path::PathBuf;

use clap::{Parser, Subcommand};
use secrets_manager::{
    commands::{
        get_config_path, handle_config, handle_config_migration, handle_secrets, init_config,
        ConfigCLI, VaultCli,
    },
    Config,
};

#[derive(Parser)]
struct Cli {
    /// Path to the config directory
    #[arg(long)]
    config_dir: Option<PathBuf>,
    /// Path to the config file, if the path is relative, it will be joined with either the default
    /// config_dir path or the one passed with --config-dir
    #[arg(long)]
    config_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage configurations
    Config(ConfigCLI),
    /// Manage vault secrets
    Secret(VaultCli),
    /// Manage the current context
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },
    /// Migrate from old config files to the new one
    Migrate { destination: Option<PathBuf> },
}

#[derive(Subcommand)]
enum ContextCommands {
    /// Returns the currently set context
    Get,
    /// Sets the context to the specified value
    Set { context: PathBuf },
    /// Empties the current context
    Reset,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let res: Result<(), Box<dyn std::error::Error>> = async {
        let cli = Cli::parse();
        let config_path = get_config_path(cli.config_dir, cli.config_file)?;
        match cli.command {
            Commands::Config(cli) => {
                init_config(&config_path)?;
                let config = Config::load(config_path).await?;
                handle_config(config, cli).await?;
            }
            Commands::Secret(cli) => {
                init_config(&config_path)?;
                let config = Config::load(config_path).await?;
                handle_secrets(config, cli).await?;
            }
            Commands::Migrate { destination } => {
                let new_path = destination.unwrap_or(config_path.clone());
                handle_config_migration(&config_path, new_path).await?;
            }
            Commands::Context {
                command: ContextCommands::Set { context },
            } => {
                init_config(&config_path)?;
                let mut config = Config::load(config_path).await?;
                config.set_current_context(context);
                config.save().await?;
            }
            Commands::Context {
                command: ContextCommands::Get,
            } => {
                init_config(&config_path)?;
                let config = Config::load(config_path).await?;
                println!(
                    "current context: {}",
                    config.get_current_context().display()
                );
                config.save().await?;
            }
            Commands::Context {
                command: ContextCommands::Reset,
            } => {
                init_config(&config_path)?;
                let mut config = Config::load(config_path).await?;
                config.set_current_context(PathBuf::new());
                config.save().await?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(err) = res {
        println!("{err}");
        std::process::exit(1);
    }
    Ok(())
}
