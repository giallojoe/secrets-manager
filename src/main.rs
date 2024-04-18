use clap::{Parser, Subcommand};
use secrets_manager::commands::{
    handle_config, handle_secrets, handle_store, Config, Secret, Store,
};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Config(Config),
    Secret(Secret),
    Store(Store),
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
        Commands::Store(cli) => {
            handle_store(cli).await?;
        }
    }
    Ok(())
}
