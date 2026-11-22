#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
"""A stand-in OpenAI-compatible model endpoint for the demos e2e.

Answers POST /v1/chat/completions with a fixed reply and appends one JSON line per request
(the Authorization header and the request body) to MODEL_STUB_LOG, so the test can check what
reached the "model" and that the key was injected on the host.
"""

import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

LOG = os.environ["MODEL_STUB_LOG"]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):  # keep CI output quiet
        pass

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", "0"))).decode("utf-8", "replace")
        with open(LOG, "a") as f:
            f.write(json.dumps({"path": self.path, "auth": self.headers.get("authorization", ""), "body": body}) + "\n")
        if self.path != "/v1/chat/completions":
            self.send_response(404)
            self.end_headers()
            return
        reply = {"choices": [{"message": {"content": "- Two invoices found\n- <b>Total</b> due: 4,200 EUR"}}]}
        data = json.dumps(reply).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
