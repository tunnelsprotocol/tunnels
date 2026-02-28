//! P2P networking for the Tunnels protocol.
//!
//! This crate provides peer-to-peer networking capabilities for the Tunnels
//! blockchain, including:
//!
//! - Peer discovery via DNS seeds and peer exchange
//! - Block and transaction propagation (gossip)
//! - Headers-first block synchronization
//! - Connection management with configurable limits
//!
//! # Architecture
//!
//! The P2P layer uses a task-per-peer architecture where each connected peer
//! runs in its own tokio task. Communication between components is handled
//! via channels.
//!
//! ```text
//! Main Task (P2pNode::run())
//! ├── Listener Task (accept incoming)
//! ├── Peer Task 1 (read/write loop)
//! ├── Peer Task 2 (read/write loop)
//! ├── Sync Task (coordinate syncing)
//! └── Discovery Task (periodic peer discovery)
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use tunnels_p2p::{P2pConfig, P2pNode};
//! use tunnels_chain::{ChainState, Mempool};
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! let config = P2pConfig::new("0.0.0.0:8333".parse().unwrap());
//! let chain = Arc::new(RwLock::new(ChainState::new()));
//! let mempool = Arc::new(RwLock::new(Mempool::with_defaults()));
//!
//! let node = P2pNode::new(config, chain, mempool);
//! node.run().await?;
//! ```

pub mod config;
pub mod error;

pub mod protocol;
pub mod peer;
pub mod manager;
pub mod sync;
pub mod gossip;
pub mod discovery;
pub mod node;

// Re-export main types
pub use config::{P2pConfig, NETWORK_MAGIC, MAX_MESSAGE_SIZE, PROTOCOL_VERSION};
pub use error::{P2pError, P2pResult};
pub use node::{P2pNode, P2pStateUpdate, PeerSnapshot as P2pPeerSnapshot};
pub use peer::{PeerId, PeerInfo, PeerState};
pub use protocol::Message;
pub use sync::SyncManager;
