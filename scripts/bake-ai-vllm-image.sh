#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
#
# Bake a vLLM + CUDA golden qcow2 for Fabric AI Workloads.
# Run on a GPU host with virt-customize / virt-install available.
#
# Usage:
#   ./scripts/bake-ai-vllm-image.sh /var/lib/libvirt/images/ubuntu-24.04-cloud.qcow2 \
#       /var/lib/zyvor-fabricd/images/vllm-cuda.qcow2
set -euo pipefail

SRC="${1:?source cloud qcow2}"
DST="${2:?destination qcow2 path}"

if [[ ! -f "$SRC" ]]; then
  echo "source image missing: $SRC" >&2
  exit 1
fi

echo "Copying $SRC → $DST"
sudo mkdir -p "$(dirname "$DST")"
sudo cp -f "$SRC" "$DST"

if ! command -v virt-customize >/dev/null 2>&1; then
  cat <<'EOF'
virt-customize not found. Install libguestfs-tools, then re-run, or manually:

  1. Boot the qcow2 with the host NVIDIA driver + CUDA matching the GPU.
  2. Install vLLM into a venv; verify: vllm serve --help
  3. Install fluxvm-guest-agent / zyvor-guest-agent.
  4. Shut down and point FLUXVM_AI_IMAGE at the snapshot path.

See docs/ai-workloads.md § Golden image runbook.
EOF
  exit 0
fi

echo "Installing packages via virt-customize (this takes a while)…"
sudo virt-customize -a "$DST" \
  --update \
  --install "python3-pip,python3-venv,curl,ca-certificates" \
  --run-command 'python3 -m venv /opt/vllm && /opt/vllm/bin/pip install -U pip && /opt/vllm/bin/pip install vllm' \
  --run-command 'ln -sf /opt/vllm/bin/vllm /usr/local/bin/vllm || true' \
  --run-command 'vllm serve --help >/dev/null' \
  || {
    echo "virt-customize failed — finish CUDA/NVIDIA driver install manually on a live VM." >&2
    exit 1
  }

echo "Golden image ready: $DST"
echo "Set FLUXVM_AI_IMAGE=$DST on the fabricd host."
