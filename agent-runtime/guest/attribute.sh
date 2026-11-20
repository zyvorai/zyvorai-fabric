#!/bin/sh
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
#
# Which program owns an established TCP connection? Run by the host through the guest agent.
#
#   attribute.sh <local-port> <remote-port>
#
# Prints one line, "<pid> <exe> <sha256>", and exits 0. Prints nothing and exits 1 when the
# connection or its owner cannot be found. Both arguments must be plain port numbers.
#
# The lookup reads /proc as root: /proc/net/tcp{,6} for the socket inode, then the fd tables for
# the process that holds it, then /proc/<pid>/exe for the program and its hash. It sees processes
# in child PID namespaces (the inner container's). It trusts the guest kernel, so it stops an
# agent *process*, not a compromised guest.
set -u

case "${1:-}${2:-}" in
  ''|*[!0-9]*) exit 2 ;;
esac
[ -n "${1:-}" ] && [ -n "${2:-}" ] || exit 2
[ "$1" -le 65535 ] && [ "$2" -le 65535 ] || exit 2

lp=$(printf '%04X' "$1")
rp=$(printf '%04X' "$2")

# Fields of /proc/net/tcp: sl local rem st tx:rx tr:tm retrnsmt uid timeout inode. State 01 is
# ESTABLISHED.
inode=$(awk -v lp="$lp" -v rp="$rp" '
  FNR > 1 {
    n = split($2, a, ":"); m = split($3, b, ":")
    if (a[n] == lp && b[m] == rp && $4 == "01") { print $10; exit }
  }' /proc/net/tcp /proc/net/tcp6 2>/dev/null)
[ -n "$inode" ] && [ "$inode" != 0 ] || exit 1

fdpath=$(find /proc/[0-9]*/fd -maxdepth 1 -lname "socket:\\[$inode\\]" -print 2>/dev/null | head -n 1)
[ -n "$fdpath" ] || exit 1
pid=${fdpath#/proc/}
pid=${pid%%/*}

exe=$(readlink "/proc/$pid/exe" 2>/dev/null) || exit 1
[ -n "$exe" ] || exit 1
sha=$(sha256sum "/proc/$pid/exe" 2>/dev/null | cut -d' ' -f1)
[ -n "$sha" ] || exit 1
printf '%s %s %s\n' "$pid" "$exe" "$sha"
