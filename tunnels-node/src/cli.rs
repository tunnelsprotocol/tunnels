//! Command-line argument parsing.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

/// Tunnels protocol node.
#[derive(Parser, Debug, Clone)]
#[command(name = "tunnels-node")]
#[command(about = "Tunnels protocol node binary")]
#[command(version)]
pub struct Cli {
    /// Data directory for blockchain data.
    #[arg(long, default_value = "~/.tunnels")]
    pub data_dir: PathBuf,

    /// P2P listen address.
    #[arg(long, default_value = "0.0.0.0:9333")]
    pub listen: SocketAddr,

    /// RPC listen address.
    #[arg(long, default_value = "127.0.0.1:9334")]
    pub rpc_listen: SocketAddr,

    /// Comma-separated list of seed nodes.
    #[arg(long, value_delimiter = ',')]
    pub seed_nodes: Option<Vec<SocketAddr>>,

    /// Enable mining.
    #[arg(long)]
    pub mine: bool,

    /// Enable devnet mode (trivial PoW, auto-mine).
    #[arg(long)]
    pub devnet: bool,

    /// Disable auto-mine in devnet mode (requires explicit mine RPC calls).
    #[arg(long)]
    pub no_auto_mine: bool,

    /// Log level (trace, debug, info, warn, error).
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

impl Cli {
    /// Parse command-line arguments.
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Expand the data directory path (handle ~ for home).
    pub fn expanded_data_dir(&self) -> PathBuf {
        let path_str = self.data_dir.to_string_lossy();
        if let Some(stripped) = path_str.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        self.data_dir.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let cli = Cli::parse_from(["tunnels-node"]);
        assert_eq!(cli.listen.port(), 9333);
        assert_eq!(cli.rpc_listen.port(), 9334);
        assert!(!cli.mine);
        assert!(!cli.devnet);
        assert_eq!(cli.log_level, "info");
    }

    #[test]
    fn test_devnet_flag() {
        let cli = Cli::parse_from(["tunnels-node", "--devnet"]);
        assert!(cli.devnet);
    }

    #[test]
    fn test_seed_nodes() {
        let cli = Cli::parse_from([
            "tunnels-node",
            "--seed-nodes",
            "127.0.0.1:9333,192.168.1.1:9333",
        ]);
        let seeds = cli.seed_nodes.unwrap();
        assert_eq!(seeds.len(), 2);
    }
}
