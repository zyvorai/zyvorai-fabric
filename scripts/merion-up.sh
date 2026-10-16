#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# ============================================================================
# keep-up — a working Keep host on THIS Linux machine, then the token to connect Solvor
# ============================================================================
#   sudo ./scripts/merion-up.sh --dry-run          # check this machine and print the plan; change nothing
#   sudo ./scripts/merion-up.sh                    # FluxVM is already installed: runtime, cell template, token
#   sudo ./scripts/merion-up.sh --install-fluxvm   # also build and install FluxVM from source (experimental, see docs)
#   sudo ./scripts/merion-up.sh --token-only       # mint a fresh user token for an existing host
#
# Options: --user-id ID (default: the login name), --ttl-days N (1-7, default 7), --no-template
#
# It does, in order: (1) preflight, (2) FluxVM check (or install), (3) the node22-agent cell template (scripts/merion-bake-node22-agent.sh, with poppler and tesseract),
# (4) the Keep runtime via `deploy-merion.sh local` (its smoke test needs the template), (5) the SDK's npm packages for `merionctl deploy`, (6) a scoped user
# token, printed once with the exact Solvor settings. Nothing leaves this machine; the token is not written to disk.
# The cell is sealed only when KVM is present: the preflight refuses without it rather than pretend.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLUXVM_URL="${KEEP_FLUXVM_URL:-http://127.0.0.1:7788}"
KEEP_URL="${KEEP_URL:-http://127.0.0.1:9096}"
ENV_FILE="${KEEP_ENV_FILE:-/etc/zyvor-fabricd/zyvor-fabric-agent.env}"

DRY=0; INSTALL_FLUXVM=0; TOKEN_ONLY=0; NO_TEMPLATE=0
# the person, not root: under sudo the login name is in SUDO_USER
LOGIN_NAME="${SUDO_USER:-$(id -un)}"
USER_ID="$(printf '%s' "$LOGIN_NAME" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9._-' '-' | cut -c1-32)"
TTL_DAYS=7
usage() { sed -n '5,14p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --dry-run) DRY=1 ;;
    --install-fluxvm) INSTALL_FLUXVM=1 ;;
    --token-only) TOKEN_ONLY=1 ;;
    --no-template) NO_TEMPLATE=1 ;;
    --user-id) USER_ID="${2:?--user-id needs a value}"; shift ;;
    --ttl-days) TTL_DAYS="${2:?--ttl-days needs a value}"; shift ;;
    *) echo "unknown option: $1" >&2; usage 64 ;;
  esac
  shift
done
[[ "$USER_ID" =~ ^[a-z0-9._-]{1,32}$ ]] || { echo "--user-id must be 1-32 characters of a-z 0-9 . _ -" >&2; exit 64; }
[[ "$TTL_DAYS" =~ ^[1-7]$ ]] || { echo "--ttl-days must be 1 to 7 (the runtime's limit)" >&2; exit 64; }

step() { printf '\n==> %s\n' "$*"; }
ok()   { printf '  ok   %s\n' "$*"; }
info() { printf '  ..   %s\n' "$*"; }
PROBLEMS=0
bad()  { printf '  FAIL %s\n' "$*" >&2; PROBLEMS=$((PROBLEMS + 1)); }

# Sources of facts, overridable so the checks can be tested without a real host.
uname_s() { echo "${KEEP_UP_UNAME:-$(uname -s)}"; }
uname_m() { echo "${KEEP_UP_ARCH:-$(uname -m)}"; }
mem_kb()  { echo "${KEEP_UP_MEM_KB:-$(awk '/MemTotal/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)}"; }
disk_kb() { echo "${KEEP_UP_DISK_KB:-$(df -Pk "${KEEP_UP_DISK_PATH:-/var/lib}" 2>/dev/null | awk 'NR==2 {print $4}' || echo 0)}"; }
kvm_ok()  { if [[ -n "${KEEP_UP_KVM:-}" ]]; then [[ "$KEEP_UP_KVM" == 1 ]]; else [[ -r /dev/kvm && -w /dev/kvm ]]; fi; }
systemd_ok() { if [[ -n "${KEEP_UP_SYSTEMD:-}" ]]; then [[ "$KEEP_UP_SYSTEMD" == 1 ]]; else [[ -d /run/systemd/system ]]; fi; }
SUDO=""; [[ "$(id -u)" == 0 ]] || SUDO="sudo"
SUDO="${KEEP_SUDO-$SUDO}"   # KEEP_SUDO="" disables sudo (used by the tests)
# A Rust installed with rustup lives in the login user's home; under sudo, HOME and PATH point at root's, so
# cargo is either not found or "could not choose a version". Use the login user's toolchain (read only here).
if [[ "$(id -u)" == 0 && -n "${SUDO_USER:-}" && "$SUDO_USER" != root ]]; then
  USER_HOME="$(getent passwd "$SUDO_USER" | cut -d: -f6)"
  if [[ -x "$USER_HOME/.cargo/bin/cargo" ]]; then
    case ":$PATH:" in *":$USER_HOME/.cargo/bin:"*) ;; *) export PATH="$USER_HOME/.cargo/bin:$PATH" ;; esac
    if [[ -z "${RUSTUP_HOME:-}" && -d "$USER_HOME/.rustup" ]]; then export RUSTUP_HOME="$USER_HOME/.rustup"; fi
  fi
fi

preflight() {
  step "Preflight"
  [[ "$(uname_s)" == Linux ]] && ok "Linux" || bad "this machine is $(uname_s): Keep cells are Linux microVMs on FluxVM. Run this on a Linux host (see docs/merion/README.md); on a Mac, install Solvor and point it at a Linux host"
  case "$(uname_m)" in x86_64|amd64) ok "x86_64" ;; *) bad "architecture $(uname_m): the cell image and the bake script are x86_64 only for now" ;; esac
  if kvm_ok; then ok "/dev/kvm is usable"; else bad "no usable /dev/kvm for $(id -un). Cells are KVM microVMs: run with sudo, enable virtualization (on a cloud VM pick an instance type with nested virtualization or bare metal), or add this user to the kvm group"; fi
  systemd_ok && ok "systemd" || bad "systemd is not running (the runtime is installed as a systemd unit)"
  local mem; mem=$(mem_kb); (( mem >= 3800000 )) && ok "memory $((mem / 1024)) MiB" || bad "memory $((mem / 1024)) MiB: at least 4 GiB (the Rust build and a cell need it)"
  local disk; disk=$(disk_kb); (( disk >= 20000000 )) && ok "free disk $((disk / 1048576)) GiB" || bad "free disk $((disk / 1048576)) GiB under /var/lib: at least 20 GiB (cell images and the build)"
  if [[ "$(id -u)" == 0 ]] || command -v sudo >/dev/null 2>&1; then ok "root or sudo"; else bad "run as root or install sudo"; fi
  local c; for c in git curl python3 openssl node cargo cc; do
    if command -v "$c" >/dev/null 2>&1; then ok "$c"; else bad "$c is missing ($(hint "$c"))"; fi
  done
  if command -v node >/dev/null 2>&1; then
    local nv; nv=$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)
    (( nv >= 20 )) || bad "node $nv is too old: 20 or newer (the signer and the SDK use it)"
  fi
  if [[ -x "${KEEP_NODE_GUEST_AGENT:-/usr/local/bin/fluxvm-guest-agent}" ]]; then ok "FluxVM guest agent binary"
  else info "FluxVM guest agent binary not found yet (installed with FluxVM; the template bake needs it)"; fi
}
hint() {
  case "$1" in
    cargo) echo "install Rust from https://rustup.rs (a rustup install in your home is found under sudo automatically; a Rust installed elsewhere: run sudo env \"PATH=\$PATH\" $0)" ;;
    cc) echo "a C compiler and linker: sudo apt-get install -y build-essential (Fedora: sudo dnf groupinstall \"Development Tools\"); Rust cannot link without one" ;;
    node) echo "install Node 20 or newer, for example from https://nodejs.org" ;;
    *) echo "for example: sudo apt-get install -y $1" ;;
  esac
}

fluxvm_ready() { curl -fsS -m 5 "$FLUXVM_URL/readyz" >/dev/null 2>&1; }
phase_fluxvm() {
  step "FluxVM"
  if fluxvm_ready; then ok "FluxVM answers at $FLUXVM_URL"; return; fi
  if [[ "$INSTALL_FLUXVM" == 0 ]]; then
    bad "FluxVM is not answering at $FLUXVM_URL. Install it first (https://github.com/zyvorai/fluxvm, Quick start), or re-run with --install-fluxvm to build it from source here"
    return
  fi
  info "building FluxVM from source (its README's Quick start; this takes a while and is experimental)"
  if [[ "$DRY" == 1 ]]; then
    info "would: install build dependencies (C toolchain, pkg-config, libsystemd, clang, libbpf headers), clone zyvorai/fluxvm and zyvorai/guestkit side by side, run scripts/bootstrap-host.sh vmbr0, cargo build --release, install fluxctl, fluxvm-hypervisor, a static guest agent, /etc/fluxvm.toml and the fluxvm systemd unit, and start it"
    return
  fi
  # Build dependencies found on a clean Ubuntu 24.04 VM: a C toolchain, pkg-config and libsystemd (guestkit),
  # clang and libbpf (the guest eBPF objects), musl-tools (a static guest agent). The Rust code needs no OpenSSL.
  . /etc/os-release 2>/dev/null || true
  if [[ "${ID:-}" == "debian" || "${ID:-}" == "ubuntu" || "${ID_LIKE:-}" == *"debian"* ]]; then
    $SUDO apt-get update -qq
    $SUDO apt-get install -y -qq build-essential pkg-config libsystemd-dev clang libbpf-dev musl-tools
  elif command -v dnf >/dev/null; then
    $SUDO dnf install -y gcc gcc-c++ make pkgconf-pkg-config systemd-devel clang libbpf-devel musl-gcc
  else
    info "unrecognized package manager: install a C toolchain, pkg-config, libsystemd headers, clang and libbpf headers yourself"
  fi
  local d="${KEEP_FLUXVM_SRC:-/opt/fluxvm-src}"
  $SUDO mkdir -p "$d" && $SUDO chown "$(id -un)" "$d"
  [[ -d "$d/fluxvm" ]] || git clone https://github.com/zyvorai/fluxvm.git "$d/fluxvm"
  [[ -d "$d/guestkit" ]] || git clone https://github.com/zyvorai/guestkit.git "$d/guestkit"
  ( cd "$d/fluxvm" && $SUDO ./scripts/bootstrap-host.sh vmbr0 && ./scripts/preflight.sh && cargo build --release )
  for b in fluxctl fluxvm-hypervisor; do $SUDO install -m 0755 "$d/fluxvm/target/release/$b" "/usr/local/bin/$b"; done
  [[ -f /etc/fluxvm.toml ]] || $SUDO install -m 0644 "$d/fluxvm/config.example.toml" /etc/fluxvm.toml
  # The guest agent goes into every cell image, so it is built static (musl): a glibc-linked one fails to start
  # in a guest with an older glibc. The musl target is added as the login user, who owns the rustup home.
  if [[ ! -x /usr/local/bin/fluxvm-guest-agent ]]; then
    info "building the static guest agent"
    if [[ "$(id -u)" == 0 && -n "${SUDO_USER:-}" && "$SUDO_USER" != root ]]; then
      sudo -u "$SUDO_USER" -H env "PATH=$PATH" rustup target add x86_64-unknown-linux-musl
    else
      rustup target add x86_64-unknown-linux-musl
    fi
    ( cd "$d/fluxvm" && cargo build --release -p fluxvm-guest-agent --target x86_64-unknown-linux-musl )
    $SUDO install -m 0755 "$d/fluxvm/target/x86_64-unknown-linux-musl/release/fluxvm-guest-agent" /usr/local/bin/fluxvm-guest-agent
  fi
  # Start the control plane with FluxVM's own unit.
  if ! fluxvm_ready && systemd_ok; then
    $SUDO install -m 0644 "$d/fluxvm/systemd/fluxvm.service" /etc/systemd/system/fluxvm.service
    # The unit sandboxes the daemon with ReadWritePaths for directories a clean machine does not have yet: systemd
    # fails it with 226/NAMESPACE before any start command runs (an ExecStartPre cannot help). /run is a tmpfs, so
    # tmpfiles.d creates them now and on every boot.
    printf 'd /run/netns 0755 root root -\nd /var/lib/kubelet 0755 root root -\n' \
      | $SUDO tee /etc/tmpfiles.d/fluxvm.conf >/dev/null
    $SUDO systemd-tmpfiles --create /etc/tmpfiles.d/fluxvm.conf
    $SUDO systemctl daemon-reload
    $SUDO systemctl enable --now fluxvm
    local i; for i in $(seq 1 30); do fluxvm_ready && break; sleep 1; done
  fi
  if fluxvm_ready; then ok "FluxVM answers at $FLUXVM_URL"
  else bad "FluxVM is installed and its unit started but is not answering at $FLUXVM_URL: see journalctl -u fluxvm"; fi
}

phase_runtime() {
  step "Keep runtime"
  if [[ "$DRY" == 1 ]]; then info "would run: $SCRIPT_DIR/deploy-merion.sh local   (builds agent-runtime, installs its unit, Keep mode on, csv-clean smoke test)"; return; fi
  "$SCRIPT_DIR/deploy-merion.sh" local
}

# merionctl deploy (custom packs and agents) bundles with esbuild from the SDK, which needs its npm packages once.
# Not needed for the built-in use cases, so a failure here is reported but does not stop the run.
phase_sdk() {
  step "SDK for merionctl deploy (your own packs and agents)"
  local sdk="$SCRIPT_DIR/../sdk/agent-runtime"
  if [[ -d "$sdk/node_modules/esbuild" ]]; then ok "already installed"; return; fi
  if [[ "$DRY" == 1 ]]; then info "would run: npm ci in $sdk (as $LOGIN_NAME)"; return; fi
  local run=(npm ci --no-audit --no-fund)
  if [[ "$(id -u)" == 0 && -n "${SUDO_USER:-}" && "$SUDO_USER" != root ]]; then run=(sudo -u "$SUDO_USER" -H env "PATH=$PATH" "${run[@]}"); fi
  if ( cd "$sdk" && "${run[@]}" ) >/dev/null 2>&1; then ok "installed"
  else info "npm ci failed in $sdk: run it yourself before 'merionctl deploy' (the built-in use cases do not need it)"; fi
}

fluxctl_profile_enforced() {
  [[ -n "${KEEP_UP_AA_ENFORCED:-}" ]] && { [[ "$KEEP_UP_AA_ENFORCED" == 1 ]]; return; }
  [[ -r /sys/kernel/security/apparmor/profiles ]] && $SUDO grep -q '^fluxctl (enforce)' /sys/kernel/security/apparmor/profiles 2>/dev/null
}
template_listed() { curl -fsS -m 5 "$FLUXVM_URL/v1/templates" 2>/dev/null | grep -q '"node22-agent"'; }
phase_template() {
  step "Cell template (node22-agent: Node, poppler, tesseract)"
  if [[ "$NO_TEMPLATE" == 1 ]]; then info "skipped (--no-template)"; return; fi
  if template_listed; then ok "node22-agent is already registered"; return; fi
  if [[ "$DRY" == 1 ]]; then info "would run: $SCRIPT_DIR/merion-bake-node22-agent.sh"; return; fi
  if ! "$SCRIPT_DIR/merion-bake-node22-agent.sh"; then
    if fluxctl_profile_enforced; then
      bad "the template bake failed. FluxVM's AppArmor profile 'fluxctl' is enforced; a FluxVM older than zyvorai/fluxvm#108 and #110 blocks building or running cells under it: update FluxVM (git pull, re-run bootstrap-host.sh) and re-run"
    else
      bad "the template bake failed (see above)"
    fi
  fi
}

operator_token() { $SUDO sed -n 's/^ZYVOR_AGENT_API_TOKEN=//p' "$ENV_FILE" 2>/dev/null | head -1; }
mint_token() {
  local op body resp
  op=$(operator_token); [[ -n "$op" ]] || { echo "no operator token in $ENV_FILE (has the runtime been installed?)" >&2; return 1; }
  body=$(printf '{"user_id":"%s","scopes":["read","run","approve"],"ttl_seconds":%d}' "$USER_ID" $((TTL_DAYS * 86400)))
  resp=$(curl -fsS -m 10 -X POST -H "Authorization: Bearer $op" -H 'content-type: application/json' -d "$body" "$KEEP_URL/v1/user-tokens") \
    || { echo "could not mint a user token at $KEEP_URL (is the runtime running?)" >&2; return 1; }
  python3 -c 'import json,sys; t=json.load(sys.stdin)["token"]; assert t.startswith("kut1."); print(t)' <<<"$resp"
}
phase_token() {
  step "Token for Solvor"
  if [[ "$DRY" == 1 ]]; then info "would mint a $TTL_DAYS-day user token for '$USER_ID' with scopes read, run, approve (never the operator token)"; return; fi
  local tok; tok=$(mint_token) || { bad "no token minted"; return; }
  local host; host=$(hostname 2>/dev/null || echo this-host)
  cat <<OUT

  Keep is up on this machine. To connect Solvor from your Mac:

    1. On the Mac:   ssh -N -L 9096:127.0.0.1:9096 $LOGIN_NAME@$host
    2. In Solvor, Settings:   Host  http://127.0.0.1:9096
                              User  $USER_ID
                              Token $tok
       (or: make run KEEP_HOST=http://127.0.0.1:9096 KEEP_TOKEN=$tok   in the Solvor repo)

  The token is valid for $TTL_DAYS day(s), scoped to '$USER_ID' (read, run, approve), and was not saved anywhere.
  Mint another with: sudo $0 --token-only --user-id $USER_ID
  Evidence class is software-test: the cell has no network, but whoever operates this host can read cell memory.
OUT
}

if [[ "$TOKEN_ONLY" == 1 ]]; then phase_token; (( PROBLEMS == 0 )); exit $?; fi
preflight
phase_fluxvm
if (( PROBLEMS > 0 )); then echo; echo "==> $PROBLEMS problem(s) above; nothing was installed" >&2; exit 1; fi
phase_template   # before the runtime: its deploy ends with a smoke test that needs the template
if (( PROBLEMS > 0 )) && [[ "$DRY" == 0 ]]; then echo; echo "==> $PROBLEMS problem(s) above; the runtime was not deployed" >&2; exit 1; fi
phase_runtime
phase_sdk
phase_token
(( PROBLEMS == 0 )) || exit 1
[[ "$DRY" == 1 ]] && echo && echo "==> dry run: nothing changed"
exit 0
