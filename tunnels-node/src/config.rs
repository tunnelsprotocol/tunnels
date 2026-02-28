//! Node configuration.

use std::net::SocketAddr;
use std::path::PathBuf;

use tunnels_p2p::P2pConfig;

use crate::cli::Cli;
use crate::devnet::DevnetConfig;

/// Complete node configuration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NodeConfig {
    /// Data directory for blockchain data.
    pub data_dir: PathBuf,

    /// P2P listen address.
    pub p2p_addr: SocketAddr,

    /// RPC listen address.
    pub rpc_addr: SocketAddr,

    /// Seed nodes to connect to.
    pub seed_nodes: Vec<SocketAddr>,

    /// Enable mining.
    pub mining_enabled: bool,

    /// Devnet configuration (if enabled).
    pub devnet: Option<DevnetConfig>,

    /// Log level.
    pub log_level: String,
}

impl NodeConfig {
    /// Create a node configuration from CLI arguments.
    pub fn from_cli(cli: &Cli) -> Self {
        let devnet = if cli.devnet {
            let mut config = DevnetConfig::default();
            if cli.no_auto_mine {
                config.auto_mine = false;
            }
            Some(config)
        } else {
            None
        };

        Self {
            data_dir: cli.expanded_data_dir(),
            p2p_addr: cli.listen,
            rpc_addr: cli.rpc_listen,
            seed_nodes: cli.seed_nodes.clone().unwrap_or_default(),
            mining_enabled: cli.mine || cli.devnet, // Devnet implies mining
            devnet,
            log_level: cli.log_level.clone(),
        }
    }

    /// Build P2P configuration from node config.
    #[allow(dead_code)]
    pub fn p2p_config(&self) -> P2pConfig {
        let mut config = P2pConfig::new(self.p2p_addr);

        // Add seed nodes
        for seed in &self.seed_nodes {
            config.bootstrap_peers.push(*seed);
        }

        // Adjust for devnet if needed
        if self.devnet.is_some() {
            // Faster timeouts for devnet
            config.connect_timeout = std::time::Duration::from_secs(2);
            config.handshake_timeout = std::time::Duration::from_secs(2);
        }

        config
    }

    /// Check if devnet mode is enabled.
    pub fn is_devnet(&self) -> bool {
        self.devnet.is_some()
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("~/.tunnels"),
            p2p_addr: "0.0.0.0:9333".parse().unwrap(),
            rpc_addr: "127.0.0.1:9334".parse().unwrap(),
            seed_nodes: Vec::new(),
            mining_enabled: false,
            devnet: None,
            log_level: "info".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NodeConfig::default();
        assert_eq!(config.p2p_addr.port(), 9333);
        assert_eq!(config.rpc_addr.port(), 9334);
        assert!(!config.mining_enabled);
        assert!(!config.is_devnet());
    }

    #[test]
    fn test_devnet_config() {
        let mut config = NodeConfig::default();
        config.devnet = Some(DevnetConfig::default());

        assert!(config.is_devnet());
    }
}
