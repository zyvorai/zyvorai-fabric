#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# ============================================================================
# deploy-keep — Fabric + the Keep runtime on a host that already runs FluxVM
# ============================================================================
#   ./scripts/deploy keep sus@host
#   ./scripts/deploy keep sus@host --dev        # no Keep mode: unsigned packs allowed
#   ./scripts/deploy keep sus@host --dry-run    # print the plan, change nothing
#   ./scripts/deploy-merion.sh local              # this machine is the Keep host (runtime only; used by scripts/merion-up.sh)
#
# What it does, in order:
#   1. makes (or reuses) a Keep signer seed on THIS machine, never sent to the host
#   2. deploys fabricd + the web console (scripts/deploy-remote.sh --quick)
#   3. on the host: builds agent-runtime, installs its systemd unit, writes its env
#      file (API token generated there, your signer's PUBLIC key registered), points
#      fabricd at it, restarts both
#   4. runs the CSV cleanup once as a smoke test (the PDF demos also need pdftotext in the template)
#
# It does not install FluxVM or bake VM templates (bake the cell template on the host with
# scripts/merion-bake-node22-agent.sh, which is in the synced tree). If either is missing it stops
# and says exactly what to do.
# ============================================================================
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_API+x}" ] || [ -z "${KEEP_API:-}" ] || export MERION_API="$KEEP_API"
[ -n "${MERION_POLICY_SEED+x}" ] || [ -z "${KEEP_POLICY_SEED:-}" ] || export MERION_POLICY_SEED="$KEEP_POLICY_SEED"
[ -n "${MERION_SEED_FILE+x}" ] || [ -z "${KEEP_SEED_FILE:-}" ] || export MERION_SEED_FILE="$KEEP_SEED_FILE"
[ -n "${MERION_TOKEN+x}" ] || [ -z "${KEEP_TOKEN:-}" ] || export MERION_TOKEN="$KEEP_TOKEN"

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

TARGET=""
DEV=0
DRY=0
SKIP_FABRIC=0
SEED_FILE="${MERION_SEED_FILE:-$HOME/.config/zyvor/keep-signer.seed}"

usage() {
  sed -n '5,24p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --dev) DEV=1 ;;
    --dry-run) DRY=1 ;;
    --skip-fabric) SKIP_FABRIC=1 ;;
    --seed-file) SEED_FILE="${2:?--seed-file needs a path}"; shift ;;
    -*) echo "unknown option: $1" >&2; usage 64 ;;
    *)
      if [[ -n "$TARGET" ]]; then echo "unexpected argument: $1" >&2; usage 64; fi
      TARGET="$1"
      ;;
  esac
  shift
done
[[ "$TARGET" == *@* || "$TARGET" == local ]] || { echo "usage: deploy keep USER@HOST|local [--dev] [--dry-run]" >&2; exit 64; }
if [[ "$TARGET" == local ]]; then
  SKIP_FABRIC=1                         # `local` installs the Keep runtime only; fabricd and the console are a separate deploy
  REMOTE_USER="$(id -un)"
else
  REMOTE_USER="${TARGET%@*}"
fi

say() { printf '%s\n' "$*"; }
step() { printf '\n==> %s\n' "$*"; }

command -v node >/dev/null || { echo "node is required locally (signer key + fabric-agent)" >&2; exit 1; }
[[ "$TARGET" == local ]] || command -v ssh >/dev/null || { echo "ssh is required" >&2; exit 1; }

# ── 1. signer seed (local only) ──────────────────────────────────────────────
step "Keep signer"
if [[ "$DEV" == "1" ]]; then
  say "  dev mode: no Keep mode, no signer registered (unsigned packs are accepted)"
  PUBKEY=""
else
  if [[ ! -s "$SEED_FILE" ]]; then
    if [[ "$DRY" == "1" ]]; then
      say "  would create a signer seed at $SEED_FILE (mode 0600)"
      PUBKEY="<public key derived from the new seed>"
    else
      mkdir -p "$(dirname "$SEED_FILE")"
      ( umask 077; node -e 'process.stdout.write(require("node:crypto").randomBytes(32).toString("hex")+"\n")' > "$SEED_FILE" )
      chmod 600 "$SEED_FILE"
      say "  created $SEED_FILE (keep it: it signs your deploys; it never leaves this machine)"
    fi
  else
    say "  using $SEED_FILE"
  fi
  if [[ -s "$SEED_FILE" ]]; then
    PUBKEY=$(S="$(tr -d '[:space:]' < "$SEED_FILE")" node --input-type=module -e \
      "import('$REPO_DIR/sdk/agent-runtime/src/sign.js').then(m=>console.log(m.publicKeyHex(process.env.S)))")
  fi
  say "  public key: $PUBKEY"
fi

if [[ "$DRY" == "1" ]]; then
  step "Plan (dry run, nothing changed)"
  [[ "$SKIP_FABRIC" == "1" ]] || say "  1. ./scripts/deploy-remote.sh $TARGET --quick   (fabricd + console + sources)"
  say "  2. on the host: check FluxVM at 127.0.0.1:7788, build agent-runtime, install the systemd unit"
  if [[ "$DEV" == "1" ]]; then
    say "  3. write /etc/zyvor-fabricd/zyvor-fabric-agent.env (API token generated on the host; dev mode, no signer)"
  else
    say "  3. write /etc/zyvor-fabricd/zyvor-fabric-agent.env (API token generated on the host; Keep mode, trusted signer $PUBKEY)"
  fi
  say "  4. add [agent_runtime] to zyvor-fabricd.toml if fabricd is installed; restart the runtime (and fabricd if present)"
  say "  5. smoke test: merion-demo.sh csv-clean on the host"
  exit 0
fi

# ── 2. Fabric + console ─────────────────────────────────────────────────────
if [[ "$SKIP_FABRIC" != "1" ]]; then
  step "Fabric daemon and console"
  "$SCRIPT_DIR/deploy-remote.sh" "$TARGET" --quick
fi

# ── 3+4. the Keep runtime on the host ───────────────────────────────────────
step "Keep runtime on ${TARGET#*@}"
if [[ "$TARGET" == local ]]; then
  REMOTE_DIR="$REPO_DIR"
elif [[ "$REMOTE_USER" == "root" ]]; then
  REMOTE_DIR="/root/zyvor-fabric"
else
  REMOTE_DIR="/home/$REMOTE_USER/zyvor-fabric"
fi

# Run the install script on the host: over ssh, or straight on this machine for `local`.
run_on_host() {
  if [[ "$TARGET" == local ]]; then
    PUBKEY="$PUBKEY" MERION_DEV="$DEV" DIR="$REMOTE_DIR" bash -s
  else
    ssh -o BatchMode=yes "$TARGET" "PUBKEY='$PUBKEY' MERION_DEV='$DEV' DIR='$REMOTE_DIR' bash -s"
  fi
}

set +e
run_on_host <<'REMOTE'
set -euo pipefail
# Non-interactive ssh has a minimal PATH; the Fabric deploy adds the same directories.
export PATH="$HOME/.cargo/bin:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:$PATH"
SUDO=""; [ "$(id -u)" = 0 ] || SUDO="sudo -n"
ok()   { printf '  ✓ %s\n' "$*"; }
info() { printf '  · %s\n' "$*"; }
die()  { printf '  ✗ %s\n' "$*" >&2; exit "${2:-1}"; }

curl -fsS -m 5 http://127.0.0.1:7788/readyz >/dev/null 2>&1 \
  || die "FluxVM is not answering on 127.0.0.1:7788. Install and start FluxVM first: https://github.com/zyvorai/fluxvm" 2
ok "FluxVM is ready"

[ -d "$DIR/agent-runtime" ] || die "sources not found at $DIR (the Fabric deploy step must run first)"
command -v cargo >/dev/null || die "cargo (Rust) is not installed for $(id -un); the Fabric deploy needs it too"
( cd "$DIR/agent-runtime" && cargo build --release 2>&1 | tail -3 )
$SUDO install -m 0755 "$DIR/agent-runtime/target/release/zyvor-fabric-agent-runtime" /usr/bin/zyvor-fabric-agent-runtime
$SUDO install -m 0644 "$DIR/systemd/zyvor-fabric-agent-runtime.service" /etc/systemd/system/zyvor-fabric-agent-runtime.service
$SUDO install -d /etc/zyvor-fabricd /var/lib/zyvor-fabric-agent /var/lib/fluxvm/agent-snapshots
ok "agent-runtime installed"

ENV=/etc/zyvor-fabricd/zyvor-fabric-agent.env
if ! $SUDO test -f "$ENV"; then
  TOKEN=$(openssl rand -hex 32)
  printf 'ZYVOR_AGENT_LISTEN=127.0.0.1:9096\nZYVOR_AGENT_FLUXVM_URL=http://127.0.0.1:7788\nZYVOR_AGENT_API_TOKEN=%s\n' "$TOKEN" \
    | $SUDO tee "$ENV" >/dev/null
  ok "wrote $ENV with a new API token"
else
  TOKEN=$($SUDO sed -n 's/^ZYVOR_AGENT_API_TOKEN=//p' "$ENV" | head -1)
  [ -n "$TOKEN" ] || die "$ENV exists but has no ZYVOR_AGENT_API_TOKEN"
  info "kept the existing $ENV and its token"
fi
$SUDO chmod 600 "$ENV"

set_env() { # idempotent KEY=VALUE in the env file
  $SUDO sed -i "/^$1=/d" "$ENV"
  printf '%s=%s\n' "$1" "$2" | $SUDO tee -a "$ENV" >/dev/null
}
if [ "$MERION_DEV" = "1" ]; then
  info "dev mode: Keep mode left off"
else
  set_env ZYVOR_AGENT_MERION_MODE 1
  set_env ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS "$PUBKEY"
  ok "Keep mode on; trusted signer registered (public key only)"
fi

# Point fabricd at the runtime.
CONF=/etc/zyvor-fabricd/zyvor-fabricd.toml
if $SUDO test -f "$CONF" && ! $SUDO grep -q '^\[agent_runtime\]' "$CONF"; then
  printf '\n[agent_runtime]\nbase_url = "http://127.0.0.1:9096"\n' | $SUDO tee -a "$CONF" >/dev/null
  ok "added [agent_runtime] to $CONF"
fi
FENV=/etc/zyvor-fabricd/zyvor-fabricd.env
$SUDO touch "$FENV"; $SUDO chmod 600 "$FENV"
$SUDO sed -i '/^ZYVOR_FABRICD_AGENT_RUNTIME_TOKEN=/d' "$FENV"
printf 'ZYVOR_FABRICD_AGENT_RUNTIME_TOKEN=%s\n' "$TOKEN" | $SUDO tee -a "$FENV" >/dev/null

$SUDO systemctl daemon-reload
$SUDO systemctl enable --now zyvor-fabric-agent-runtime >/dev/null 2>&1
$SUDO systemctl restart zyvor-fabric-agent-runtime
# fabricd is optional (a Keep-only host has none): restart it only when it is installed
if systemctl list-unit-files zyvor-fabricd.service 2>/dev/null | grep -q '^zyvor-fabricd.service'; then
  $SUDO systemctl restart zyvor-fabricd
fi
for i in $(seq 1 30); do
  curl -fsS -m 2 http://127.0.0.1:9096/healthz >/dev/null 2>&1 && break
  sleep 1
  [ "$i" = 30 ] && die "agent-runtime did not come up: sudo journalctl -u zyvor-fabric-agent-runtime -n 50"
done
ok "agent-runtime is running"

curl -fsS -H "Authorization: Bearer $TOKEN" http://127.0.0.1:9096/v1/merion/status \
  | python3 -c '
import json, sys
s = json.load(sys.stdin)
print("  · Keep mode: %s, trusted signers: %d, use cases: %d built-in" % (s["merion_mode"], s["trusted_signers"], s["demos"]["builtin"]))'

echo
echo "==> smoke test: CSV cleanup (needs no extra tools in the template)"
if MERION_API=http://127.0.0.1:9096 MERION_TOKEN="$TOKEN" "$DIR/scripts/merion-demo.sh" csv-clean >/tmp/keep-smoke.log 2>&1; then
  ok "csv-clean ran in a sealed cell with 0 CONNECT"
else
  tail -5 /tmp/keep-smoke.log | sed 's/^/    /'
  die "the smoke test failed. Usually the '${ZYVOR_DEMO_TEMPLATE:-node22-agent}' template is missing or its guest agent is not answering: bake it on the host with ~/zyvor-fabric/scripts/merion-bake-node22-agent.sh (--dry-run shows what is missing), or set ZYVOR_DEMO_TEMPLATE in $ENV to a template that boots, then re-run this command." 3
fi
# The PDF demos also need pdftotext (poppler) in the template. Report, do not fail.
if MERION_API=http://127.0.0.1:9096 MERION_TOKEN="$TOKEN" "$DIR/scripts/merion-demo.sh" pdf-brief >/tmp/keep-smoke-pdf.log 2>&1; then
  ok "pdf-brief ran too"
else
  info "the PDF demos need a template with pdftotext (poppler-utils); the other demos work. The template built by scripts/merion-bake-node22-agent.sh has it. See docs/tutorials/17-merion-pdf-brief.md"
fi
REMOTE
rc=$?
set -e

if [[ $rc -ne 0 ]]; then
  step "Stopped (exit $rc)"
  say "  Fabric is deployed; the Keep runtime step needs attention (see above). Safe to re-run."
  exit $rc
fi

step "Done"
HOST="${TARGET#*@}"
say "  Console:   https://$HOST:9095/app/merion"
[[ "$DEV" == "1" ]] || say "  Signer:    $SEED_FILE  (export MERION_POLICY_SEED=\$(cat $SEED_FILE) to sign deploys)"
say "  Next:      ./scripts/merionctl deploy examples/merion-agents/invoice-check --test   (use ssh -L 9096:127.0.0.1:9096 $TARGET, MERION_API=http://127.0.0.1:9096)"
say "  Docs:      docs/tutorials/19-build-your-own-use-case.md"
