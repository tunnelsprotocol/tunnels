# Running a Tunnels Node

> **Note**: This document contains placeholder URLs that will be updated at launch:
> - GitHub repository: `github.com/tunnelsprotocol/tunnels`
> - Seed nodes: `seed1.tunnelsprotocol.org`, `seed2.tunnelsprotocol.org`
> - Discord: `discord.gg/tunnelsprotocol`

This guide explains how to download, configure, and run a Tunnels protocol node.

## System Requirements

### Minimum Requirements

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 2 cores | 4+ cores |
| RAM | 4 GB | 8+ GB |
| Storage | 50 GB SSD | 100+ GB NVMe |
| Network | 10 Mbps | 100+ Mbps |

### Supported Operating Systems

- **Linux**: Ubuntu 20.04+, Debian 11+, Fedora 36+, CentOS 8+
- **macOS**: 12.0 (Monterey) or later
- **Windows**: WSL2 with Ubuntu recommended

### Dependencies

The node requires RocksDB for persistent storage:

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install -y librocksdb-dev clang
```

**Fedora:**
```bash
sudo dnf install -y rocksdb-devel clang
```

**macOS:**
```bash
brew install rocksdb
```

## Installation

### Option 1: Download Pre-built Binary

Download the latest release from the releases page:

```bash
# Linux x86_64
curl -LO https://github.com/tunnelsprotocol/tunnels/releases/latest/download/tunnels-node-linux-x86_64
chmod +x tunnels-node-linux-x86_64
sudo mv tunnels-node-linux-x86_64 /usr/local/bin/tunnels-node

# macOS Apple Silicon
curl -LO https://github.com/tunnelsprotocol/tunnels/releases/latest/download/tunnels-node-macos-aarch64
chmod +x tunnels-node-macos-aarch64
sudo mv tunnels-node-macos-aarch64 /usr/local/bin/tunnels-node
```

Verify the checksum:
```bash
sha256sum tunnels-node
# Compare with checksums.txt from the release
```

### Option 2: Build from Source

Requires Rust 1.75 or later:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone and build
git clone https://github.com/tunnelsprotocol/tunnels.git
cd tunnels
./build.sh

# Binary is in dist/tunnels-node-<platform>
```

## Configuration

### Command Line Options

```
tunnels-node [OPTIONS]

Options:
  --data-dir <PATH>        Data directory for blockchain data
                           [default: ~/.tunnels]

  --listen <ADDRESS>       P2P listen address
                           [default: 0.0.0.0:9333]

  --rpc-listen <ADDRESS>   RPC listen address
                           [default: 127.0.0.1:9334]

  --seed-nodes <NODES>     Comma-separated list of seed nodes
                           [example: node1.example.com:9333,node2.example.com:9333]

  --mine                   Enable mining

  --devnet                 Enable devnet mode (for testing only)

  --log-level <LEVEL>      Log level: trace, debug, info, warn, error
                           [default: info]

  --help                   Print help
  --version                Print version
```

### Default Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| 9333 | TCP | P2P network communication |
| 9334 | TCP | JSON-RPC API |

### Data Directory Structure

```
~/.tunnels/
├── chain/              # Blockchain data (RocksDB)
│   ├── blocks/         # Block storage
│   └── state/          # Protocol state (identities, projects, etc.)
├── peers.json          # Known peer addresses
└── node.log            # Log file (if configured)
```

## Running the Node

### Basic Usage

Start a node with default settings:

```bash
tunnels-node
```

The node will:
1. Initialize the data directory at `~/.tunnels`
2. Start the P2P server on port 9333
3. Start the RPC server on port 9334 (localhost only)
4. Connect to seed nodes and sync the blockchain

### Custom Configuration

Run with custom settings:

```bash
tunnels-node \
  --data-dir /var/lib/tunnels \
  --listen 0.0.0.0:9333 \
  --rpc-listen 0.0.0.0:9334 \
  --seed-nodes seed1.tunnelsprotocol.org:9333,seed2.tunnelsprotocol.org:9333 \
  --log-level info
```

### Running as a Service (systemd)

Create `/etc/systemd/system/tunnels-node.service`:

```ini
[Unit]
Description=Tunnels Protocol Node
After=network.target

[Service]
Type=simple
User=tunnels
Group=tunnels
ExecStart=/usr/local/bin/tunnels-node \
  --data-dir /var/lib/tunnels \
  --log-level info
Restart=always
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo useradd -r -s /bin/false tunnels
sudo mkdir -p /var/lib/tunnels
sudo chown tunnels:tunnels /var/lib/tunnels
sudo systemctl daemon-reload
sudo systemctl enable tunnels-node
sudo systemctl start tunnels-node
```

Check status:

```bash
sudo systemctl status tunnels-node
sudo journalctl -u tunnels-node -f
```

## Devnet Mode (Testing)

For local development and testing, use devnet mode:

```bash
tunnels-node --devnet --mine
```

Devnet mode:
- Uses minimal proof-of-work difficulty
- Enables auto-mining when transactions arrive
- Uses shorter verification windows
- Does NOT connect to mainnet

This is useful for:
- Local development
- Testing transaction flows
- Running integration tests

## Connecting to Seed Nodes

Seed nodes help new nodes discover peers. Use the `--seed-nodes` flag:

```bash
tunnels-node --seed-nodes seed1.tunnelsprotocol.org:9333,seed2.tunnelsprotocol.org:9333
```

You can also add peers dynamically via RPC:

```bash
curl -X POST http://localhost:9334 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"addPeer","params":["192.168.1.100:9333"],"id":1}'
```

## Monitoring

### Log Levels

Available log levels (most to least verbose):
- `trace`: Very detailed debugging information
- `debug`: Debugging information
- `info`: General operational messages (default)
- `warn`: Warning conditions
- `error`: Error conditions

### Health Checks

Check node status via RPC:

```bash
# Get node info
curl -s http://localhost:9334 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"getNodeInfo","params":[],"id":1}' | jq

# Get peer info
curl -s http://localhost:9334 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"getPeerInfo","params":[],"id":1}' | jq

# Get block height
curl -s http://localhost:9334 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"getBlockCount","params":[],"id":1}' | jq
```

### What to Monitor

| Metric | Description | Healthy Range |
|--------|-------------|---------------|
| Block height | Current chain tip | Increasing, matches peers |
| Peer count | Connected peers | 4-8 outbound, accepting inbound |
| Sync status | Whether node is synced | `synced: true` |
| Memory usage | RAM consumption | < 4 GB typical |
| Disk I/O | Storage activity | Varies with block activity |

### Common Log Messages

**Normal Operation:**
```
INFO Starting Tunnels node v0.1.0
INFO P2P server listening on 0.0.0.0:9333
INFO RPC server listening on 127.0.0.1:9334
INFO Connected to peer 198.51.100.42:9333
INFO New block 1234 (hash: 004f22d5...)
INFO Chain synced to height 5678
```

**Warning Signs:**
```
WARN Peer disconnected: connection reset
WARN Block validation failed: invalid state root
WARN High mempool size: 10000 pending transactions
```

**Errors:**
```
ERROR Failed to open database: permission denied
ERROR RPC server bind failed: address already in use
ERROR Chain reorg at height 1000 (depth: 3)
```

## Troubleshooting

### Node Won't Start

**Permission denied on data directory:**
```bash
sudo chown -R $USER:$USER ~/.tunnels
```

**Port already in use:**
```bash
# Check what's using the port
lsof -i :9333
# Use a different port
tunnels-node --listen 0.0.0.0:9335
```

**RocksDB error:**
```bash
# Ensure RocksDB is installed
# Ubuntu: sudo apt install librocksdb-dev
# macOS: brew install rocksdb
```

### Node Not Syncing

1. Check peer connections:
   ```bash
   curl -s localhost:9334 -d '{"jsonrpc":"2.0","method":"getPeerInfo","params":[],"id":1}'
   ```

2. Verify seed nodes are reachable:
   ```bash
   nc -zv seed1.tunnelsprotocol.org 9333
   ```

3. Check firewall rules:
   ```bash
   sudo ufw allow 9333/tcp
   ```

### High Resource Usage

**High CPU:**
- Normal during initial sync
- Check for stuck reorgs in logs

**High Memory:**
- Increase system RAM
- Check for memory leaks (restart node)

**High Disk Usage:**
- Prune old data (if supported)
- Move to larger disk

## Security Considerations

1. **RPC Access**: By default, RPC only listens on localhost. Never expose RPC to the public internet without authentication.

2. **Firewall**: Only open port 9333 for P2P. Keep 9334 (RPC) closed or restricted.

3. **Updates**: Keep the node software updated for security patches.

4. **Backups**: Regularly backup the data directory, especially if running a mining node.

5. **Private Keys**: Never store private keys on the node server. Use a separate secure machine for key management.

## Getting Help

- **Documentation**: [docs/](.)
- **GitHub Issues**: [github.com/tunnelsprotocol/tunnels/issues](https://github.com/tunnelsprotocol/tunnels/issues)
- **Discord**: [discord.gg/tunnelsprotocol](https://discord.gg/tunnelsprotocol)
