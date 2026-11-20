#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# scripts/build-dist.sh — build a self-contained user distribution
# tar.gz: prebuilt zyvor-fabric + FluxVM (+ guestkit vendor agents)
# binaries, web dashboard, configs, systemd units, and an offline
# install.sh. No cargo/npm/rustc required on the user's machine to
# install it — only to build it here.
#
# This repo can't produce Linux binaries on macOS, so the actual `cargo
# build --release` runs on a remote Linux host over SSH (reuses the same
# source trees build-vendor-binaries.sh expects: ~/zyvor-fabric,
# ~/FluxVM, ~/guestkit). The finished tar.gz is pulled back to this
# machine under dist/.
#
# Usage: scripts/build-dist.sh user@host [version]
#   FLUXVM_LOCAL=/path/to/FluxVM   (default: ../FluxVM next to this repo)
#   GUESTKIT_LOCAL=/path/to/guestkit   (default: ../guestkit next to this repo)
#   FLUXVM_DIR=~/FluxVM            (remote checkout path)
#   GUESTKIT_DIR=~/guestkit            (remote checkout path)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/deploy-common.sh
source "$SCRIPT_DIR/lib/deploy-common.sh"

info() { deploy_ui_info_b "$@"; }
ok()   { deploy_ui_info "$@"; }
warn() { deploy_ui_warn "$@"; }
die()  { deploy_ui_error "$@"; }
phase() { deploy_ui_phase "$@"; }

REMOTE="${1:-}"
[[ -z "$REMOTE" ]] && die "Usage: $0 user@host [version]"
shift || true
VERSION="${1:-$(grep -m1 '^version' "$REPO/backend/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')}"
VERSION="${VERSION:-0.0.0}"

FLUXVM_LOCAL="${FLUXVM_LOCAL:-$REPO/../FluxVM}"
GUESTKIT_LOCAL="${GUESTKIT_LOCAL:-$REPO/../guestkit}"
[[ -d "$FLUXVM_LOCAL" ]] || die "FluxVM source not found at $FLUXVM_LOCAL (set FLUXVM_LOCAL=)"
[[ -d "$GUESTKIT_LOCAL" ]] || die "guestkit source not found at $GUESTKIT_LOCAL (set GUESTKIT_LOCAL=)"

RSYNC_EXCLUDES=(--exclude='.git' --exclude='target/' --exclude='node_modules/' --exclude='web/dist/')

deploy_ui_banner "Build distribution package → ${REMOTE}" "zyvor-fabric v${VERSION}"
deploy_ui_kv "🎯" "Build host" "$REMOTE"
deploy_ui_kv "📦" "Bundles" "zyvor-fabric + FluxVM + guestkit vendor agents"

phase 1 6 "Sync source trees to build host" "zyvor-fabric · FluxVM · guestkit"
ssh "$REMOTE" 'mkdir -p ~/zyvor-fabric ~/FluxVM ~/guestkit'
rsync -az --delete "${RSYNC_EXCLUDES[@]}" -e ssh "$REPO/" "$REMOTE:zyvor-fabric/"
rsync -az --delete "${RSYNC_EXCLUDES[@]}" -e ssh "$FLUXVM_LOCAL/" "$REMOTE:FluxVM/"
rsync -az --delete "${RSYNC_EXCLUDES[@]}" -e ssh "$GUESTKIT_LOCAL/" "$REMOTE:guestkit/"
ok "Sources synced"

phase 2 6 "Build zyvor-fabric (release)" "zyvor-fabricd · fabricctl"
ssh "$REMOTE" bash -s <<'EOS'
set -euo pipefail
export PATH="${HOME}/.cargo/bin:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:${PATH}"
cd ~/zyvor-fabric/backend
cargo build --release -p zyvor-fabricd -p fabricctl
echo "  built zyvor-fabricd, fabricctl"
EOS
ok "zyvor-fabric built"

phase 3 6 "Build FluxVM (release + musl guest agent)" "fluxvm · fluxvm-guest-agent"
ssh "$REMOTE" bash -s <<'EOS'
set -euo pipefail
export PATH="${HOME}/.cargo/bin:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:${PATH}"
cd ~/FluxVM
cargo build --release --bin fluxvm
rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
cargo build --release -p fluxvm-guest-agent --target x86_64-unknown-linux-musl
echo "  built fluxvm, fluxvm-guest-agent (musl)"
EOS
ok "FluxVM built"

phase 4 6 "Build guestkit vendor agents (agent feature)" "guestkit-agent-cli · zyvor-guest-agent"
ssh "$REMOTE" bash -s <<'EOS'
set -euo pipefail
export PATH="${HOME}/.cargo/bin:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:${PATH}"
if [ -z "${LIBCLANG_PATH:-}" ] && command -v llvm-config >/dev/null 2>&1; then
    _maj="$(llvm-config --version 2>/dev/null | cut -d. -f1 || true)"
    [ -n "$_maj" ] && [ -d "/usr/lib64/llvm${_maj}/lib64" ] && export LIBCLANG_PATH="/usr/lib64/llvm${_maj}/lib64"
fi
cd ~/guestkit
cargo build --release --features agent --bin guestkit --bin zyvor-guest-agent
echo "  built guestkit (agent feature), zyvor-guest-agent"
EOS
ok "guestkit vendor agents built"

phase 5 6 "Build web dashboard" "npm install · npm run build"
ssh "$REMOTE" bash -s <<'EOS'
set -euo pipefail
export PATH="${HOME}/.cargo/bin:/usr/local/bin:/usr/bin:${PATH}"
cd ~/zyvor-fabric/web
npm install --silent
npm run build
EOS
ok "Web dashboard built"

phase 6 6 "Stage package and create tar.gz" "bin/ vendor/ web/ configs/ systemd/ install.sh"
ARCH="$(ssh "$REMOTE" uname -m)"
PKG="zyvor-fabric-${VERSION}-linux-${ARCH}"
ssh "$REMOTE" bash -s <<EOS
set -euo pipefail
STAGE=~/dist-stage/${PKG}
rm -rf "\$STAGE"
mkdir -p "\$STAGE"/{bin,vendor,web,configs/pam.d,configs/modules-load.d,configs/logrotate.d,systemd}

cp ~/zyvor-fabric/backend/target/release/zyvor-fabricd "\$STAGE/bin/"
cp ~/zyvor-fabric/backend/target/release/fabricctl "\$STAGE/bin/"
cp ~/FluxVM/target/release/fluxvm "\$STAGE/bin/"

cp ~/guestkit/target/release/guestkit "\$STAGE/vendor/guestkit-agent-cli"
cp ~/guestkit/target/release/zyvor-guest-agent "\$STAGE/vendor/zyvor-guest-agent"
cp ~/FluxVM/target/x86_64-unknown-linux-musl/release/fluxvm-guest-agent "\$STAGE/vendor/fluxvm-guest-agent"
[ -f ~/FluxVM/systemd/fluxvm-guest-agent.service ] && cp ~/FluxVM/systemd/fluxvm-guest-agent.service "\$STAGE/vendor/"

cp -r ~/zyvor-fabric/web/dist/* "\$STAGE/web/"

cp ~/zyvor-fabric/configs/zyvor-fabricd.toml "\$STAGE/configs/"
cp ~/zyvor-fabric/configs/zyvor-fabricd.env "\$STAGE/configs/"
cp ~/zyvor-fabric/configs/pam.d/zyvor-fabricd "\$STAGE/configs/pam.d/"
cp ~/zyvor-fabric/configs/modules-load.d/zyvor-fabricd.conf "\$STAGE/configs/modules-load.d/"
cp ~/zyvor-fabric/configs/logrotate.d/zyvor-fabricd "\$STAGE/configs/logrotate.d/"
cp ~/FluxVM/config.example.toml "\$STAGE/configs/fluxvm.toml"

cp ~/zyvor-fabric/systemd/zyvor-fabricd.service "\$STAGE/systemd/"
cp ~/FluxVM/systemd/fluxvm.service "\$STAGE/systemd/"

echo "${VERSION}" > "\$STAGE/VERSION"

chmod 755 "\$STAGE"/bin/* "\$STAGE"/vendor/guestkit-agent-cli "\$STAGE"/vendor/zyvor-guest-agent "\$STAGE"/vendor/fluxvm-guest-agent
EOS

rsync -az -e ssh "$SCRIPT_DIR/dist-install.sh" "$REMOTE:dist-stage/${PKG}/install.sh"
ssh "$REMOTE" "chmod +x ~/dist-stage/${PKG}/install.sh && tar czf ~/dist-stage/${PKG}.tar.gz -C ~/dist-stage ${PKG} && sha256sum ~/dist-stage/${PKG}.tar.gz"

mkdir -p "$REPO/dist"
scp "$REMOTE:dist-stage/${PKG}.tar.gz" "$REPO/dist/"

ok "Package ready: dist/${PKG}.tar.gz"
shasum -a 256 "$REPO/dist/${PKG}.tar.gz" 2>/dev/null || sha256sum "$REPO/dist/${PKG}.tar.gz"
