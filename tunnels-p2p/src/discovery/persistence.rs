//! Persistent peer list storage.

use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{P2pError, P2pResult};
use crate::protocol::PeerAddress;

/// Persistent peer storage format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStore {
    /// Version of the storage format.
    pub version: u32,
    /// Stored peer addresses.
    pub peers: Vec<StoredPeer>,
}

/// A stored peer entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPeer {
    /// The peer address.
    pub addr: String,
    /// Last time the peer was seen.
    pub last_seen: u64,
    /// Number of successful connections.
    pub success_count: u32,
    /// Number of failed connection attempts.
    pub failure_count: u32,
}

impl Default for PeerStore {
    fn default() -> Self {
        Self {
            version: 1,
            peers: Vec::new(),
        }
    }
}

impl From<PeerAddress> for StoredPeer {
    fn from(addr: PeerAddress) -> Self {
        Self {
            addr: addr.addr.to_string(),
            last_seen: addr.last_seen,
            success_count: 1,
            failure_count: 0,
        }
    }
}

impl StoredPeer {
    /// Try to parse the stored address.
    pub fn socket_addr(&self) -> Option<SocketAddr> {
        self.addr.parse().ok()
    }

    /// Convert to PeerAddress.
    pub fn to_peer_address(&self) -> Option<PeerAddress> {
        self.socket_addr().map(|addr| PeerAddress {
            addr,
            last_seen: self.last_seen,
        })
    }
}

/// Load peers from a file.
pub async fn load_peers(path: &Path) -> P2pResult<Vec<PeerAddress>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut file = fs::File::open(path).await?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;

    let store: PeerStore = serde_json::from_str(&contents).map_err(|e| {
        P2pError::Serialization(format!("Failed to parse peers file: {}", e))
    })?;

    let peers: Vec<PeerAddress> = store
        .peers
        .iter()
        .filter_map(|p| p.to_peer_address())
        .collect();

    tracing::info!(count = peers.len(), path = ?path, "Loaded peers from file");

    Ok(peers)
}

/// Save peers to a file.
pub async fn save_peers(path: &Path, peers: &[PeerAddress]) -> P2pResult<()> {
    let store = PeerStore {
        version: 1,
        peers: peers
            .iter()
            .map(|p| StoredPeer::from(p.clone()))
            .collect(),
    };

    let contents = serde_json::to_string_pretty(&store).map_err(|e| {
        P2pError::Serialization(format!("Failed to serialize peers: {}", e))
    })?;

    // Write to temp file first, then rename (atomic)
    let temp_path = path.with_extension("tmp");

    let mut file = fs::File::create(&temp_path).await?;
    file.write_all(contents.as_bytes()).await?;
    file.sync_all().await?;
    drop(file);

    fs::rename(&temp_path, path).await?;

    tracing::debug!(count = peers.len(), path = ?path, "Saved peers to file");

    Ok(())
}

/// Peer persistence manager.
pub struct PeerPersistence {
    /// Path to the peers file.
    path: std::path::PathBuf,
    /// Whether dirty (needs saving).
    dirty: bool,
}

impl PeerPersistence {
    /// Create a new persistence manager.
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path, dirty: false }
    }

    /// Load peers from disk.
    pub async fn load(&self) -> P2pResult<Vec<PeerAddress>> {
        load_peers(&self.path).await
    }

    /// Save peers to disk.
    pub async fn save(&mut self, peers: &[PeerAddress]) -> P2pResult<()> {
        save_peers(&self.path, peers).await?;
        self.dirty = false;
        Ok(())
    }

    /// Mark as dirty (needs saving).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Check if dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get the path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::tempdir;

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peers.json");

        let peers = vec![
            PeerAddress {
                addr: "127.0.0.1:8333".parse().unwrap(),
                last_seen: now_secs(),
            },
            PeerAddress {
                addr: "127.0.0.2:8333".parse().unwrap(),
                last_seen: now_secs() - 100,
            },
        ];

        // Save
        save_peers(&path, &peers).await.unwrap();

        // Load
        let loaded = load_peers(&path).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].addr, peers[0].addr);
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        let peers = load_peers(&path).await.unwrap();
        assert!(peers.is_empty());
    }
}
