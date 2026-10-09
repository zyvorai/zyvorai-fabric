#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# Runs scripts/demo/live-speculate.sh on a lab host that has the Keep runtime on :19097 and a throwaway
# FluxVM on :7799 with a seeded session (see docs/assets/demos/README.md). LAB_HOST=user@host required.
: "${LAB_HOST:?set LAB_HOST=user@host}"
ssh -o ConnectTimeout=10 "$LAB_HOST" 'KEEP=http://127.0.0.1:19097 TOKEN=live-token SID=$(cat $HOME/keep-live/sid) FLUX=http://127.0.0.1:7799 VM=$(cat $HOME/keep-live.id) bash -s' < "$(dirname "$0")/live-speculate.sh"
