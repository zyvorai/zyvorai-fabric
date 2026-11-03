#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""One-time consent for the Microsoft 365 connectors (Outlook mail, calendar): writes a refresh token to a file.

Run it on your own machine. It opens your browser for Microsoft's consent page (authorization code with PKCE on a loopback port),
exchanges the code for a refresh token, and writes `MICROSOFT_REFRESH_TOKEN=...` to a file with mode 0600 (default
`./microsoft-refresh-token.env`). The token is never printed. Then give it to your Keep host as YOUR connection (Microsoft refresh tokens
rotate, so Keep keeps them per person and stores each new one; see docs/merion/connectors/README.md):

    curl -X PUT "$KEEP/v1/connections/microsoft" -H "Authorization: Bearer $USER_TOKEN" -H 'content-type: application/json' \\
         -d "{\\"refresh_token\\": \\"$(sed 's/^MICROSOFT_REFRESH_TOKEN=//' microsoft-refresh-token.env)\\"}"

You need an app registration (Microsoft Entra admin center, "App registrations"): a **public client** with the redirect URI
`http://localhost` (platform "Mobile and desktop applications") and the delegated Microsoft Graph permissions you will ask for. There is no
client secret. Put its Application (client) ID in MICROSOFT_CLIENT_ID.

    MICROSOFT_CLIENT_ID=... scripts/merion-microsoft-auth.py                    # read-only mail and calendar
    scripts/merion-microsoft-auth.py --with-drafts                              # also create drafts (Mail.ReadWrite)
    scripts/merion-microsoft-auth.py --with-send --with-events                  # also send mail and create events
    scripts/merion-microsoft-auth.py --tenant contoso.onmicrosoft.com           # a work or school directory (default: common)

Nothing is sent anywhere except to Microsoft's own endpoints (or the ones you pass with --auth-url/--token-url).
"""
import argparse, base64, hashlib, http.server, json, os, secrets, sys, threading, urllib.error, urllib.parse, urllib.request, webbrowser

GRAPH = "https://graph.microsoft.com/"
READ = [GRAPH + "Mail.Read", GRAPH + "Calendars.Read"]
DRAFTS = [GRAPH + "Mail.ReadWrite"]
SEND = [GRAPH + "Mail.Send"]
EVENTS = [GRAPH + "Calendars.ReadWrite"]


def pkce():
    verifier = base64.urlsafe_b64encode(secrets.token_bytes(48)).rstrip(b"=").decode()
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b"=").decode()
    return verifier, challenge


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--with-drafts", action="store_true", help="also ask for Mail.ReadWrite (create drafts; every draft stays behind a phone approval)")
    p.add_argument("--with-send", action="store_true", help="also ask for Mail.Send (every send stays behind a phone approval)")
    p.add_argument("--with-events", action="store_true", help="also ask for Calendars.ReadWrite (create events; behind a phone approval)")
    p.add_argument("--tenant", default="common", help="common (any account), organizations, consumers, or a tenant id or domain")
    p.add_argument("--out", default="microsoft-refresh-token.env", help="file for MICROSOFT_REFRESH_TOKEN (mode 0600)")
    p.add_argument("--port", type=int, default=0, help="loopback port (default: any free port)")
    p.add_argument("--no-browser", action="store_true", help="print the URL instead of opening a browser")
    p.add_argument("--auth-url", default=None)
    p.add_argument("--token-url", default=None)
    p.add_argument("--timeout", type=int, default=300, help="seconds to wait for the browser")
    a = p.parse_args(argv)
    cid = os.environ.get("MICROSOFT_CLIENT_ID", "")
    if not cid:
        sys.exit("set MICROSOFT_CLIENT_ID to your app registration's Application (client) ID")
    base = "https://login.microsoftonline.com/" + urllib.parse.quote(a.tenant, safe=".-") + "/oauth2/v2.0/"
    auth_url, token_url = a.auth_url or base + "authorize", a.token_url or base + "token"

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
    # Microsoft matches a registered loopback redirect on the host name "localhost", whatever the port
    redirect = f"http://localhost:{srv.server_address[1]}/callback"
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    scope = ["offline_access"] + READ + (DRAFTS if a.with_drafts else []) + (SEND if a.with_send else []) + (EVENTS if a.with_events else [])
    url = auth_url + "?" + urllib.parse.urlencode({
        "client_id": cid, "redirect_uri": redirect, "response_type": "code", "response_mode": "query",
        "scope": " ".join(scope), "code_challenge": challenge, "code_challenge_method": "S256", "state": state, "prompt": "select_account",
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
        "client_id": cid, "code_verifier": verifier, "scope": " ".join(scope),
    }).encode()
    req = urllib.request.Request(token_url, data=body, headers={"content-type": "application/x-www-form-urlencoded"})
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
        sys.exit("no refresh_token returned (the offline_access permission must be allowed for the app)")
    fd = os.open(a.out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write("MICROSOFT_REFRESH_TOKEN=" + tok["refresh_token"] + "\n")
    os.chmod(a.out, 0o600)
    print(f"Wrote {a.out} (mode 600). Scopes granted: {tok.get('scope', '?')}")


if __name__ == "__main__":
    main()
