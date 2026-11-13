#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# Fail closed if this Merion/Keep host is missing the agent egress advertise address.
#   sudo ./scripts/merion-check-agent-egress.sh
# Optional: ENV_FILE=/etc/zyvor-fabricd/zyvor-fabric-agent.env
set -euo pipefail
ENV_FILE="${ENV_FILE:-/etc/zyvor-fabricd/zyvor-fabric-agent.env}"
MERION_URL="${MERION_URL:-http://127.0.0.1:9096}"

if [[ ! -r "$ENV_FILE" ]]; then
  echo "FAIL: $ENV_FILE missing (deploy Merion first)" >&2
  exit 1
fi
HOST="$(sed -n 's/^ZYVOR_AGENT_EGRESS_ADVERTISE_HOST=//p' "$ENV_FILE" | head -1)"
if [[ -z "$HOST" ]]; then
  echo "FAIL: ZYVOR_AGENT_EGRESS_ADVERTISE_HOST unset in $ENV_FILE" >&2
  echo "      set it (e.g. 169.254.0.1) or re-run scripts/deploy-merion.sh / merion-up.sh" >&2
  exit 1
fi
echo "ok   ZYVOR_AGENT_EGRESS_ADVERTISE_HOST=$HOST"

if curl -fsS -m 3 "$MERION_URL/healthz" >/dev/null 2>&1; then
  echo "ok   runtime healthy at $MERION_URL"
else
  echo "FAIL: runtime not healthy at $MERION_URL" >&2
  exit 1
fi

if command -v dnsmasq >/dev/null 2>&1; then
  echo "ok   dnsmasq present (needed for guest DHCP on tap+netns)"
else
  echo "warn dnsmasq not installed — sealed cells may still work; agent sessions with guest NIC may not get a gateway"
fi
exit 0
