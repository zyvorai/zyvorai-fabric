#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Bake a Firecracker-friendly flat ext4 rootfs from the QEMU GPT image and
# register the node22-fc FluxVM template.
#
# Why: Firecracker appends `root=/dev/vda` to the guest cmdline. A partitioned
# Ubuntu cloud image has the filesystem on vda1, so the FC default panics with
# "Unable to mount root fs on unknown-block(254,0)". A flat ext4 rootfs matches
# that cmdline. The guest-agent unit must tolerate vsock built into the FC
# kernel (no /lib/modules/$(uname -r)).
#
# Usage (on the FluxVM host, as a user with sudo):
#   ./scripts/merion-bake-fc-rootfs.sh
#   # optional overrides:
#   MERION_FC_SRC_RAW=/var/lib/fluxvm/images/node22-agent.raw \
#   MERION_FC_OUT=/var/lib/fluxvm/images/node22-fc-rootfs.ext4 \
#   ./scripts/merion-bake-fc-rootfs.sh
#
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_E2E_TEMPLATE+x}" ] || [ -z "${KEEP_E2E_TEMPLATE:-}" ] || export MERION_E2E_TEMPLATE="$KEEP_E2E_TEMPLATE"
[ -n "${MERION_FC_KERNEL+x}" ] || [ -z "${KEEP_FC_KERNEL:-}" ] || export MERION_FC_KERNEL="$KEEP_FC_KERNEL"
[ -n "${MERION_FC_OUT+x}" ] || [ -z "${KEEP_FC_OUT:-}" ] || export MERION_FC_OUT="$KEEP_FC_OUT"
[ -n "${MERION_FC_SRC_RAW+x}" ] || [ -z "${KEEP_FC_SRC_RAW:-}" ] || export MERION_FC_SRC_RAW="$KEEP_FC_SRC_RAW"
[ -n "${MERION_FC_TEMPLATE_DIR+x}" ] || [ -z "${KEEP_FC_TEMPLATE_DIR:-}" ] || export MERION_FC_TEMPLATE_DIR="$KEEP_FC_TEMPLATE_DIR"

set -euo pipefail

SRC="${MERION_FC_SRC_RAW:-/var/lib/fluxvm/images/node22-agent.raw}"
OUT="${MERION_FC_OUT:-/var/lib/fluxvm/images/node22-fc-rootfs.ext4}"
KERNEL="${MERION_FC_KERNEL:-/var/lib/fluxvm/kernels/vmlinux}"
TDIR="${MERION_FC_TEMPLATE_DIR:-/var/lib/fluxvm/templates/node22-fc}"

[[ -f "$SRC" ]] || { echo "missing source image: $SRC" >&2; exit 1; }
[[ -f "$KERNEL" ]] || { echo "missing Firecracker kernel: $KERNEL" >&2; exit 1; }

echo "==> extracting ext4 partition from $SRC"
LOOP=$(sudo losetup -f --show -P "$SRC")
cleanup() { sudo losetup -d "$LOOP" 2>/dev/null || true; }
trap cleanup EXIT

PART=""
for cand in "${LOOP}p1" "${LOOP}p2" "${LOOP}p3"; do
  [[ -b "$cand" ]] || continue
  FSTYPE=$(sudo blkid -o value -s TYPE "$cand" 2>/dev/null || true)
  if [[ "$FSTYPE" == "ext4" ]]; then
    PART=$cand
    break
  fi
done
[[ -n "$PART" ]] || { echo "no ext4 partition on $SRC" >&2; exit 1; }

sudo dd if="$PART" of="$OUT" bs=16M status=progress
sudo losetup -d "$LOOP"
trap - EXIT

echo "==> fixing fstab + guest-agent for Firecracker kernel"
MNT=$(mktemp -d)
sudo mount -o loop "$OUT" "$MNT"
sudo tee "$MNT/etc/fstab" >/dev/null <<'FSTAB'
LABEL=cloudimg-rootfs	/	ext4	defaults	0	1
FSTAB
sudo rm -f "$MNT/etc/modules-load.d/vsock.conf"
if [[ -f "$MNT/etc/modules" ]]; then
  sudo sed -i '/^vsock$/d;/^vmw_vsock_/d;/^vhost_vsock$/d' "$MNT/etc/modules" || true
fi
sudo tee "$MNT/etc/systemd/system/fluxvm-guest-agent.service" >/dev/null <<'UNIT'
[Unit]
Description=Zyvor FluxVM in-guest agent (vsock ping/exec/shutdown)
After=local-fs.target
DefaultDependencies=no
ConditionPathExists=/usr/local/bin/fluxvm-guest-agent

[Service]
Type=simple
# Firecracker kernels often build vsock in; ignore missing module dirs.
ExecStartPre=-/sbin/modprobe vsock
ExecStartPre=-/sbin/modprobe vmw_vsock_virtio_transport_common
ExecStartPre=-/sbin/modprobe vmw_vsock_virtio_transport
ExecStartPre=/bin/sh -c 'for i in $(seq 1 60); do [ -e /dev/vsock ] && exit 0; sleep 0.5; done; echo no /dev/vsock >&2; exit 1'
ExecStart=/usr/local/bin/fluxvm-guest-agent --port 17777
Restart=on-failure
RestartSec=2
StandardOutput=journal+console
StandardError=journal+console

[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl --root="$MNT" enable fluxvm-guest-agent.service >/dev/null
sudo umount "$MNT"
rmdir "$MNT"

echo "==> registering template $TDIR"
sudo mkdir -p "$TDIR"
sudo tee "$TDIR/spec.json" >/dev/null <<JSON
{
  "name": "node22-fc",
  "backend": "flux-vm",
  "image": "$OUT",
  "kernel": "$KERNEL",
  "vcpus": 1,
  "memory_mib": 1024,
  "network": {"mode": "tap", "netns": true, "mac": "52:54:00:9f:a9:2e"},
  "agent": {"enabled": true}
}
JSON

echo "OK — flat FC rootfs at $OUT"
echo "    template node22-fc ready; try:"
echo "    MERION_E2E_TEMPLATE=node22-fc ./scripts/merion-pilot-gate.sh"
