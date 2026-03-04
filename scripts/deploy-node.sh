#!/usr/bin/env bash
#
# Deploy a Tunnels Protocol node to Google Cloud Compute Engine.
#
# Prerequisites:
#   - gcloud CLI installed and authenticated (gcloud auth login)
#   - A GCP project selected (gcloud config set project <PROJECT_ID>)
#
# Usage:
#   bash scripts/deploy-node.sh
#
# This script creates a VM, opens firewall ports, installs dependencies,
# builds tunnels-node from source, and configures it as a systemd service.

set -euo pipefail

# --------------------------------------------------------------------------
# Configuration — edit these if needed
# --------------------------------------------------------------------------

VM_NAME="tunnels-node-1"
ZONE="us-central1-a"
MACHINE_TYPE="e2-small"
IMAGE_FAMILY="ubuntu-2404-lts-amd64"
IMAGE_PROJECT="ubuntu-os-cloud"
BOOT_DISK_SIZE="20GB"
NETWORK_TAG="tunnels-node"

P2P_PORT="9333"
RPC_PORT="9334"

REPO_URL="https://github.com/tunnelsprotocol/tunnels.git"

# --------------------------------------------------------------------------
# Step 1: Create the VM
# --------------------------------------------------------------------------

echo "==> Creating VM: ${VM_NAME} (${MACHINE_TYPE}, ${ZONE})"

gcloud compute instances create "${VM_NAME}" \
    --zone="${ZONE}" \
    --machine-type="${MACHINE_TYPE}" \
    --image-family="${IMAGE_FAMILY}" \
    --image-project="${IMAGE_PROJECT}" \
    --boot-disk-size="${BOOT_DISK_SIZE}" \
    --boot-disk-type=pd-ssd \
    --tags="${NETWORK_TAG}" \
    --metadata=startup-script='#!/bin/bash
echo "VM created successfully" > /tmp/tunnels-boot.log'

echo "==> VM created."

# --------------------------------------------------------------------------
# Step 2: Create firewall rules for P2P and JSON-RPC ports
#
# Uses --no-user-output-enabled to suppress "already exists" noise if
# re-running the script. The || true ensures the script continues if
# the rule already exists.
# --------------------------------------------------------------------------

echo "==> Opening firewall: TCP 80 (HTTP), TCP ${P2P_PORT} (P2P), TCP ${RPC_PORT} (JSON-RPC)"

gcloud compute firewall-rules create allow-tunnels-http \
    --allow="tcp:80" \
    --source-ranges="0.0.0.0/0" \
    --target-tags="${NETWORK_TAG}" \
    --description="Tunnels node status page (HTTP)" \
    2>/dev/null || echo "    Firewall rule allow-tunnels-http already exists, skipping."

gcloud compute firewall-rules create allow-tunnels-p2p \
    --allow="tcp:${P2P_PORT}" \
    --source-ranges="0.0.0.0/0" \
    --target-tags="${NETWORK_TAG}" \
    --description="Tunnels P2P port (libp2p/gossipsub)" \
    2>/dev/null || echo "    Firewall rule allow-tunnels-p2p already exists, skipping."

gcloud compute firewall-rules create allow-tunnels-rpc \
    --allow="tcp:${RPC_PORT}" \
    --source-ranges="0.0.0.0/0" \
    --target-tags="${NETWORK_TAG}" \
    --description="Tunnels JSON-RPC API" \
    2>/dev/null || echo "    Firewall rule allow-tunnels-rpc already exists, skipping."

echo "==> Firewall rules configured."

# --------------------------------------------------------------------------
# Step 3: Wait for SSH to become available
# --------------------------------------------------------------------------

echo "==> Waiting for SSH access..."
for i in $(seq 1 30); do
    if gcloud compute ssh "${VM_NAME}" --zone="${ZONE}" --command="true" 2>/dev/null; then
        break
    fi
    sleep 5
done

# --------------------------------------------------------------------------
# Step 4: Install dependencies, build, and configure systemd
#
# Everything from here runs on the remote VM via a single SSH session.
# --------------------------------------------------------------------------

echo "==> Installing dependencies, building tunnels-node, configuring systemd..."

gcloud compute ssh "${VM_NAME}" --zone="${ZONE}" --command="bash -s" << 'REMOTE_SCRIPT'
set -euo pipefail

# --- Install system packages ---
echo "[remote] Installing system packages..."
sudo apt-get update -qq
sudo apt-get install -y -qq build-essential librocksdb-dev pkg-config libssl-dev clang libclang-dev curl git

# --- Install Rust via rustup ---
echo "[remote] Installing Rust..."
if ! command -v rustc &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "[remote] Rust already installed: $(rustc --version)"
fi
source "$HOME/.cargo/env"

# --- Clone and build ---
echo "[remote] Cloning and building tunnels-node (this takes a few minutes)..."
if [ ! -d "$HOME/tunnels" ]; then
    git clone https://github.com/tunnelsprotocol/tunnels.git "$HOME/tunnels"
fi
cd "$HOME/tunnels"
git pull --ff-only
cargo build --release

# --- Install the binary (stop service first if running to avoid "Text file busy") ---
echo "[remote] Installing binary to /usr/local/bin..."
sudo systemctl stop tunnels-node 2>/dev/null || true
sudo cp target/release/tunnels-node /usr/local/bin/

# --- Create a dedicated system user ---
if ! id tunnels &>/dev/null; then
    sudo useradd --system --no-create-home --shell /usr/sbin/nologin tunnels
fi
sudo mkdir -p /var/lib/tunnels
sudo chown tunnels:tunnels /var/lib/tunnels

# --- Write the systemd service file ---
echo "[remote] Configuring systemd service..."
sudo tee /etc/systemd/system/tunnels-node.service > /dev/null << 'UNIT'
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
UNIT

# --- Enable and start ---
sudo systemctl daemon-reload
sudo systemctl enable tunnels-node
sudo systemctl start tunnels-node

echo "[remote] tunnels-node is running."
sudo systemctl status tunnels-node --no-pager

# --- Set up nginx as a reverse proxy + status page ---
echo "[remote] Setting up nginx..."
sudo apt-get install -y -qq nginx

sudo tee /etc/nginx/sites-available/tunnels > /dev/null << 'NGINX'
server {
    listen 80 default_server;
    listen [::]:80 default_server;
    server_name _;

    root /var/www/tunnels;
    index index.html;

    location / {
        try_files $uri $uri/ =404;
    }

    location /rpc {
        proxy_pass http://127.0.0.1:9334/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header Content-Type "application/json";
    }
}
NGINX

sudo mkdir -p /var/www/tunnels
sudo rm -f /etc/nginx/sites-enabled/default
sudo ln -sf /etc/nginx/sites-available/tunnels /etc/nginx/sites-enabled/tunnels
sudo nginx -t
sudo systemctl enable nginx
sudo systemctl restart nginx
echo "[remote] nginx is running."
REMOTE_SCRIPT

# --------------------------------------------------------------------------
# Step 5: Copy the status page to the VM
# --------------------------------------------------------------------------

echo "==> Uploading status page..."

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
gcloud compute scp "${SCRIPT_DIR}/node-status.html" "${VM_NAME}:/tmp/node-status.html" --zone="${ZONE}"
gcloud compute ssh "${VM_NAME}" --zone="${ZONE}" --command="sudo cp /tmp/node-status.html /var/www/tunnels/index.html"

echo "==> Status page deployed."

# --------------------------------------------------------------------------
# Step 6: Print the VM's external IP
# --------------------------------------------------------------------------

echo ""
echo "========================================="
echo "  Deployment complete."
echo "========================================="

EXTERNAL_IP=$(gcloud compute instances describe "${VM_NAME}" \
    --zone="${ZONE}" \
    --format='get(networkInterfaces[0].accessConfigs[0].natIP)')

echo ""
echo "  VM:       ${VM_NAME}"
echo "  Zone:     ${ZONE}"
echo "  IP:       ${EXTERNAL_IP}"
echo ""
echo "  Status:   http://${EXTERNAL_IP}"
echo "  P2P:      ${EXTERNAL_IP}:${P2P_PORT}"
echo "  RPC:      ${EXTERNAL_IP}:${RPC_PORT}"
echo ""
echo "  Point a subdomain at this IP:"
echo "    A record: node1.tunnelsprotocol.org -> ${EXTERNAL_IP}"
echo ""
echo "  Other nodes can connect with:"
echo "    tunnels-node --seed-nodes ${EXTERNAL_IP}:${P2P_PORT}"
