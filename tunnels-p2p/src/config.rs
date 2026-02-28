//! P2P configuration.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Network magic bytes identifying the Tunnels protocol.
pub const NETWORK_MAGIC: [u8; 4] = [0x54, 0x55, 0x4E, 0x4C]; // "TUNL"

/// Maximum message size in bytes (1 MB).
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Current protocol version.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default target number of outbound connections.
pub const DEFAULT_TARGET_OUTBOUND: usize = 8;

/// Default maximum outbound connections.
pub const DEFAULT_MAX_OUTBOUND: usize = 12;

/// Default maximum inbound connections.
pub const DEFAULT_MAX_INBOUND: usize = 8;

/// Default connection timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default handshake timeout.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Default ping interval.
pub const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(60);

/// Default ping timeout.
pub const DEFAULT_PING_TIMEOUT: Duration = Duration::from_secs(120);

/// Default user agent string.
pub const DEFAULT_USER_AGENT: &str = "tunnels-p2p/0.1.0";

/// Configuration for the P2P node.
#[derive(Debug, Clone)]
pub struct P2pConfig {
    /// Address to bind the listener to.
    pub bind_addr: SocketAddr,

    /// Target number of outbound connections to maintain.
    pub target_outbound: usize,

    /// Maximum number of outbound connections.
    pub max_outbound: usize,

    /// Maximum number of inbound connections.
    pub max_inbound: usize,

    /// Timeout for establishing outbound connections.
    pub connect_timeout: Duration,

    /// Timeout for completing the handshake.
    pub handshake_timeout: Duration,

    /// Interval between ping messages.
    pub ping_interval: Duration,

    /// Timeout waiting for pong response.
    pub ping_timeout: Duration,

    /// DNS seed hostnames for peer discovery.
    pub dns_seeds: Vec<String>,

    /// Path to persistent peer list file.
    pub peers_file: PathBuf,

    /// User agent string to send in version messages.
    pub user_agent: String,

    /// Initial peers to connect to (bootstrap nodes).
    pub bootstrap_peers: Vec<SocketAddr>,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8333".parse().unwrap(),
            target_outbound: DEFAULT_TARGET_OUTBOUND,
            max_outbound: DEFAULT_MAX_OUTBOUND,
            max_inbound: DEFAULT_MAX_INBOUND,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            ping_interval: DEFAULT_PING_INTERVAL,
            ping_timeout: DEFAULT_PING_TIMEOUT,
            dns_seeds: Vec::new(),
            peers_file: PathBuf::from("peers.json"),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            bootstrap_peers: Vec::new(),
        }
    }
}

impl P2pConfig {
    /// Create a new configuration with the specified bind address.
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            ..Default::default()
        }
    }

    /// Set the target number of outbound connections.
    pub fn with_target_outbound(mut self, count: usize) -> Self {
        self.target_outbound = count;
        self
    }

    /// Set the maximum outbound connections.
    pub fn with_max_outbound(mut self, count: usize) -> Self {
        self.max_outbound = count;
        self
    }

    /// Set the maximum inbound connections.
    pub fn with_max_inbound(mut self, count: usize) -> Self {
        self.max_inbound = count;
        self
    }

    /// Set the connection timeout.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the handshake timeout.
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Add DNS seeds for peer discovery.
    pub fn with_dns_seeds(mut self, seeds: Vec<String>) -> Self {
        self.dns_seeds = seeds;
        self
    }

    /// Set the peers file path.
    pub fn with_peers_file(mut self, path: PathBuf) -> Self {
        self.peers_file = path;
        self
    }

    /// Set the user agent string.
    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = user_agent;
        self
    }

    /// Add bootstrap peers to connect to on startup.
    pub fn with_bootstrap_peers(mut self, peers: Vec<SocketAddr>) -> Self {
        self.bootstrap_peers = peers;
        self
    }

    /// Get the total maximum connections.
    pub fn max_connections(&self) -> usize {
        self.max_outbound + self.max_inbound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = P2pConfig::default();
        assert_eq!(config.target_outbound, DEFAULT_TARGET_OUTBOUND);
        assert_eq!(config.max_outbound, DEFAULT_MAX_OUTBOUND);
        assert_eq!(config.max_inbound, DEFAULT_MAX_INBOUND);
        assert_eq!(config.max_connections(), DEFAULT_MAX_OUTBOUND + DEFAULT_MAX_INBOUND);
    }

    #[test]
    fn test_config_builder() {
        let config = P2pConfig::new("127.0.0.1:9999".parse().unwrap())
            .with_target_outbound(4)
            .with_max_outbound(6)
            .with_max_inbound(4)
            .with_user_agent("test/1.0".to_string());

        assert_eq!(config.bind_addr.port(), 9999);
        assert_eq!(config.target_outbound, 4);
        assert_eq!(config.max_outbound, 6);
        assert_eq!(config.max_inbound, 4);
        assert_eq!(config.user_agent, "test/1.0");
    }
}
