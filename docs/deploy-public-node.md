# Deploy a Public Tunnels Node on Google Cloud

This guide covers deploying `tunnels-node` on a Google Cloud Compute Engine VM with a public JSON-RPC endpoint and P2P connectivity.

There are two ways to deploy: run the automated script, or follow the manual steps below.

## Automated deployment

The script at `scripts/deploy-node.sh` does everything in one command:

```bash
bash scripts/deploy-node.sh
```

Prerequisites: `gcloud` CLI installed, authenticated (`gcloud auth login`), and a project selected (`gcloud config set project <PROJECT_ID>`).

The script:

1. Creates an `e2-small` VM (`tunnels-node-1`) running Ubuntu 24.04 with a 20GB SSD
2. Opens firewall rules for P2P (TCP 9333) and JSON-RPC (TCP 9334)
3. SSHs into the VM and installs build-essential, librocksdb-dev, pkg-config, libssl-dev, curl, git
4. Installs Rust via rustup
5. Clones the tunnels repo and builds with `cargo build --release`
6. Creates a dedicated `tunnels` system user with data at `/var/lib/tunnels`
7. Installs `tunnels-node` as a systemd service that starts on boot and restarts on failure
8. Prints the VM's external IP with test commands

Edit the variables at the top of the script to change the VM name, zone, or machine type.

## Manual deployment

### Create the VM

In the Google Cloud Console (Compute Engine → VM instances → Create Instance):

- **Name**: `tunnels-node-1`
- **Region**: pick one close to your users
- **Machine type**: `e2-small` (2 vCPU, 2 GB RAM)
- **Boot disk**: Ubuntu 24.04 LTS, 20 GB SSD (increase if you expect significant chain growth)

Click Create, then SSH into the instance.

### Install dependencies

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install -y build-essential librocksdb-dev pkg-config libssl-dev clang libclang-dev curl git
```

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

Verify:

```bash
rustc --version   # should be 1.75 or later
```

### Build tunnels-node

```bash
git clone https://github.com/tunnelsprotocol/tunnels.git
cd tunnels
cargo build --release
```

This takes a few minutes. The binary lands at `target/release/tunnels-node`.

Copy it somewhere permanent:

```bash
sudo cp target/release/tunnels-node /usr/local/bin/
```

### Configure the systemd service

Create a dedicated user:

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin tunnels
sudo mkdir -p /var/lib/tunnels
sudo chown tunnels:tunnels /var/lib/tunnels
```

Create the service file:

```bash
sudo tee /etc/systemd/system/tunnels-node.service > /dev/null << 'EOF'
[Unit]
Description=Tunnels Protocol Node
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=tunnels
Group=tunnels
ExecStart=/usr/local/bin/tunnels-node \
    --data-dir /var/lib/tunnels \
    --listen 0.0.0.0:9333 \
    --rpc-listen 0.0.0.0:9334 \
    --log-level info
Restart=on-failure
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable tunnels-node
sudo systemctl start tunnels-node
```

Check that it's running:

```bash
sudo systemctl status tunnels-node
sudo journalctl -u tunnels-node -f
```

## Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| 9333 | TCP | P2P (libp2p) — peer discovery, block/transaction gossip |
| 9334 | TCP | JSON-RPC API — query state, submit transactions |

## Firewall rules

In the Google Cloud Console (VPC network → Firewall → Create Firewall Rule), create two rules:

**Rule 1: P2P**

- **Name**: `allow-tunnels-p2p`
- **Direction**: Ingress
- **Targets**: Specified target tags → `tunnels-node`
- **Source IP ranges**: `0.0.0.0/0`
- **Protocols and ports**: TCP `9333`

**Rule 2: JSON-RPC**

- **Name**: `allow-tunnels-rpc`
- **Direction**: Ingress
- **Targets**: Specified target tags → `tunnels-node`
- **Source IP ranges**: `0.0.0.0/0`
- **Protocols and ports**: TCP `9334`

Or via `gcloud`:

```bash
gcloud compute firewall-rules create allow-tunnels-p2p \
    --allow tcp:9333 \
    --source-ranges 0.0.0.0/0 \
    --target-tags tunnels-node \
    --description "Tunnels P2P port"

gcloud compute firewall-rules create allow-tunnels-rpc \
    --allow tcp:9334 \
    --source-ranges 0.0.0.0/0 \
    --target-tags tunnels-node \
    --description "Tunnels JSON-RPC API"
```

## Point a subdomain at the VM

1. Get the VM's external IP from the Compute Engine console (or reserve a static IP via VPC network → External IP addresses).

2. In your DNS provider, add an A record:

   ```
   Type: A
   Name: node1
   Value: <VM_EXTERNAL_IP>
   TTL: 300
   ```

   This makes `node1.tunnelsprotocol.org` resolve to your VM.

3. Verify:

   ```bash
   dig node1.tunnelsprotocol.org
   ```

## Verify the node

From any machine, test the JSON-RPC endpoint:

```bash
curl -s -X POST http://node1.tunnelsprotocol.org:9334 \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"chain_getBlockHeight","params":[],"id":1}'
```

You should get a JSON response with the current block height.

## Connecting other nodes

Other nodes can connect to yours by passing it as a seed node:

```bash
tunnels-node --seed-nodes node1.tunnelsprotocol.org:9333
```

## Updating

To update to a new version:

```bash
cd ~/tunnels
git pull
cargo build --release
sudo cp target/release/tunnels-node /usr/local/bin/
sudo systemctl restart tunnels-node
```
