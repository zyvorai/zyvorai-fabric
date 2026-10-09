#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# ILLUSTRATIVE scripted replay of the credential-broker idea. It prints fixed text: it does not
# run Keep or FluxVM, and it contains no measured numbers. The first line says so on screen.
B=$'\e[1m'; D=$'\e[2m'; G=$'\e[32m'; Y=$'\e[33m'; C=$'\e[36m'; M=$'\e[35m'; R=$'\e[0m'
say() { printf '%s\n' "$*"; sleep 1.1; }
cmd() { printf '%s$ %s%s\n' "$C" "$*" "$R"; sleep 0.8; }

say "${M}${B}ILLUSTRATIVE${R}${M} scripted replay, not a recording${R}"
say "${B}The agent uses an API. It never holds the key.${R}"
say "${Y}host:${R} a person grants the credential, for named hosts, for a while"
cmd "curl -X POST \$KEEP/v1/sessions/\$SID/grants -d '{...}'"
say "${D}201 the response names the grant and its hosts, never the secret${R}"
say "${Y}agent, inside the sandbox:${R}"
cmd "env | grep -i api_key"
say "${D}(nothing)${R}"
cmd "curl -s https://api.example.com/v1/me"
say "${G}200 OK${R} ${D}the host attached the credential in flight${R}"
say "${Y}host:${R} revoke it"
cmd "curl -X DELETE \$KEEP/v1/sessions/\$SID/grants"
cmd "curl -s https://api.example.com/v1/me   ${D}# agent${R}${C}"
say "${G}no credential attached${R} ${D}the request goes out bare${R}"
say "${D}Credentials needing approval, or with method, path or per-user${R}"
say "${D}limits stay with Keep's own broker: a grant cannot enforce those.${R}"
sleep 2
