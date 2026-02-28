//! DNS seed configuration.
//!
//! DNS seeds are used for initial peer discovery when a node starts for the
//! first time. The node queries these DNS hostnames to get a list of active
//! nodes to connect to.
//!
//! # Adding Seeds
//!
//! To add a new DNS seed:
//!
//! 1. Set up a DNS seeder that returns A/AAAA records for active Tunnels nodes
//! 2. Add the hostname to the appropriate list below
//! 3. Seeds should return nodes that:
//!    - Have been online for at least 24 hours
//!    - Are running a compatible protocol version
//!    - Have good connectivity (accepting inbound connections)
//!
//! # DNS Record Format
//!
//! Seeds should return standard A (IPv4) or AAAA (IPv6) records pointing to
//! Tunnels nodes. The P2P port (9333) is assumed.
//!
//! Example DNS zone file entries:
//! ```text
//! seed.tunnelsprotocol.com.  IN A     203.0.113.42
//! seed.tunnelsprotocol.com.  IN A     198.51.100.17
//! seed.tunnelsprotocol.com.  IN AAAA  2001:db8::1
//! ```

/// Default P2P port for Tunnels nodes.
pub const DEFAULT_P2P_PORT: u16 = 9333;

/// Mainnet DNS seed hostnames.
///
/// These are queried at startup to discover initial peers.
/// Add production seeds here when the mainnet launches.
pub const MAINNET_SEEDS: &[&str] = &[
    // Placeholder seeds - replace with real hostnames at launch
    // "seed1.tunnelsprotocol.com",
    // "seed2.tunnelsprotocol.com",
    // "seed3.tunnelsprotocol.com",
];

/// Testnet DNS seed hostnames.
///
/// Used for testing and development networks.
pub const TESTNET_SEEDS: &[&str] = &[
    // Placeholder seeds - add testnet seeds here
    // "testnet-seed1.tunnelsprotocol.com",
];

/// Devnet seed addresses.
///
/// For local development and testing. These are socket addresses
/// rather than DNS hostnames since devnet doesn't use DNS.
pub const DEVNET_SEEDS: &[&str] = &[
    // Local development seeds
    // "127.0.0.1:9333",
    // "127.0.0.1:9334",
];

/// Get the appropriate seed list for a network.
pub fn get_seeds(network: Network) -> &'static [&'static str] {
    match network {
        Network::Mainnet => MAINNET_SEEDS,
        Network::Testnet => TESTNET_SEEDS,
        Network::Devnet => DEVNET_SEEDS,
    }
}

/// Network type for seed selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Network {
    /// Production mainnet.
    #[default]
    Mainnet,
    /// Public testnet for testing.
    Testnet,
    /// Local development network.
    Devnet,
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet => write!(f, "testnet"),
            Network::Devnet => write!(f, "devnet"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mainnet_seeds_format() {
        // Mainnet seeds should be valid hostnames (when populated)
        for seed in MAINNET_SEEDS {
            // Should not contain port numbers
            assert!(!seed.contains(':'), "Seed should be hostname only: {}", seed);
            // Should not start with a number (IP address)
            assert!(
                !seed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false),
                "Seed should be hostname, not IP: {}",
                seed
            );
        }
    }

    #[test]
    fn test_get_seeds() {
        assert_eq!(get_seeds(Network::Mainnet), MAINNET_SEEDS);
        assert_eq!(get_seeds(Network::Testnet), TESTNET_SEEDS);
        assert_eq!(get_seeds(Network::Devnet), DEVNET_SEEDS);
    }

    #[test]
    fn test_network_display() {
        assert_eq!(Network::Mainnet.to_string(), "mainnet");
        assert_eq!(Network::Testnet.to_string(), "testnet");
        assert_eq!(Network::Devnet.to_string(), "devnet");
    }

    #[test]
    fn test_default_port() {
        assert_eq!(DEFAULT_P2P_PORT, 9333);
    }
}
