//! Tunnels protocol node binary.
//!
//! This is the main entry point for the Tunnels node, which composes
//! all protocol crates into a running node with JSON-RPC API.

mod cli;
mod config;
mod devnet;
mod node;
mod rpc;
mod shutdown;

use tracing_subscriber::EnvFilter;

use crate::cli::Cli;
use crate::config::NodeConfig;
use crate::node::Node;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse_args();

    // Set up logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cli.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    tracing::info!("Tunnels Node v{}", env!("CARGO_PKG_VERSION"));

    // Build configuration
    let config = NodeConfig::from_cli(&cli);

    // Create and run node
    let node = Node::new(config).await?;
    node.run().await?;

    Ok(())
}
