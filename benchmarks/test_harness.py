#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Unit tests for benchmarks/harness.py (in-process HTTP server, no fabricd)."""

from __future__ import annotations

import json
import os
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

import harness  # noqa: E402


class _Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):  # noqa: ANN002
        return

    def do_GET(self):  # noqa: N802
        if self.path in ("/health", "/readyz"):
            body = b'{"ok":true}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path.startswith("/api/vms"):
            auth = self.headers.get("Authorization", "")
            if "Bearer secret" not in auth:
                self.send_response(401)
                self.end_headers()
                return
            body = b'[{"name":"web-1"}]'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(404)
        self.end_headers()


class HarnessTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.httpd = ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
        cls.port = cls.httpd.server_address[1]
        cls.thread = threading.Thread(target=cls.httpd.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.port}"

    @classmethod
    def tearDownClass(cls):
        cls.httpd.shutdown()

    def test_percentile_empty_and_single(self):
        self.assertEqual(harness.percentile([], 99), 0.0)
        self.assertEqual(harness.percentile([7.0], 99), 7.0)
        self.assertGreaterEqual(harness.percentile([1.0, 2.0, 3.0, 4.0], 50), 2.0)

    def test_health_p99_populated(self):
        result = harness.run_series(self.base + "/health", None, 20, 1.0)
        self.assertEqual(result["errors"], 0)
        self.assertEqual(result["ok"], 20)
        self.assertIsNotNone(result["p99_ms"])
        self.assertGreaterEqual(result["p99_ms"], 0.0)

    def test_inventory_requires_token(self):
        denied = harness.run_series(self.base + "/api/vms", None, 5, 1.0)
        self.assertEqual(denied["ok"], 0)
        allowed = harness.run_series(self.base + "/api/vms", "secret", 5, 1.0)
        self.assertEqual(allowed["errors"], 0)
        self.assertEqual(allowed["ok"], 5)

    def test_concurrent_health(self):
        result = harness.run_concurrent(self.base + "/health", None, 4, 16, 1.0)
        self.assertEqual(result["ok"], 16)
        self.assertEqual(result["workers"], 4)

    def test_offline_placeholder_schema(self):
        args = harness.parse_args(
            ["--base-url", "http://127.0.0.1:1", "--allow-offline", "--iterations", "1"]
        )
        report = harness.build_report(args, offline=True)
        self.assertEqual(report["status"], "offline")
        self.assertIn("generated_at", report)
        self.assertFalse(report["labeled_fluxvm_dependent"])

    def test_cli_writes_json(self):
        fd, path = tempfile.mkstemp(suffix=".json")
        os.close(fd)
        try:
            rc = harness.main(
                [
                    "--base-url",
                    self.base,
                    "--token",
                    "secret",
                    "--iterations",
                    "8",
                    "--workers",
                    "2",
                    "--out",
                    path,
                ]
            )
            self.assertEqual(rc, 0)
            data = json.loads(Path(path).read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "ok")
            self.assertIn("health", data["results"])
            self.assertIn("inventory", data["results"])
            self.assertIn("concurrent_health", data["results"])
        finally:
            os.unlink(path)

    def test_cli_offline_without_flag_fails(self):
        rc = harness.main(["--base-url", "http://127.0.0.1:1", "--timeout", "0.2"])
        self.assertEqual(rc, 2)


if __name__ == "__main__":
    unittest.main()
