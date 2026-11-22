#!/bin/bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
set -euo pipefail

echo "=== Zyvor Fabric installer (zyvor-fabricd) ==="
echo "Zyvor suite — zyvor.dev · HyperSDK · © 2026"
echo "No git clone required after you have this tree."
echo ""

# Build backend
echo "[1/6] Building backend..."
cd backend
cargo build --release
cd ..

# Build web UI
echo "[2/6] Building web UI..."
cd web
npm install --silent
npm run build
cd ..

# Install binaries
echo "[3/6] Installing binaries..."
sudo install -d /usr/bin
sudo install -m 0755 backend/target/release/zyvor-fabricd  /usr/bin/zyvor-fabricd
sudo install -m 0755 backend/target/release/fabricctl      /usr/bin/fabricctl
# fabricctl was named zyvorctl before the rename; drop a stale copy
sudo rm -f /usr/bin/zyvorctl /usr/local/bin/zyvorctl

# Install config
echo "[4/6] Installing configuration..."
sudo install -d /etc/zyvor-fabricd
sudo install -m 0644 configs/zyvor-fabricd.toml /etc/zyvor-fabricd/zyvor-fabricd.toml
sudo install -m 0644 configs/zyvor-fabricd.env  /etc/zyvor-fabricd/zyvor-fabricd.env
sudo install -m 0644 configs/pam.d/zyvor-fabricd /etc/pam.d/zyvor-fabricd

# Create the zyvor-fabricd system group and runtime directories directly
# (no systemd-sysusers/tmpfiles dependency — the daemon also creates these
# defensively at startup, see daemon.rs::ensure_runtime_dirs, but creating
# them here up front gets ownership/mode right from first boot).
echo "[5/6] Creating system group and directories..."
getent group zyvor-fabricd >/dev/null || sudo groupadd -r zyvor-fabricd
sudo install -d /var/lib/zyvor-fabricd/images
sudo install -d -m 0750 /var/lib/zyvor-fabricd/state
sudo install -d /run/zyvor-fabricd
sudo install -d /var/log/zyvor-fabricd

# Install systemd units (optional — nothing here enables or starts them;
# zyvor-fabricd runs fine without systemd at all, this is only for
# operators who choose to supervise it that way).
sudo install -d /usr/lib/systemd/system
sudo install -m 0644 systemd/zyvor-fabricd.service /usr/lib/systemd/system/zyvor-fabricd.service
if command -v systemctl &>/dev/null; then
    sudo systemctl daemon-reload
fi

sudo install -d /etc/modules-load.d
sudo install -m 0644 configs/modules-load.d/zyvor-fabricd.conf /etc/modules-load.d/zyvor-fabricd.conf

# Install web UI
echo "[6/6] Installing web UI..."
sudo install -d /usr/share/zyvor-fabricd/web
sudo cp -r web/dist/* /usr/share/zyvor-fabricd/web/

echo ""
echo "=== Installation complete ==="
echo ""
echo "To run zyvor-fabricd directly:"
echo "  sudo zyvor-fabricd"
echo ""
echo "Or, if you'd rather run it under systemd:"
echo "  sudo systemctl enable --now zyvor-fabricd.service"
echo "  sudo systemctl status zyvor-fabricd"
echo ""
echo "To enable debug logging:"
echo "  echo 'ZYVOR_FABRICD_LOG_LEVEL=debug' | sudo tee -a /etc/zyvor-fabricd/zyvor-fabricd.env"
echo ""
echo "Access web UI at http://localhost:9095"
echo "Use fabricctl from the command line"
