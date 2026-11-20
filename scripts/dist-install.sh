#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# install.sh — offline installer bundled inside the zyvor-fabric distribution
# tar.gz. Installs prebuilt binaries only: no git, no cargo, no npm, no
# network access required. Run as root (or with sudo) from the extracted
# package directory:
#
#   tar xzf zyvor-fabric-<version>-linux-<arch>.tar.gz
#   cd zyvor-fabric-<version>-linux-<arch>
#   sudo ./install.sh [--start]
#
# --start   also enable+start zyvor-fabricd.service and fluxvm.service
#           (otherwise they're installed but left stopped/disabled)
set -euo pipefail

PKG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
START=false
[[ "${1:-}" == "--start" ]] && START=true

[[ $EUID -eq 0 ]] || { echo "Run as root: sudo ./install.sh" >&2; exit 1; }
[[ -f "$PKG_DIR/VERSION" ]] || { echo "Not a zyvor-fabric distribution package (VERSION missing)" >&2; exit 1; }

VERSION="$(cat "$PKG_DIR/VERSION")"
echo "=== Zyvor Fabric offline installer — v${VERSION} ==="
echo "Includes: zyvor-fabricd (control plane) + FluxVM (VM engine)"
echo ""

echo "[1/7] Installing zyvor-fabric binaries..."
install -d /usr/bin
for bin in zyvor-fabricd fabricctl; do
    [[ -f "$PKG_DIR/bin/$bin" ]] && install -m 0755 "$PKG_DIR/bin/$bin" "/usr/bin/$bin" && echo "  + /usr/bin/$bin"
done
# fabricctl was named zyvorctl before the rename; drop a stale copy
rm -f /usr/bin/zyvorctl /usr/local/bin/zyvorctl

echo "[2/7] Installing FluxVM binary..."
install -d /usr/local/bin
[[ -f "$PKG_DIR/bin/fluxvm" ]] && install -m 0755 "$PKG_DIR/bin/fluxvm" /usr/local/bin/fluxvm && echo "  + /usr/local/bin/fluxvm"

echo "[3/7] Installing vendor guest-side binaries..."
install -d -m 0755 /var/lib/zyvor-fabricd/vendor
for f in guestkit-agent-cli zyvor-guest-agent fluxvm-guest-agent; do
    if [[ -f "$PKG_DIR/vendor/$f" ]]; then
        install -m 0755 "$PKG_DIR/vendor/$f" "/var/lib/zyvor-fabricd/vendor/$f"
        echo "  + /var/lib/zyvor-fabricd/vendor/$f"
    fi
done
[[ -f "$PKG_DIR/vendor/fluxvm-guest-agent.service" ]] && \
    install -m 0644 "$PKG_DIR/vendor/fluxvm-guest-agent.service" /var/lib/zyvor-fabricd/vendor/fluxvm-guest-agent.service

echo "[4/7] Installing configuration (existing files kept as-is)..."
install -d /etc/zyvor-fabricd
[[ -f /etc/zyvor-fabricd/zyvor-fabricd.toml ]] || install -m 0644 "$PKG_DIR/configs/zyvor-fabricd.toml" /etc/zyvor-fabricd/zyvor-fabricd.toml
[[ -f /etc/zyvor-fabricd/zyvor-fabricd.env ]]  || install -m 0644 "$PKG_DIR/configs/zyvor-fabricd.env"  /etc/zyvor-fabricd/zyvor-fabricd.env
install -m 0644 "$PKG_DIR/configs/pam.d/zyvor-fabricd" /etc/pam.d/zyvor-fabricd
install -d /etc/modules-load.d
install -m 0644 "$PKG_DIR/configs/modules-load.d/zyvor-fabricd.conf" /etc/modules-load.d/zyvor-fabricd.conf
if [[ -d /etc/logrotate.d ]]; then
    install -m 0644 "$PKG_DIR/configs/logrotate.d/zyvor-fabricd" /etc/logrotate.d/zyvor-fabricd
fi
[[ -f /etc/fluxvm.toml ]] || install -m 0644 "$PKG_DIR/configs/fluxvm.toml" /etc/fluxvm.toml

echo "[5/7] Creating runtime directories..."
getent group zyvor-fabricd >/dev/null || groupadd -r zyvor-fabricd
install -d /var/lib/zyvor-fabricd/images
install -d -m 0750 /var/lib/zyvor-fabricd/state
install -d /run/zyvor-fabricd
install -d /var/log/zyvor-fabricd
install -d /var/lib/fluxvm
install -d /run/fluxvm

echo "[6/7] Installing systemd units..."
install -d /usr/lib/systemd/system
install -m 0644 "$PKG_DIR/systemd/zyvor-fabricd.service" /usr/lib/systemd/system/zyvor-fabricd.service
install -m 0644 "$PKG_DIR/systemd/fluxvm.service" /usr/lib/systemd/system/fluxvm.service
command -v systemctl &>/dev/null && systemctl daemon-reload

echo "[7/7] Installing web dashboard..."
install -d /usr/share/zyvor-fabricd/web
cp -r "$PKG_DIR"/web/* /usr/share/zyvor-fabricd/web/

echo ""
echo "=== Installation complete (v${VERSION}) ==="
echo ""
if $START; then
    systemctl enable --now fluxvm.service
    systemctl enable --now zyvor-fabricd.service
    sleep 2
    systemctl is-active --quiet fluxvm && echo "  fluxvm: running" || echo "  fluxvm: NOT running (check: journalctl -u fluxvm)"
    systemctl is-active --quiet zyvor-fabricd && echo "  zyvor-fabricd: running" || echo "  zyvor-fabricd: NOT running (check: journalctl -u zyvor-fabricd)"
else
    echo "Start both services with:"
    echo "  sudo systemctl enable --now fluxvm.service"
    echo "  sudo systemctl enable --now zyvor-fabricd.service"
fi
echo ""
echo "Dashboard (self-signed TLS by default): https://localhost:9095"
echo "Admin password: sudo cat /var/lib/zyvor-fabricd/.admin_password"
echo "Manage:         zyvor-fabricd-ctl status · zyvor-fabricd-ctl verify"
echo ""
