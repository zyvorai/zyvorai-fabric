#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""One-time consent for the Google connectors (Gmail, Calendar): prints a refresh token.

You bring your own Google OAuth client (Google Cloud console -> APIs & Services -> Credentials ->
"Desktop app"). This script runs the authorization-code flow with PKCE on a loopback port, exchanges
the code for a refresh token, and writes `GOOGLE_REFRESH_TOKEN=...` to a file with mode 0600 (default
`./google-refresh-token.env`). The refresh token is never printed to the terminal. Put the three
values in the Keep host's environment and use docs/merion/connectors/google.credentials.json.
On a host with several people, each person instead stores the token with PUT /v1/connections/google (see
docs/merion/connectors/README.md) and the host uses google.per-person.credentials.json.

    GOOGLE_CLIENT_ID=... GOOGLE_CLIENT_SECRET=... scripts/merion-google-auth.py            # read-only scopes
    scripts/merion-google-auth.py --with-drafts                                            # also Gmail drafts
    scripts/merion-google-auth.py --with-send --with-events                                # also send mail and create events

Nothing is sent anywhere except to Google's own endpoints (or the ones you pass with --auth-url/--token-url).
"""
import argparse, base64, hashlib, http.server, json, os, secrets, sys, threading, urllib.error, urllib.parse, urllib.request, webbrowser

READ = ["https://www.googleapis.com/auth/gmail.readonly", "https://www.googleapis.com/auth/calendar.readonly"]
DRAFTS = ["https://www.googleapis.com/auth/gmail.compose"]
SEND = ["https://www.googleapis.com/auth/gmail.send"]
EVENTS = ["https://www.googleapis.com/auth/calendar.events"]


def pkce():
    verifier = base64.urlsafe_b64encode(secrets.token_bytes(48)).rstrip(b"=").decode()
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b"=").decode()
    return verifier, challenge


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--with-drafts", action="store_true", help="also ask for Gmail compose (drafts; sending stays behind a phone approval)")
    p.add_argument("--with-send", action="store_true", help="also ask for Gmail send (every send stays behind a phone approval)")
    p.add_argument("--with-events", action="store_true", help="also ask to create and edit calendar events (behind a phone approval)")
    p.add_argument("--out", default="google-refresh-token.env", help="file for GOOGLE_REFRESH_TOKEN (mode 0600)")
    p.add_argument("--port", type=int, default=0, help="loopback port (default: any free port)")
    p.add_argument("--no-browser", action="store_true", help="print the URL instead of opening a browser")
    p.add_argument("--auth-url", default="https://accounts.google.com/o/oauth2/v2/auth")
    p.add_argument("--token-url", default="https://oauth2.googleapis.com/token")
    p.add_argument("--timeout", type=int, default=300, help="seconds to wait for the browser")
    a = p.parse_args(argv)
    cid, secret = os.environ.get("GOOGLE_CLIENT_ID", ""), os.environ.get("GOOGLE_CLIENT_SECRET", "")
    if not cid:
        sys.exit("set GOOGLE_CLIENT_ID (and GOOGLE_CLIENT_SECRET) from your Desktop-app OAuth client")

    verifier, challenge = pkce()
    state = secrets.token_urlsafe(24)
    got = {}
    done = threading.Event()

    class H(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):  # never log the query (it carries the code)
            pass

        def do_GET(self):
            q = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
            if urllib.parse.urlparse(self.path).path != "/callback":
                self.send_response(404); self.end_headers(); return
            got["state"], got["code"], got["error"] = (q.get(k, [""])[0] for k in ("state", "code", "error"))
            self.send_response(200); self.send_header("content-type", "text/plain; charset=utf-8"); self.end_headers()
            self.wfile.write(b"Keep: you can close this tab and return to the terminal.\n")
            done.set()

    srv = http.server.HTTPServer(("127.0.0.1", a.port), H)
    redirect = f"http://127.0.0.1:{srv.server_address[1]}/callback"
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = a.auth_url + "?" + urllib.parse.urlencode({
        "client_id": cid, "redirect_uri": redirect, "response_type": "code",
        "scope": " ".join(READ + (DRAFTS if a.with_drafts else []) + (SEND if a.with_send else []) + (EVENTS if a.with_events else [])),
        "code_challenge": challenge, "code_challenge_method": "S256", "state": state,
        "access_type": "offline", "prompt": "consent",
    })
    print("Open this URL and approve access:\n" + url if a.no_browser else "Opening your browser for consent...", flush=True)
    if not a.no_browser:
        webbrowser.open(url)
    if not done.wait(a.timeout):
        sys.exit("timed out waiting for the browser")
    srv.shutdown()
    if got.get("error"):
        sys.exit("consent refused: " + got["error"])
    if not secrets.compare_digest(got.get("state", ""), state):
        sys.exit("state mismatch; aborting")

    body = urllib.parse.urlencode({
        "grant_type": "authorization_code", "code": got["code"], "redirect_uri": redirect,
        "client_id": cid, "client_secret": secret, "code_verifier": verifier,
    }).encode()
    req = urllib.request.Request(a.token_url, data=body, headers={"content-type": "application/x-www-form-urlencoded"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            tok = json.load(r)
    except urllib.error.HTTPError as e:
        try:
            err = json.load(e).get("error", "unknown")
        except Exception:
            err = "unknown"
        sys.exit(f"token endpoint refused the code: {e.code} {err}")
    if not tok.get("refresh_token"):
        sys.exit("no refresh_token returned (revoke the app's access in your Google account and run again)")
    fd = os.open(a.out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write("GOOGLE_REFRESH_TOKEN=" + tok["refresh_token"] + "\n")
    os.chmod(a.out, 0o600)
    print(f"Wrote {a.out} (mode 600). Scopes granted: {tok.get('scope', '?')}")


if __name__ == "__main__":
    main()
