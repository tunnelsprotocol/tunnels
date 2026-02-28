//! Peer address exchange.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::{Message, PeerAddress, PeersMessage};

/// Maximum peers to send in a Peers response.
pub const MAX_PEERS_TO_SEND: usize = 1000;

/// Maximum age (in seconds) for a peer address to be considered fresh.
pub const MAX_PEER_AGE_SECS: u64 = 3 * 60 * 60; // 3 hours

/// Check if an IP address is a private/local address that should be rejected.
pub fn is_private_or_local(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(ip) => {
            // Loopback (127.0.0.0/8)
            ip.is_loopback()
            // Private ranges
            || ip.is_private()
            // Link-local (169.254.0.0/16)
            || ip.is_link_local()
            // Broadcast
            || ip.is_broadcast()
            // Documentation (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24)
            || is_documentation_v4(ip)
            // Unspecified (0.0.0.0)
            || ip.is_unspecified()
        }
        IpAddr::V6(ip) => {
            // Loopback (::1)
            ip.is_loopback()
            // Unspecified (::)
            || ip.is_unspecified()
            // Link-local (fe80::/10)
            || is_unicast_link_local_v6(ip)
            // Unique local (fc00::/7)
            || is_unique_local_v6(ip)
        }
    }
}

/// Check if IPv4 address is in documentation range.
fn is_documentation_v4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    // 192.0.2.0/24
    (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
    // 198.51.100.0/24
    || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
    // 203.0.113.0/24
    || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
}

/// Check if IPv6 address is link-local unicast (fe80::/10).
fn is_unicast_link_local_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xffc0) == 0xfe80
}

/// Check if IPv6 address is unique local (fc00::/7).
fn is_unique_local_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xfe00) == 0xfc00
}

/// Validate a peer address for external use.
pub fn is_valid_peer_address(addr: &SocketAddr, our_addr: Option<&SocketAddr>) -> bool {
    // Reject private/local addresses
    if is_private_or_local(addr) {
        return false;
    }

    // Reject our own address
    if let Some(own) = our_addr {
        if addr == own {
            return false;
        }
    }

    // Reject port 0
    if addr.port() == 0 {
        return false;
    }

    true
}

/// Peer address manager for exchange.
#[derive(Debug)]
pub struct PeerExchange {
    /// Known peer addresses with last seen times.
    known_peers: Vec<PeerAddress>,
    /// Set of addresses for quick lookup.
    address_set: HashSet<SocketAddr>,
    /// Maximum number of addresses to store.
    max_addresses: usize,
    /// Our own address (to avoid adding it).
    our_addr: Option<SocketAddr>,
    /// Whether to validate addresses (disable for testing).
    validate_addresses: bool,
}

impl PeerExchange {
    /// Create a new peer exchange manager.
    pub fn new() -> Self {
        Self {
            known_peers: Vec::new(),
            address_set: HashSet::new(),
            max_addresses: 10000,
            our_addr: None,
            validate_addresses: true,
        }
    }

    /// Create with custom max addresses.
    pub fn with_max_addresses(max: usize) -> Self {
        Self {
            known_peers: Vec::new(),
            address_set: HashSet::new(),
            max_addresses: max,
            our_addr: None,
            validate_addresses: true,
        }
    }

    /// Set our own address (to avoid adding it to the pool).
    pub fn set_our_addr(&mut self, addr: SocketAddr) {
        self.our_addr = Some(addr);
    }

    /// Disable address validation (for testing).
    pub fn disable_validation(&mut self) {
        self.validate_addresses = false;
    }

    /// Add or update a peer address.
    pub fn add_address(&mut self, addr: SocketAddr, last_seen: u64) {
        // Validate address unless disabled
        if self.validate_addresses && !is_valid_peer_address(&addr, self.our_addr.as_ref()) {
            tracing::debug!(addr = %addr, "Rejecting invalid peer address");
            return;
        }

        if self.address_set.contains(&addr) {
            // Update existing
            if let Some(peer) = self.known_peers.iter_mut().find(|p| p.addr == addr) {
                peer.last_seen = peer.last_seen.max(last_seen);
            }
        } else if self.known_peers.len() < self.max_addresses {
            // Add new
            self.known_peers.push(PeerAddress { addr, last_seen });
            self.address_set.insert(addr);
        } else {
            // Replace oldest if new is fresher
            if let Some((idx, oldest)) = self
                .known_peers
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| p.last_seen)
            {
                if last_seen > oldest.last_seen {
                    self.address_set.remove(&oldest.addr);
                    self.known_peers[idx] = PeerAddress { addr, last_seen };
                    self.address_set.insert(addr);
                }
            }
        }
    }

    /// Add multiple addresses.
    pub fn add_addresses(&mut self, addresses: Vec<PeerAddress>) {
        for peer in addresses {
            self.add_address(peer.addr, peer.last_seen);
        }
    }

    /// Mark an address as seen now.
    pub fn mark_seen(&mut self, addr: SocketAddr) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.add_address(addr, now);
    }

    /// Get fresh addresses to send to a peer.
    pub fn get_addresses_for_peer(&self, max: usize) -> Vec<PeerAddress> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut fresh: Vec<_> = self
            .known_peers
            .iter()
            .filter(|p| now.saturating_sub(p.last_seen) < MAX_PEER_AGE_SECS)
            .cloned()
            .collect();

        // Sort by last_seen descending (freshest first)
        fresh.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        fresh.truncate(max.min(MAX_PEERS_TO_SEND));

        fresh
    }

    /// Get addresses for connection attempts.
    pub fn get_addresses_to_try(&self, exclude: &HashSet<SocketAddr>, max: usize) -> Vec<SocketAddr> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut candidates: Vec<_> = self
            .known_peers
            .iter()
            .filter(|p| !exclude.contains(&p.addr))
            .filter(|p| now.saturating_sub(p.last_seen) < MAX_PEER_AGE_SECS)
            .cloned()
            .collect();

        // Sort by last_seen descending
        candidates.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        candidates.truncate(max);

        candidates.into_iter().map(|p| p.addr).collect()
    }

    /// Create a GetPeers message.
    pub fn create_get_peers() -> Message {
        Message::GetPeers
    }

    /// Create a Peers response.
    pub fn create_peers_response(&self) -> Message {
        let peers = self.get_addresses_for_peer(MAX_PEERS_TO_SEND);
        Message::Peers(PeersMessage { peers })
    }

    /// Process a received Peers message.
    pub fn process_peers(&mut self, peers: &PeersMessage) {
        self.add_addresses(peers.peers.clone());
    }

    /// Get the number of known addresses.
    pub fn len(&self) -> usize {
        self.known_peers.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.known_peers.is_empty()
    }

    /// Check if an address is known.
    pub fn contains(&self, addr: &SocketAddr) -> bool {
        self.address_set.contains(addr)
    }

    /// Remove an address.
    pub fn remove(&mut self, addr: &SocketAddr) -> bool {
        if self.address_set.remove(addr) {
            self.known_peers.retain(|p| &p.addr != addr);
            true
        } else {
            false
        }
    }

    /// Clear all addresses.
    pub fn clear(&mut self) {
        self.known_peers.clear();
        self.address_set.clear();
    }

    /// Get all known addresses.
    pub fn all_addresses(&self) -> &[PeerAddress] {
        &self.known_peers
    }
}

impl Default for PeerExchange {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn test_add_address() {
        let mut exchange = PeerExchange::new();
        // Use public IP for testing
        let addr: SocketAddr = "8.8.8.8:8333".parse().unwrap();
        let now = now_secs();

        exchange.add_address(addr, now);
        assert_eq!(exchange.len(), 1);
        assert!(exchange.contains(&addr));

        // Duplicate should update
        exchange.add_address(addr, now + 100);
        assert_eq!(exchange.len(), 1);
    }

    #[test]
    fn test_get_addresses_excludes() {
        let mut exchange = PeerExchange::new();
        let now = now_secs();

        // Use public IPs for testing
        let addr1: SocketAddr = "8.8.8.8:8333".parse().unwrap();
        let addr2: SocketAddr = "1.1.1.1:8333".parse().unwrap();

        exchange.add_address(addr1, now);
        exchange.add_address(addr2, now);

        let mut exclude = HashSet::new();
        exclude.insert(addr1);

        let addrs = exchange.get_addresses_to_try(&exclude, 10);
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0], addr2);
    }

    #[test]
    fn test_mark_seen() {
        let mut exchange = PeerExchange::new();
        // Use a public IP for testing
        let addr: SocketAddr = "8.8.8.8:8333".parse().unwrap();

        exchange.mark_seen(addr);
        assert!(exchange.contains(&addr));
    }

    #[test]
    fn test_is_private_or_local() {
        // Private addresses
        assert!(is_private_or_local(&"10.0.0.1:8333".parse().unwrap()));
        assert!(is_private_or_local(&"172.16.0.1:8333".parse().unwrap()));
        assert!(is_private_or_local(&"192.168.1.1:8333".parse().unwrap()));

        // Loopback
        assert!(is_private_or_local(&"127.0.0.1:8333".parse().unwrap()));
        assert!(is_private_or_local(&"[::1]:8333".parse().unwrap()));

        // Link-local
        assert!(is_private_or_local(&"169.254.1.1:8333".parse().unwrap()));

        // Public addresses should be valid
        assert!(!is_private_or_local(&"8.8.8.8:8333".parse().unwrap()));
        assert!(!is_private_or_local(&"1.1.1.1:443".parse().unwrap()));
    }

    #[test]
    fn test_is_valid_peer_address() {
        let our_addr: SocketAddr = "203.0.113.1:8333".parse().unwrap();

        // Reject our own address
        assert!(!is_valid_peer_address(&our_addr, Some(&our_addr)));

        // Reject port 0
        assert!(!is_valid_peer_address(&"8.8.8.8:0".parse().unwrap(), None));

        // Accept valid public addresses
        assert!(is_valid_peer_address(&"8.8.8.8:8333".parse().unwrap(), Some(&our_addr)));
    }

    #[test]
    fn test_exchange_rejects_invalid_addresses() {
        let mut exchange = PeerExchange::new();
        let now = now_secs();

        // These should be rejected
        exchange.add_address("127.0.0.1:8333".parse().unwrap(), now);
        exchange.add_address("192.168.1.1:8333".parse().unwrap(), now);
        exchange.add_address("10.0.0.1:8333".parse().unwrap(), now);

        assert_eq!(exchange.len(), 0, "Invalid addresses should be rejected");

        // This should be accepted
        exchange.add_address("8.8.8.8:8333".parse().unwrap(), now);
        assert_eq!(exchange.len(), 1, "Valid address should be accepted");
    }

    #[test]
    fn test_exchange_with_validation_disabled() {
        let mut exchange = PeerExchange::new();
        exchange.disable_validation();
        let now = now_secs();

        // Should be accepted when validation is disabled
        exchange.add_address("127.0.0.1:8333".parse().unwrap(), now);
        assert_eq!(exchange.len(), 1, "Address should be accepted when validation disabled");
    }
}
