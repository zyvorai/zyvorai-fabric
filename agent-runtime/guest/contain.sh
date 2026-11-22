#!/bin/sh
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
#
# Run a command as an unprivileged user inside a bubblewrap container. The VM is
# still the outer boundary; this is defense in depth against an escape from the
# agent's own process (a browser exploit, a hostile tool), so that what escapes is
# a nobody in a read-only world, not root in the guest.
#
#   contain.sh <command> [args...]
#
# Fails closed: without bwrap and setpriv the command does not run at all.
#
# A seccomp filter (/opt/zyvor/seccomp.bpf, written next to this script) refuses the syscalls
# an agent has no business making: ptrace, mount, bpf, kexec, module loading, perf, io_uring
# and similar. It is x86_64 only; on any other CPU, or with the filter missing, the command
# does not run.
#
# What the agent gets: a read-only view of the guest (so it cannot edit its own
# bundle, the worker, or this script), a private /tmp, its writable home, no
# capabilities, no new privileges, its own PID/IPC/UTS namespaces.
# What it does not get: root, other users' data under /root, a way to gain
# privileges. Network is shared on purpose (it must reach the egress broker);
# pair with `confinement: strict` so that is the only place it can reach.
# Known gap: the guest agent's vsock channel is reachable from a shared network
# namespace, so set a guest-agent token if the agent is untrusted.
set -eu

user="${ZYVOR_CONTAIN_USER:-agent}"
seccomp="${ZYVOR_CONTAIN_SECCOMP_FILE:-/opt/zyvor/seccomp.bpf}"
home="${ZYVOR_CONTAIN_HOME:-/home/$user}"

if [ "$#" -eq 0 ]; then
  echo "contain.sh: no command given" >&2
  exit 64
fi
for tool in bwrap setpriv; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "contain.sh: $tool is required for inner containment but is not installed" >&2
    exit 127
  fi
done

if [ "$(uname -m)" != x86_64 ]; then
  echo "contain.sh: the syscall filter is x86_64 only; refusing to run without it on $(uname -m)" >&2
  exit 127
fi
if [ ! -r "$seccomp" ]; then
  echo "contain.sh: $seccomp is missing; refusing to run without the syscall filter" >&2
  exit 127
fi

if ! id "$user" >/dev/null 2>&1; then
  useradd --create-home --home-dir "$home" --shell /bin/bash "$user"
fi
uid="$(id -u "$user")"
gid="$(id -g "$user")"
mkdir -p "$home"
chown "$uid:$gid" "$home" 2>/dev/null || true

exec bwrap \
  --die-with-parent --new-session --seccomp 3 \
  --unshare-pid --unshare-ipc --unshare-uts --unshare-cgroup-try \
  --ro-bind / / \
  --dev /dev --proc /proc \
  --tmpfs /tmp --tmpfs /run --tmpfs /root \
  --bind "$home" "$home" \
  --setenv HOME "$home" --setenv USER "$user" \
  --chdir "$home" \
  -- \
  setpriv --reuid="$uid" --regid="$gid" --clear-groups \
    --inh-caps=-all --bounding-set=-all --no-new-privs \
  "$@" 3<"$seccomp"
