#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Tests scripts/merion-google-auth.py against a fake Google (auth and token endpoints on loopback).
The 'browser' is simulated by requesting the redirect URL the script prints."""
import http.server, json, os, re, stat, subprocess, sys, tempfile, threading, urllib.parse, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "..", "..", "scripts", "merion-google-auth.py")


class Fake(http.server.BaseHTTPRequestHandler):
    posts = []
    reply = (200, {"refresh_token": "1//rt-secret", "access_token": "a", "scope": "s"})

    def log_message(self, *a):
        pass

    def do_POST(self):
        Fake.posts.append(urllib.parse.parse_qs(self.rfile.read(int(self.headers["content-length"])).decode()))
        code, body = Fake.reply
        self.send_response(code); self.send_header("content-type", "application/json"); self.end_headers()
        self.wfile.write(json.dumps(body).encode())


def run(extra, env_extra=None, deny=False, tamper_state=False):
    srv = http.server.HTTPServer(("127.0.0.1", 0), Fake)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{srv.server_address[1]}"
    out = os.path.join(tempfile.mkdtemp(), "tok.env")
    env = dict(os.environ, GOOGLE_CLIENT_ID="cid", GOOGLE_CLIENT_SECRET="csecret", **(env_extra or {}))
    p = subprocess.Popen([sys.executable, SCRIPT, "--no-browser", "--out", out, "--auth-url", base + "/auth", "--token-url", base + "/token", "--timeout", "20"] + extra,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    url = ""
    while True:
        line = p.stdout.readline()
        if line.startswith("http"):
            url = line.strip(); break
        if not line:
            break
    q = urllib.parse.parse_qs(urllib.parse.urlparse(url).query)
    cb = q["redirect_uri"][0]
    if deny:
        urllib.request.urlopen(cb + "?error=access_denied&state=" + q["state"][0]).read()
    else:
        urllib.request.urlopen(cb + "?code=CODE123&state=" + ("wrong" if tamper_state else q["state"][0])).read()
    so, se = p.communicate(timeout=30)
    srv.shutdown()
    return p.returncode, url, q, so, se, out


def main():
    Fake.posts.clear(); Fake.reply = (200, {"refresh_token": "1//rt-secret", "access_token": "a", "scope": "s"})
    rc, url, q, so, se, out = run([])
    assert rc == 0, (so, se)
    assert q["code_challenge_method"] == ["S256"] and q["access_type"] == ["offline"] and q["prompt"] == ["consent"], q
    assert "gmail.compose" not in q["scope"][0] and "gmail.readonly" in q["scope"][0] and "calendar.readonly" in q["scope"][0], q["scope"]
    post = Fake.posts[0]
    assert post["grant_type"] == ["authorization_code"] and post["code"] == ["CODE123"] and post["client_secret"] == ["csecret"], post
    assert post["code_verifier"][0] and post["redirect_uri"] == q["redirect_uri"], post
    assert open(out).read() == "GOOGLE_REFRESH_TOKEN=1//rt-secret\n"
    assert stat.S_IMODE(os.stat(out).st_mode) == 0o600
    assert "rt-secret" not in so + se, "refresh token must not be printed"

    rc, url, q, so, se, out = run(["--with-drafts"])
    assert rc == 0 and "gmail.compose" in q["scope"][0], q["scope"]

    rc, url, q, so, se, out = run(["--with-send", "--with-events"])
    scopes = q["scope"][0].split()
    assert rc == 0 and "https://www.googleapis.com/auth/gmail.send" in scopes and "https://www.googleapis.com/auth/calendar.events" in scopes, scopes
    assert not any(s.endswith("gmail.compose") for s in scopes), "send does not imply drafts"

    rc, url, q, so, se, out = run([], tamper_state=True)  # a wrong state aborts
    assert rc != 0 and not os.path.exists(out)

    rc, url, q, so, se, out = run([], deny=True)
    assert rc != 0 and "access_denied" in se and not os.path.exists(out)

    Fake.reply = (200, {"access_token": "a"})
    rc, url, q, so, se, out = run([])
    assert rc != 0 and "no refresh_token" in se and not os.path.exists(out)

    Fake.reply = (400, {"error": "invalid_grant", "error_description": "secret detail"})
    rc, url, q, so, se, out = run([])
    assert rc != 0 and "invalid_grant" in se and "secret detail" not in se and not os.path.exists(out)
    print("keep-google-auth-test: ok")


main()
