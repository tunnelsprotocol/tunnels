//! Inbound connection listener.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::config::P2pConfig;
use crate::error::{P2pError, P2pResult};
use crate::peer::{spawn_peer_connection, ConnectionDirection, PeerEvent, PeerId};

/// Event from the inbound listener.
pub enum InboundEvent {
    /// New connection accepted.
    NewConnection {
        peer_id: PeerId,
        addr: SocketAddr,
    },
    /// Listener error.
    Error(P2pError),
}

/// Inbound connection listener.
pub struct InboundListener {
    /// TCP listener.
    listener: TcpListener,
    /// P2P configuration.
    config: Arc<P2pConfig>,
    /// Channel to send events to the manager.
    event_tx: mpsc::Sender<PeerEvent>,
    /// Genesis hash for handshake.
    genesis_hash: [u8; 32],
    /// Next peer ID to assign.
    next_peer_id: u64,
}

impl InboundListener {
    /// Create a new inbound listener.
    pub async fn new(
        config: Arc<P2pConfig>,
        event_tx: mpsc::Sender<PeerEvent>,
        genesis_hash: [u8; 32],
    ) -> P2pResult<Self> {
        let listener = TcpListener::bind(config.bind_addr).await?;
        tracing::info!(addr = %config.bind_addr, "Listening for inbound connections");

        Ok(Self {
            listener,
            config,
            event_tx,
            genesis_hash,
            next_peer_id: 1_000_000, // Start inbound IDs at 1M to distinguish from outbound
        })
    }

    /// Get the local address we're listening on.
    pub fn local_addr(&self) -> P2pResult<SocketAddr> {
        self.listener.local_addr().map_err(P2pError::Io)
    }

    /// Accept the next inbound connection.
    pub async fn accept(
        &mut self,
        best_height: u64,
        best_hash: [u8; 32],
    ) -> P2pResult<(PeerId, SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;

        // Configure the stream
        if let Err(e) = stream.set_nodelay(true) {
            tracing::warn!(addr = %addr, error = %e, "Failed to set TCP_NODELAY");
        }

        // Assign peer ID
        let peer_id = PeerId::new(self.next_peer_id);
        self.next_peer_id += 1;

        tracing::debug!(addr = %addr, peer = %peer_id, "Accepted inbound connection");

        // Spawn peer connection handler
        let (_command_tx, _handle) = spawn_peer_connection(
            peer_id,
            addr,
            ConnectionDirection::Inbound,
            stream,
            self.event_tx.clone(),
            self.config.clone(),
            self.genesis_hash,
            best_height,
            best_hash,
        );

        Ok((peer_id, addr))
    }
}

/// Run the inbound listener task.
pub async fn run_listener(
    mut listener: InboundListener,
    mut shutdown_rx: mpsc::Receiver<()>,
    slots_available: impl Fn() -> bool + Send + 'static,
    get_chain_tip: impl Fn() -> (u64, [u8; 32]) + Send + 'static,
) {
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                tracing::info!("Inbound listener shutting down");
                break;
            }

            result = listener.listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        if !slots_available() {
                            tracing::debug!(addr = %addr, "Rejecting inbound connection: no slots available");
                            drop(stream);
                            continue;
                        }

                        let (best_height, best_hash) = get_chain_tip();

                        // Configure the stream
                        if let Err(e) = stream.set_nodelay(true) {
                            tracing::warn!(addr = %addr, error = %e, "Failed to set TCP_NODELAY");
                        }

                        // Assign peer ID
                        let peer_id = PeerId::new(listener.next_peer_id);
                        listener.next_peer_id += 1;

                        tracing::debug!(addr = %addr, peer = %peer_id, "Accepted inbound connection");

                        // Spawn peer connection handler
                        let (_command_tx, _handle) = spawn_peer_connection(
                            peer_id,
                            addr,
                            ConnectionDirection::Inbound,
                            stream,
                            listener.event_tx.clone(),
                            listener.config.clone(),
                            listener.genesis_hash,
                            best_height,
                            best_hash,
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Error accepting connection");
                    }
                }
            }
        }
    }
}

// Integration tests require actual network setup and are covered in acceptance tests.
