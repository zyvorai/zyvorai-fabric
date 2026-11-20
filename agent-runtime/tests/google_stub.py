#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""A stand-in for Google's token endpoint, Gmail API and Calendar API, for the demos e2e.

    google_stub.py API_PORT TOKEN_PORT CERT KEY

The API is HTTPS (the broker injects credentials only over TLS; CERT is signed by a throwaway CA the runtime is told to trust). The token
endpoint is plain loopback HTTP and answers `access_token = "at-" + refresh_token`. Every API request is appended to GOOGLE_STUB_LOG as one JSON
line: method, path (with query), the Authorization header it arrived with, and the body. It also answers the Microsoft Graph paths the Outlook
agents use; for client id "ms-app" the token endpoint rotates the refresh token and logs each request to TOKEN_STUB_LOG.
"""

import json
import os
import ssl
import sys
import threading
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOG = os.environ["GOOGLE_STUB_LOG"]
api_port, token_port, cert, key = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]


class Api(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def reply(self, status, obj):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.send_header("connection", "close")
        self.end_headers()
        self.wfile.write(data)
        self.close_connection = True

    def handle_any(self):
        n = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(n).decode("utf-8", "replace") if n else ""
        with open(LOG, "a") as f:
            f.write(json.dumps({"method": self.command, "path": self.path, "auth": self.headers.get("authorization", ""), "body": body}) + "\n")
        path = urllib.parse.urlparse(self.path).path
        who = self.headers.get("authorization", "").replace("Bearer at-", "")
        if not self.headers.get("authorization", "").startswith("Bearer at-"):
            return self.reply(401, {"error": {"message": "no token"}})
        m = (self.command, path)
        if m == ("GET", "/gmail/v1/users/me/messages"):
            return self.reply(200, {"messages": [{"id": "m1"}, {"id": "m2"}]})
        if self.command == "GET" and path.startswith("/gmail/v1/users/me/messages/"):
            mid = path.rsplit("/", 1)[1]
            subject = {"m1": "Board notes", "m2": "Lunch?"}.get(mid, "?")
            return self.reply(200, {"payload": {"headers": [
                {"name": "From", "value": f"{who}-friend@example.com"}, {"name": "Subject", "value": subject}, {"name": "Date", "value": "Mon, 28 Sep 2026"}]}})
        if m == ("POST", "/gmail/v1/users/me/drafts"):
            return self.reply(200, {"id": "draft-1"})
        if m == ("POST", "/gmail/v1/users/me/messages/send"):
            return self.reply(200, {"id": "sent-1"})
        if m == ("GET", "/calendar/v3/calendars/primary/events"):
            return self.reply(200, {"items": [{"start": {"dateTime": "2026-09-28T19:00:00+02:00"}, "summary": f"{who} dinner", "location": "Home"}]})
        if m == ("POST", "/calendar/v3/calendars/primary/events"):
            return self.reply(200, {"id": "event-1"})
        # Microsoft Graph
        if m == ("GET", "/v1.0/me/mailFolders/inbox/messages"):
            return self.reply(200, {"value": [{"from": {"emailAddress": {"name": "Board", "address": f"{who}-friend@example.com"}}, "subject": "Board notes", "receivedDateTime": "2026-09-28T08:00:00Z"}]})
        if m == ("POST", "/v1.0/me/messages"):
            return self.reply(201, {"id": "AAMkDraft1"})
        if m == ("POST", "/v1.0/me/sendMail"):
            self.send_response(202); self.send_header("content-length", "0"); self.send_header("connection", "close"); self.end_headers()
            self.close_connection = True
            return
        if m == ("GET", "/v1.0/me/calendarView"):
            return self.reply(200, {"value": [{"subject": f"{who} dinner", "start": {"dateTime": "2026-09-28T17:00:00.0000000"}, "location": {"displayName": "Home"}}]})
        if m == ("POST", "/v1.0/me/events"):
            return self.reply(201, {"id": "ev1"})
        return self.reply(404, {"error": {"message": "not found"}})

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = handle_any


class Token(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        form = urllib.parse.parse_qs(self.rfile.read(int(self.headers.get("content-length", "0"))).decode())
        rt = form.get("refresh_token", [""])[0]
        microsoft = form.get("client_id", [""])[0] == "ms-app"
        if microsoft and os.environ.get("TOKEN_STUB_LOG"):
            # what the token endpoint was asked: the refresh token used, the scope, and whether a client secret came along
            with open(os.environ["TOKEN_STUB_LOG"], "a") as f:
                f.write(json.dumps({"refresh_token": rt, "scope": form.get("scope", [""])[0], "client_secret": "client_secret" in form}) + "\n")
        if rt.startswith("revoked"):
            data, status = json.dumps({"error": "invalid_grant"}).encode(), 400
        else:
            reply = {"access_token": "at-" + rt, "expires_in": 3600}
            if microsoft:
                reply["refresh_token"] = rt + "~"   # Microsoft hands back a new refresh token every time
            data, status = json.dumps(reply).encode(), 200
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


api = ThreadingHTTPServer(("127.0.0.1", api_port), Api)
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.load_cert_chain(cert, key)
api.socket = ctx.wrap_socket(api.socket, server_side=True)
threading.Thread(target=api.serve_forever, daemon=True).start()
ThreadingHTTPServer(("127.0.0.1", token_port), Token).serve_forever()
