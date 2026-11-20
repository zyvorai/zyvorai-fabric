#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""A stand-in push relay for the demos e2e: records every message the runtime sends it."""

import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

LOG = os.environ["RELAY_STUB_LOG"]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", "0"))).decode("utf-8", "replace")
        with open(LOG, "a") as f:
            f.write(json.dumps({"event": self.headers.get("x-zyvor-event"), "sig": self.headers.get("x-zyvor-signature", ""), "body": body}) + "\n")
        self.send_response(200)
        self.end_headers()


HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
