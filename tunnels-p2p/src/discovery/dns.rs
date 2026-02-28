//! DNS seed resolution.

use std::net::SocketAddr;
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::TokioAsyncResolver;

use crate::error::{P2pError, P2pResult};

/// Default port for peer connections.
pub const DEFAULT_PORT: u16 = 8333;

/// DNS seed resolver.
pub struct DnsResolver {
    /// The async resolver.
    resolver: TokioAsyncResolver,
}

impl DnsResolver {
    /// Create a new DNS resolver.
    pub async fn new() -> P2pResult<Self> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());
        Ok(Self { resolver })
    }

    /// Resolve a DNS seed hostname to peer addresses.
    pub async fn resolve_seed(&self, hostname: &str) -> P2pResult<Vec<SocketAddr>> {
        tracing::debug!(hostname, "Resolving DNS seed");

        let response = self
            .resolver
            .lookup_ip(hostname)
            .await
            .map_err(|e| P2pError::DnsResolutionFailed {
                host: hostname.to_string(),
                error: e.to_string(),
            })?;

        let addrs: Vec<SocketAddr> = response
            .iter()
            .map(|ip| SocketAddr::new(ip, DEFAULT_PORT))
            .collect();

        tracing::debug!(
            hostname,
            count = addrs.len(),
            "Resolved DNS seed"
        );

        Ok(addrs)
    }

    /// Resolve multiple DNS seeds.
    pub async fn resolve_seeds(&self, hostnames: &[String]) -> Vec<SocketAddr> {
        let mut all_addrs = Vec::new();

        for hostname in hostnames {
            match self.resolve_seed(hostname).await {
                Ok(addrs) => {
                    all_addrs.extend(addrs);
                }
                Err(e) => {
                    tracing::warn!(hostname, error = %e, "Failed to resolve DNS seed");
                }
            }
        }

        // Remove duplicates
        all_addrs.sort();
        all_addrs.dedup();

        all_addrs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // DNS resolution tests require network access, so we just test basic creation
    #[tokio::test]
    async fn test_resolver_creation() {
        let resolver = DnsResolver::new().await;
        assert!(resolver.is_ok());
    }
}
