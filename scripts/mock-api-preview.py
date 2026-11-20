#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Minimal mock zyvor-fabricd API for local UX preview on macOS."""

from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

TOKEN = "ux-preview-token"
USER = {"id": "1", "username": "admin", "role": "admin", "user_id": "1"}

CAPS = {
    "vm_driver": {"phase": "live", "detail": "mock FluxVM"},
    "storage": {"phase": "live", "detail": "local"},
    "network_security": {"phase": "off", "detail": "preview"},
    "auth": {"phase": "live"},
    "events": {"phase": "unreachable", "detail": "no WS in mock"},
}

VMS = [
    {
        "name": "web-01",
        "state": "running",
        "cpus": 2,
        "memory": 4096,
        "image": "fedora-cloud",
        "ip": "10.0.0.11",
        "tags": ["demo"],
    },
    {
        "name": "db-01",
        "state": "stopped",
        "cpus": 4,
        "memory": 8192,
        "image": "ubuntu-24.04",
        "tags": ["demo", "db"],
    },
]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:
        print("[%s] %s" % (self.log_date_time_string(), fmt % args))

    def _json(self, code: int, body) -> None:
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.send_header("Access-Control-Allow-Methods", "*")
        self.end_headers()
        self.wfile.write(data)

    def do_OPTIONS(self) -> None:
        self._json(204, {})

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        try:
            payload = json.loads(raw.decode() or "{}")
        except json.JSONDecodeError:
            payload = {}

        if path in ("/api/auth/login", "/api/v1/auth/login"):
            user = payload.get("username") or "admin"
            self._json(
                200,
                {
                    "token": TOKEN,
                    "user_id": "1",
                    "role": "admin",
                    "username": user,
                },
            )
            return

        # Accept most mutating calls in preview
        self._json(200, {"ok": True, "preview": True})

    def do_GET(self) -> None:
        path = urlparse(self.path).path

        if path in ("/health", "/api/health"):
            self._json(200, {"status": "ok", "preview": True})
            return

        if path in ("/api/auth/me", "/api/v1/auth/me"):
            auth = self.headers.get("Authorization", "")
            if "Bearer" not in auth and TOKEN not in auth:
                # still allow for preview convenience if token key present in client
                pass
            self._json(200, {"id": "1", "username": "admin", "role": "admin"})
            return

        if path in ("/api/capabilities", "/api/v1/capabilities"):
            self._json(200, CAPS)
            return

        if path in ("/api/vms", "/api/v1/vms"):
            self._json(200, VMS)
            return

        if "/vms/" in path and path.rstrip("/").endswith(tuple(v["name"] for v in VMS)):
            name = path.rstrip("/").split("/")[-1]
            vm = next((v for v in VMS if v["name"] == name), None)
            if vm:
                self._json(200, vm)
                return

        if path.endswith("/metrics") or path in ("/api/metrics", "/api/v1/metrics"):
            self._json(
                200,
                {
                    "cpu_usage": 12.5,
                    "memory_usage": 38.0,
                    "disk_usage": 22.0,
                    "network_rx": 0,
                    "network_tx": 0,
                },
            )
            return

        if path.startswith("/api/events") or path.startswith("/api/v1/events"):
            self._json(200, [])
            return

        # Default empty list for collection endpoints, object for others
        if path.rstrip("/").endswith("s") or "list" in path:
            self._json(200, [])
        else:
            self._json(200, {})

    def do_PUT(self) -> None:
        self.do_POST()

    def do_DELETE(self) -> None:
        self._json(200, {"ok": True})


if __name__ == "__main__":
    host, port = "127.0.0.1", 9095
    print(f"Mock zyvor-fabricd API on http://{host}:{port} (UX preview)")
    ThreadingHTTPServer((host, port), Handler).serve_forever()
