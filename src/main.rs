use clap::{Parser, Subcommand};
use secrets_manager::commands::{handle_config, handle_secrets, Config, Secret};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Config(Config),
    Secret(Secret),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Config(cli) => {
            handle_config(cli).await?;
        }
        Commands::Secret(cli) => {
            handle_secrets(cli).await?;
        }
    }
    Ok(())
}
