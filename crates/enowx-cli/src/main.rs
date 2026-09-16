use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use enowx_core::Config;
#[cfg(feature = "web")]
use tracing_subscriber::EnvFilter;

mod dev;

#[derive(Debug, Parser)]
#[command(
    name = "enx",
    version,
    about = "Rust coding agent with the enowx-cli terminal interface"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open the enowx-cli terminal interface.
    Tui {
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Resume this session id instead of starting a new conversation.
        #[arg(long)]
        session: Option<String>,
    },
    /// Rebuild and relaunch the interface whenever a source file changes.
    Dev {
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Pin one session across restarts; defaults to the newest one.
        #[arg(long)]
        session: Option<String>,
    },
    /// Run the API and embedded dashboard in the foreground.
    #[cfg(feature = "web")]
    Serve {
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Read or change ~/.enx/config.toml.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print a dotted key, for example `model.default`.
    Get { key: String },
    /// Set and persist a dotted key.
    Set { key: String, value: String },
    /// Print the config file path.
    Path,
}

#[tokio::main]
async fn main() -> Result<()> {
    // The alternate-screen TUI owns stdout. Server mode initializes tracing
    // below, so logs never corrupt the terminal interface.

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Tui {
        workspace: None,
        session: None,
    }) {
        Command::Tui { workspace, session } => {
            let mut config = Config::load()?;
            if workspace.is_some() {
                config.agent.workspace = workspace;
            }
            enowx_tui::run(config, session).await
        }
        Command::Dev { workspace, session } => dev::run(workspace, session),
        #[cfg(feature = "web")]
        Command::Serve {
            port,
            host,
            workspace,
        } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("enx=info")),
                )
                .compact()
                .init();
            let mut config = Config::load()?;
            if let Some(port) = port {
                config.server.port = port;
            }
            if let Some(host) = host {
                config.server.host = host;
            }
            if workspace.is_some() {
                config.agent.workspace = workspace;
            }
            enowx_server::serve(config).await
        }
        Command::Config { command } => {
            let mut config = Config::load()?;
            match command {
                ConfigCommand::Get { key } => {
                    let value = config
                        .get(&key)
                        .ok_or_else(|| anyhow::anyhow!("unknown config key: {key}"))?;
                    println!("{value}");
                }
                ConfigCommand::Set { key, value } => {
                    config.set(&key, &value)?;
                    let path = config.save()?;
                    println!("Updated {key}");
                    println!("saved {}", path.display());
                }
                ConfigCommand::Path => println!("{}", enowx_core::config::config_path().display()),
            }
            Ok(())
        }
    }
}
