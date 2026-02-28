//! Peer discovery.
//!
//! This module provides:
//! - DNS seed resolution
//! - Peer address exchange
//! - Persistent peer storage
//! - Network seed configuration

pub mod dns;
pub mod exchange;
pub mod persistence;
pub mod seeds;

use std::collections::HashSet;
use std::net::SocketAddr;


use crate::config::P2pConfig;
use crate::error::P2pResult;
use crate::protocol::PeerAddress;

pub use dns::DnsResolver;
pub use exchange::{is_private_or_local, is_valid_peer_address, PeerExchange};
pub use persistence::{load_peers, save_peers, PeerPersistence};

/// Discovery coordinator.
pub struct Discovery {
    /// Peer exchange manager.
    exchange: PeerExchange,
    /// Peer persistence.
    persistence: PeerPersistence,
    /// DNS resolver (lazy initialized).
    dns_resolver: Option<DnsResolver>,
    /// DNS seeds.
    dns_seeds: Vec<String>,
    /// Bootstrap peers.
    bootstrap_peers: Vec<SocketAddr>,
}

impl Discovery {
    /// Create a new discovery coordinator.
    pub fn new(config: &P2pConfig) -> Self {
        Self {
            exchange: PeerExchange::new(),
            persistence: PeerPersistence::new(config.peers_file.clone()),
            dns_resolver: None,
            dns_seeds: config.dns_seeds.clone(),
            bootstrap_peers: config.bootstrap_peers.clone(),
        }
    }

    /// Initialize discovery (load peers, resolve DNS).
    pub async fn initialize(&mut self) -> P2pResult<()> {
        // Load persisted peers
        let persisted = self.persistence.load().await?;
        self.exchange.add_addresses(persisted);

        tracing::info!(
            persisted = self.exchange.len(),
            bootstrap = self.bootstrap_peers.len(),
            dns_seeds = self.dns_seeds.len(),
            "Discovery initialized"
        );

        Ok(())
    }

    /// Get addresses to connect to.
    pub fn get_addresses_to_connect(
        &self,
        connected: &HashSet<SocketAddr>,
        max: usize,
    ) -> Vec<SocketAddr> {
        let mut addrs = Vec::new();

        // First, try bootstrap peers
        for addr in &self.bootstrap_peers {
            if !connected.contains(addr) && addrs.len() < max {
                addrs.push(*addr);
            }
        }

        // Then, try known peers from exchange
        if addrs.len() < max {
            let from_exchange = self
                .exchange
                .get_addresses_to_try(connected, max - addrs.len());
            addrs.extend(from_exchange);
        }

        addrs
    }

    /// Resolve DNS seeds and add to exchange.
    pub async fn resolve_dns_seeds(&mut self) -> P2pResult<Vec<SocketAddr>> {
        if self.dns_seeds.is_empty() {
            return Ok(Vec::new());
        }

        // Initialize resolver if needed
        if self.dns_resolver.is_none() {
            self.dns_resolver = Some(DnsResolver::new().await?);
        }

        let resolver = self.dns_resolver.as_ref().unwrap();
        let addrs = resolver.resolve_seeds(&self.dns_seeds).await;

        // Add to exchange
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for addr in &addrs {
            self.exchange.add_address(*addr, now);
        }

        self.persistence.mark_dirty();

        Ok(addrs)
    }

    /// Add a peer address (e.g., from GetPeers response).
    pub fn add_address(&mut self, addr: SocketAddr, last_seen: u64) {
        self.exchange.add_address(addr, last_seen);
        self.persistence.mark_dirty();
    }

    /// Add multiple addresses.
    pub fn add_addresses(&mut self, addresses: Vec<PeerAddress>) {
        self.exchange.add_addresses(addresses);
        self.persistence.mark_dirty();
    }

    /// Mark a peer as seen (successful connection).
    pub fn mark_seen(&mut self, addr: SocketAddr) {
        self.exchange.mark_seen(addr);
        self.persistence.mark_dirty();
    }

    /// Save peers to disk if dirty.
    pub async fn save_if_dirty(&mut self) -> P2pResult<()> {
        if self.persistence.is_dirty() {
            let peers = self.exchange.all_addresses().to_vec();
            self.persistence.save(&peers).await?;
        }
        Ok(())
    }

    /// Get the peer exchange manager.
    pub fn exchange(&self) -> &PeerExchange {
        &self.exchange
    }

    /// Get mutable peer exchange manager.
    pub fn exchange_mut(&mut self) -> &mut PeerExchange {
        &mut self.exchange
    }

    /// Get the number of known addresses.
    pub fn known_count(&self) -> usize {
        self.exchange.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_discovery_creation() {
        let mut config = P2pConfig::default();
        let dir = tempdir().unwrap();
        config.peers_file = dir.path().join("peers.json");

        let discovery = Discovery::new(&config);
        assert_eq!(discovery.known_count(), 0);
    }

    #[tokio::test]
    async fn test_get_addresses_bootstrap() {
        let mut config = P2pConfig::default();
        let dir = tempdir().unwrap();
        config.peers_file = dir.path().join("peers.json");
        config.bootstrap_peers = vec!["127.0.0.1:8333".parse().unwrap()];

        let discovery = Discovery::new(&config);
        let connected = HashSet::new();

        let addrs = discovery.get_addresses_to_connect(&connected, 10);
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0], config.bootstrap_peers[0]);
    }
}
