#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# scripts/build-vendor-binaries.sh — build & install the three guest-side
# binaries zyvor-fabricd serves/injects but does not build itself:
#
#   guestkit-agent-cli   guestkit's own CLI, rebuilt with `--features agent`
#                        so `guestkit agent-inject` works (the distro-installed
#                        `guestkit` binary is built WITHOUT that feature and
#                        will refuse the subcommand -- this is a second,
#                        differently-named copy, not a replacement for it).
#   zyvor-guest-agent    the in-guest agent CloudInitTab.tsx's default
#                        user-data curls from /vendor/zyvor-guest-agent and
#                        installs into new VMs at first boot.
#   fluxvm-guest-agent FluxVM's own in-guest vsock agent (ping/exec/
#                        put-file/get-file/shutdown) -- required for
#                        Console/Terminal access on any VM. Built as a musl
#                        static binary so it runs on any guest libc.
#
# All three come from source trees OUTSIDE this repo (guestkit, FluxVM are
# separate git repos) and are not part of zyvor-fabricd's own `cargo build`,
# so they can't live in deploy-remote.sh's normal build step. Run this
# whenever guestkit or FluxVM's guest-agent source changes and you want
# that reflected in the vendor binaries new VMs receive -- it is NOT run
# automatically by deploy-remote.sh.
#
# Usage: scripts/build-vendor-binaries.sh user@host
#   GUESTKIT_DIR=/path/to/guestkit    (default: ~/guestkit on the remote)
#   FLUXVM_DIR=/path/to/FluxVM    (default: ~/FluxVM on the remote)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/deploy-common.sh
source "$SCRIPT_DIR/lib/deploy-common.sh"

info() { deploy_ui_info_b "$@"; }
ok()   { deploy_ui_info "$@"; }
warn() { deploy_ui_warn "$@"; }
die()  { deploy_ui_error "$@"; }
phase() { deploy_ui_phase "$@"; }

REMOTE="${1:-}"
[[ -z "$REMOTE" ]] && die "Usage: $0 user@host"

VENDOR_DIR="/var/lib/zyvor-fabricd/vendor"

deploy_ui_banner "Build vendor binaries → ${REMOTE}" "guestkit-agent-cli · zyvor-guest-agent · fluxvm-guest-agent"

phase 1 3 "Build guestkit binaries (agent feature)" "guestkit + zyvor-guest-agent"
ssh "$REMOTE" bash -s <<EOS
set -euo pipefail
export PATH="\${HOME}/.cargo/bin:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:\${PATH}"
GUESTKIT_DIR="${GUESTKIT_DIR:-\$HOME/guestkit}"
[ -d "\$GUESTKIT_DIR" ] || { echo "guestkit checkout not found at \$GUESTKIT_DIR (set GUESTKIT_DIR=)" >&2; exit 1; }
cd "\$GUESTKIT_DIR"
cargo build --release --features agent --bin guestkit --bin zyvor-guest-agent
echo "  ✅ guestkit (agent feature) + zyvor-guest-agent built"
EOS
ok "guestkit binaries built"

phase 2 3 "Build fluxvm-guest-agent (musl static)" "cargo build --target x86_64-unknown-linux-musl"
ssh "$REMOTE" bash -s <<EOS
set -euo pipefail
export PATH="\${HOME}/.cargo/bin:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:\${PATH}"
FLUXVM_DIR="${FLUXVM_DIR:-\$HOME/FluxVM}"
[ -d "\$FLUXVM_DIR" ] || { echo "FluxVM checkout not found at \$FLUXVM_DIR (set FLUXVM_DIR=)" >&2; exit 1; }
cd "\$FLUXVM_DIR"
rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
cargo build --release -p fluxvm-guest-agent --target x86_64-unknown-linux-musl
echo "  ✅ fluxvm-guest-agent (musl) built"
EOS
ok "fluxvm-guest-agent built"

phase 3 3 "Install into $VENDOR_DIR" "atomic swap, preserves service uptime"
ssh "$REMOTE" bash -s <<EOS
set -euo pipefail
GUESTKIT_DIR="${GUESTKIT_DIR:-\$HOME/guestkit}"
FLUXVM_DIR="${FLUXVM_DIR:-\$HOME/FluxVM}"
VENDOR_DIR="$VENDOR_DIR"
SUDO="sudo"
command -v sudo >/dev/null 2>&1 || SUDO=""
\$SUDO install -d -m 755 "\$VENDOR_DIR"

install_atomic() {
    local src="\$1" dest="\$2"
    \$SUDO install -m 755 "\$src" "\$dest.new"
    \$SUDO mv -f "\$dest.new" "\$dest"
    echo "  ✅ \$(basename "\$dest") -> \$dest"
}

install_atomic "\$GUESTKIT_DIR/target/release/guestkit" "\$VENDOR_DIR/guestkit-agent-cli"
install_atomic "\$GUESTKIT_DIR/target/release/zyvor-guest-agent" "\$VENDOR_DIR/zyvor-guest-agent"
install_atomic "\$FLUXVM_DIR/target/x86_64-unknown-linux-musl/release/fluxvm-guest-agent" "\$VENDOR_DIR/fluxvm-guest-agent"

if [ -f "\$FLUXVM_DIR/systemd/fluxvm-guest-agent.service" ]; then
    \$SUDO install -m 644 "\$FLUXVM_DIR/systemd/fluxvm-guest-agent.service" "\$VENDOR_DIR/fluxvm-guest-agent.service"
    echo "  ✅ fluxvm-guest-agent.service -> \$VENDOR_DIR/fluxvm-guest-agent.service"
fi

echo
echo "  ℹ️  zyvor-fabricd itself does not need a restart -- it reads these"
echo "     binaries from disk on each golden-image build / cloud-init render,"
echo "     it doesn't cache them in memory."
EOS
ok "Vendor binaries installed"

echo
info "Note: this only affects VMs/golden images built AFTER this point --"
info "existing images already baked are unaffected until rebuilt."
