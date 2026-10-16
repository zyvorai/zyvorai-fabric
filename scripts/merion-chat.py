#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""keep-chat: a small web chat for a Keep agent, over the AG-UI endpoint (POST /v1/agui).

    MERION_API=http://127.0.0.1:9096 MERION_TOKEN=... ./scripts/merion-chat.py --agent echo-agent      # then open http://127.0.0.1:8787

It serves one static page, forwards the chat run (POST /agui) and lets the page list, reopen and forget this agent's conversations
(GET /threads, GET /threads/<id>/messages, DELETE /threads/<id>), read what was done after you approved (/receipts, read-only), manage the person's memory (/memory: read, turn on or off, add, delete, accept or
refuse an agent's proposal) and their goals for this agent (/goals: list, create, pause or resume, cancel). Your token stays in this process: the browser never sees it. The agent is
fixed by --agent (the page cannot choose another), and a conversation is reachable only if the Keep host itself lists it for this agent (and,
with the operator token, for --user): the page cannot name any other thread. Nothing else on the Keep host is reachable through it, it listens
on 127.0.0.1 only, and it refuses requests whose Host or Origin is not itself (DNS rebinding and cross-site posts). A chat cannot approve
anything: an approval the agent asks for is shown as a notice, and it is decided on your own device.
"""
import argparse
import http.client
import json
import os
import re
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "keep-chat")
STATIC = {"/": ("index.html", "text/html; charset=utf-8"), "/app.js": ("app.js", "text/javascript; charset=utf-8"), "/app.css": ("app.css", "text/css; charset=utf-8")}
MAX_BODY = 64 * 1024
CSP = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"


def force_agent(body: bytes, agent: str) -> bytes:
    """The browser's request with `forwardedProps.agent` set to the configured agent, whatever the page sent."""
    data = json.loads(body)
    if not isinstance(data, dict):
        raise ValueError("the request must be a JSON object")
    data["forwardedProps"] = {"agent": agent}
    return json.dumps(data).encode()


UUID = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")


def project_threads(listing, agent: str):
    """What the page needs of the host's thread list: only this agent's threads that a chat can continue (they have a client thread id)."""
    out = []
    for t in (listing or {}).get("items", []):
        if isinstance(t, dict) and t.get("agent") == agent and t.get("client_thread_id") and UUID.fullmatch(str(t.get("id", ""))):
            out.append({k: t.get(k) for k in ("id", "client_thread_id", "title", "updated_at", "message_count")})
    return out


MEMORY_KINDS = ("preference", "fact", "note")


def _item(i):
    return {k: i.get(k) for k in ("id", "text", "kind", "pinned", "tainted", "origin")}


def project_memory(view):
    """What the page needs of the host's memory view: the switch, the accepted entries and the proposals waiting for review."""
    view = view or {}
    return {
        "enabled": bool(view.get("enabled")),
        "items": [_item(i) for i in view.get("items", []) if isinstance(i, dict) and UUID.fullmatch(str(i.get("id", "")))],
        "proposals": [_item(i) for i in view.get("proposals", []) if isinstance(i, dict) and UUID.fullmatch(str(i.get("id", "")))],
    }


def project_goal(g):
    prop = g.get("proposed_plan") if isinstance(g.get("proposed_plan"), dict) else None
    return {
        "id": g.get("id"), "title": g.get("title"), "status": g.get("status"), "autorun": bool(g.get("autorun")), "updated_at": g.get("updated_at"),
        "plan": [{k: st.get(k) for k in ("id", "title", "status", "detail", "attempts")} for st in g.get("plan", []) if isinstance(st, dict)],
        # a plan an agent proposed, waiting for the person: only what they need to read before accepting
        "planning": bool(g.get("planning_session_id")),
        "proposed": None if prop is None else {
            "tainted": bool(prop.get("tainted")),
            "steps": [{"title": s.get("title"), "requires_approval": bool(s.get("requires_approval"))} for s in prop.get("steps", []) if isinstance(s, dict)],
        },
    }


def project_suggestions(view):
    """The host's suggestions for this person, as the page shows them: the switch and what is waiting."""
    view = view or {}
    keep = ("id", "title", "reason", "agent", "tainted")
    return {
        "enabled": bool(view.get("enabled")),
        "pending": [{k: s.get(k) for k in keep} for s in view.get("pending", []) if isinstance(s, dict) and UUID.fullmatch(str(s.get("id", "")))],
    }


def project_receipts(listing):
    """What was done after the person approved, as the page shows it: when, what (method, host, path), the answer, which agent. The
    request body, its digest, the query string and the idempotency key never reach the page."""
    out = []
    for r in (listing or {}).get("items", []):
        if not isinstance(r, dict) or not UUID.fullmatch(str(r.get("id", ""))):
            continue
        u = urllib.parse.urlparse(str(r.get("url", "")))
        out.append({"id": r["id"], "at": r.get("at"), "agent": r.get("agent"), "credential": r.get("credential"), "method": r.get("method"),
                    "host": u.hostname or "", "path": u.path, "status": r.get("status"), "approved": bool(r.get("approval_id"))})
    return out


def project_goals(listing, agent: str):
    """This agent's goals, as the page shows them."""
    return [project_goal(g) for g in (listing or {}).get("items", []) if isinstance(g, dict) and g.get("agent") == agent and UUID.fullmatch(str(g.get("id", "")))]


def clean_goal_request(data):
    """The browser's goal, reduced to a title, up to 20 step titles (none: an agent can propose them) and the run-automatically switch.
    Raises ValueError otherwise."""
    if not isinstance(data, dict):
        raise ValueError("the request must be a JSON object")
    title = str(data.get("title", "")).strip()
    steps = [str(x).strip() for x in data.get("steps", []) if str(x).strip()] if isinstance(data.get("steps", []), list) else None
    if not title or len(title) > 120:
        raise ValueError("a goal needs a title of at most 120 characters")
    if steps is None or len(steps) > 20 or any(len(x) > 200 for x in steps):
        raise ValueError("a goal has at most 20 steps of at most 200 characters")
    if not steps and data.get("autorun"):
        raise ValueError("a goal with no steps cannot run automatically: ask for a plan and accept it first")
    return {"title": title, "steps": steps, "autorun": bool(data.get("autorun"))}


def allowed_host(host_header: str, port: int) -> bool:
    return host_header in (f"127.0.0.1:{port}", f"localhost:{port}")


def make_handler(upstream: str, token: str, agent: str, port: int, user: str = "operator"):
    up = urllib.parse.urlparse(upstream)
    ident = {}   # who the token is, learned once from the host
    if up.scheme not in ("http", "https") or not up.hostname:
        raise SystemExit("MERION_API must be an http(s) URL")

    class Handler(BaseHTTPRequestHandler):
        server_version = "keep-chat"

        def log_message(self, fmt, *args):  # quiet: no request bodies, no tokens
            sys.stderr.write("keep-chat: %s\n" % (fmt % args))

        def _send(self, code, body=b"", ctype="text/plain; charset=utf-8", extra=None):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Security-Policy", CSP)
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Referrer-Policy", "no-referrer")
            for k, v in (extra or {}).items():
                self.send_header(k, v)
            self.end_headers()
            self.wfile.write(body)

        def _upstream(self, method, path, body=None):
            """One short call to the Keep host with the token; returns (status, parsed JSON or None)."""
            conn_cls = http.client.HTTPSConnection if up.scheme == "https" else http.client.HTTPConnection
            conn = conn_cls(up.hostname, up.port or (443 if up.scheme == "https" else 80), timeout=30)
            try:
                headers = {"Accept": "application/json"}
                if token:
                    headers["Authorization"] = "Bearer " + token
                payload = None
                if body is not None:
                    payload = json.dumps(body).encode()
                    headers["Content-Type"] = "application/json"
                conn.request(method, path, body=payload, headers=headers)
                resp = conn.getresponse()
                raw = resp.read(4 * 1024 * 1024)
                try:
                    return resp.status, json.loads(raw) if raw else None
                except ValueError:
                    return resp.status, None
            finally:
                conn.close()

        def _my_threads(self):
            """This agent's threads for this user, as the Keep host lists them."""
            q = urllib.parse.urlencode({"agent": agent, "user_id": user})
            status, listing = self._upstream("GET", "/v1/threads?" + q)
            if status != 200:
                return None
            return project_threads(listing, agent)

        def _origin_ok(self):
            origin = self.headers.get("Origin")
            if origin is not None and origin not in (f"http://127.0.0.1:{port}", f"http://localhost:{port}"):
                self._send(403, b"cross-origin request refused")
                return False
            return True

        def _json(self, code, value):
            self._send(code, json.dumps(value).encode(), "application/json")

        def _thread_route(self, method, path):
            """/threads, /threads/<id>/messages and DELETE /threads/<id>. The id must be one the host lists for this agent and user."""
            try:
                if path == "/threads" and method == "GET":
                    mine = self._my_threads()
                    return self._json(200, {"items": mine}) if mine is not None else self._send(502, b"the Keep host did not list threads")
                m = re.fullmatch(r"/threads/([0-9a-f-]{36})(/messages)?", path)
                if not m or not UUID.fullmatch(m.group(1)):
                    return self._send(404, b"not found")
                tid, messages = m.group(1), bool(m.group(2))
                if (messages and method != "GET") or (not messages and method != "DELETE"):
                    return self._send(404, b"not found")
                mine = self._my_threads()
                if mine is None:
                    return self._send(502, b"the Keep host did not list threads")
                if tid not in {t["id"] for t in mine}:
                    return self._send(404, b"not found")
                if messages:
                    status, data = self._upstream("GET", f"/v1/threads/{tid}/messages")
                    items = [{"role": x.get("role"), "text": x.get("text"), "at": x.get("created_at")} for x in (data or {}).get("items", []) if isinstance(x, dict)]
                    return self._json(200, {"items": items}) if status == 200 else self._send(502, b"the Keep host did not return messages")
                status, _ = self._upstream("DELETE", f"/v1/threads/{tid}")
                return self._send(204 if status == 204 else 502)
            except (OSError, http.client.HTTPException) as e:
                self._send(502, ("could not reach the Keep host: %s" % e).encode())

        def _owner(self):
            """Whose data this page shows: the user the host says the token is, or --user for the operator token (learned once)."""
            if "owner" not in ident:
                status, who = self._upstream("GET", "/v1/whoami")
                if status == 200 and isinstance(who, dict) and who.get("role") == "user" and who.get("user_id"):
                    ident.update(owner=who["user_id"], role="user")
                elif status == 200 and isinstance(who, dict) and who.get("role") == "operator":
                    ident.update(owner=user, role="operator")
                else:
                    return None
            return ident["owner"]

        def _json_body(self):
            try:
                length = int(self.headers.get("Content-Length", "0"))
            except ValueError:
                length = 0
            if length <= 0 or length > MAX_BODY:
                self._send(413, b"the request is empty or too large")
                return None
            try:
                return json.loads(self.rfile.read(length))
            except ValueError:
                self._send(400, b"bad json")
                return None

        def _optional_body(self):
            """A small JSON object, or {} when there is no body. None (after answering) when the body is not an object."""
            try:
                length = int(self.headers.get("Content-Length", "0"))
            except ValueError:
                self._send(400, b"bad content-length")
                return None
            if length <= 0:
                return {}
            data = self._json_body()
            if not isinstance(data, dict):
                if data is not None:
                    self._send(400, b"the body must be a JSON object")
                return None
            return data

        def _suggestions_view(self, owner):
            status, view = self._upstream("GET", "/v1/suggestions?" + urllib.parse.urlencode({"user_id": owner}))
            return project_suggestions(view) if status == 200 else None

        def _my_receipts(self, owner):
            status, listing = self._upstream("GET", "/v1/receipts?" + urllib.parse.urlencode({"user_id": owner, "limit": 50}))
            return project_receipts(listing) if status == 200 else None

        def _memory_view(self, owner):
            status, view = self._upstream("GET", "/v1/memory?" + urllib.parse.urlencode({"user_id": owner}))
            return project_memory(view) if status == 200 else None

        def _my_goals(self, owner):
            status, listing = self._upstream("GET", "/v1/goals?" + urllib.parse.urlencode({"agent": agent, "user_id": owner}))
            return project_goals(listing, agent) if status == 200 else None

        def _person_route(self, method, path):
            """/memory... and /goals...: the person's own memory and this agent's goals for them. An id must be one the host itself lists."""
            try:
                owner = self._owner()
                if owner is None:
                    return self._send(502, b"the Keep host did not say who this is")
                q = "?" + urllib.parse.urlencode({"user_id": owner})
                if path == "/memory" and method == "GET":
                    view = self._memory_view(owner)
                    return self._json(200, view) if view is not None else self._send(502, b"the Keep host did not return memory")
                if path == "/memory/settings" and method == "PUT":
                    data = self._json_body()
                    if data is None:
                        return
                    if not isinstance(data, dict) or not isinstance(data.get("enabled"), bool):
                        return self._send(400, b"enabled must be true or false")
                    st, _ = self._upstream("PUT", "/v1/memory/settings" + q, {"enabled": data["enabled"]})
                    return self._send(204 if st == 200 else 502)
                if path == "/memory" and method == "POST":
                    data = self._json_body()
                    if data is None:
                        return
                    text, kind = (data.get("text") if isinstance(data, dict) else None), (data.get("kind", "note") if isinstance(data, dict) else None)
                    if not isinstance(text, str) or not text.strip() or len(text) > 2000 or kind not in MEMORY_KINDS:
                        return self._send(400, b"text (up to 2000 characters) and a kind of preference, fact or note are required")
                    st, item = self._upstream("POST", "/v1/memory" + q, {"text": text, "kind": kind, "pinned": bool(data.get("pinned"))})
                    if st == 201 and isinstance(item, dict):
                        return self._json(201, _item(item))
                    return self._send(400 if st == 400 else 502, (json.dumps(item) if isinstance(item, dict) else "the Keep host refused").encode(), "application/json")
                m = re.fullmatch(r"/memory/([0-9a-f-]{36})(?:/(accept|reject))?", path)
                if m and UUID.fullmatch(m.group(1)):
                    mid, action = m.group(1), m.group(2)
                    if (action and method != "POST") or (not action and method != "DELETE"):
                        return self._send(404, b"not found")
                    view = self._memory_view(owner)
                    if view is None:
                        return self._send(502, b"the Keep host did not return memory")
                    pool = view["proposals"] if action else view["items"] + view["proposals"]
                    if mid not in {i["id"] for i in pool}:
                        return self._send(404, b"not found")
                    if action:
                        st, _ = self._upstream("POST", f"/v1/memory/{mid}/{action}" + q)
                        return self._send(204 if st == 200 else 502)
                    st, _ = self._upstream("DELETE", f"/v1/memory/{mid}" + q)
                    return self._send(204 if st == 204 else 502)
                if path == "/goals" and method == "GET":
                    goals = self._my_goals(owner)
                    return self._json(200, {"items": goals}) if goals is not None else self._send(502, b"the Keep host did not list goals")
                if path == "/goals" and method == "POST":
                    data = self._json_body()
                    if data is None:
                        return
                    try:
                        want = clean_goal_request(data)
                    except ValueError as e:
                        return self._send(400, str(e).encode())
                    body = {"title": want["title"], "agent": agent, "autorun": want["autorun"], "plan": [{"title": t} for t in want["steps"]]}
                    if ident["role"] == "operator":
                        body["user_id"] = owner
                    st, goal = self._upstream("POST", "/v1/goals", body)
                    if st == 201 and isinstance(goal, dict):
                        return self._json(201, project_goal(goal))
                    return self._send(429 if st == 429 else 400 if st in (400, 404) else 502, (json.dumps(goal) if isinstance(goal, dict) else "the Keep host refused").encode(), "application/json")
                if path == "/receipts" and method == "GET":
                    items = self._my_receipts(owner)
                    return self._json(200, {"items": items}) if items is not None else self._send(502, b"the Keep host did not return receipts")
                if path == "/suggestions" and method == "GET":
                    view = self._suggestions_view(owner)
                    return self._json(200, view) if view is not None else self._send(502, b"the Keep host did not return suggestions")
                if path == "/suggestions/settings" and method == "PUT":
                    data = self._json_body()
                    if data is None:
                        return
                    if not isinstance(data, dict) or not isinstance(data.get("enabled"), bool):
                        return self._send(400, b"enabled must be true or false")
                    st, _ = self._upstream("PUT", "/v1/suggestions/settings" + q, {"enabled": data["enabled"]})
                    return self._send(204 if st == 200 else 502)
                m = re.fullmatch(r"/suggestions/([0-9a-f-]{36})/(accept|dismiss)", path)
                if m and UUID.fullmatch(m.group(1)) and method == "POST":
                    sid, action = m.group(1), m.group(2)
                    data = self._optional_body()
                    if data is None:
                        return
                    view = self._suggestions_view(owner)
                    if view is None:
                        return self._send(502, b"the Keep host did not return suggestions")
                    if sid not in {s["id"] for s in view["pending"]}:
                        return self._send(404, b"not found")
                    body = {"confirm_tainted": True} if action == "accept" and data.get("confirm_tainted") is True else {}
                    st, res = self._upstream("POST", f"/v1/suggestions/{sid}/{action}" + q, body)
                    if st == 200:
                        return self._json(200, {"ok": True})
                    return self._send(409 if st == 409 else 429 if st == 429 else 502, (json.dumps(res) if isinstance(res, dict) else "the Keep host refused").encode(), "application/json")
                m = re.fullmatch(r"/goals/([0-9a-f-]{36})/plan(?:/(accept|reject))?", path)
                if m and UUID.fullmatch(m.group(1)) and method == "POST":
                    gid, action = m.group(1), m.group(2)
                    data = self._optional_body()
                    if data is None:
                        return
                    goals = self._my_goals(owner)
                    if goals is None:
                        return self._send(502, b"the Keep host did not list goals")
                    if gid not in {g["id"] for g in goals}:
                        return self._send(404, b"not found")
                    body = {}
                    if action == "accept":
                        for k in ("confirm_tainted", "autorun"):
                            if data.get(k) is True:
                                body[k] = True
                    st, res = self._upstream("POST", f"/v1/goals/{gid}/plan" + (f"/{action}" if action else ""), body)
                    if st in (200, 202):
                        return self._json(200, {"ok": True})
                    return self._send(st if st in (400, 404, 409, 429) else 502, (json.dumps(res) if isinstance(res, dict) else "the Keep host refused").encode(), "application/json")
                m = re.fullmatch(r"/goals/([0-9a-f-]{36})", path)
                if m and UUID.fullmatch(m.group(1)) and method == "PATCH":
                    data = self._json_body()
                    if data is None:
                        return
                    allowed = isinstance(data, dict) and len(data) == 1 and (isinstance(data.get("autorun"), bool) or data.get("status") == "cancelled")
                    if not allowed:
                        return self._send(400, b"only autorun (true or false) or status cancelled can be changed here")
                    goals = self._my_goals(owner)
                    if goals is None:
                        return self._send(502, b"the Keep host did not list goals")
                    if m.group(1) not in {g["id"] for g in goals}:
                        return self._send(404, b"not found")
                    st, goal = self._upstream("PATCH", f"/v1/goals/{m.group(1)}", data)
                    if st == 200 and isinstance(goal, dict):
                        return self._json(200, project_goal(goal))
                    return self._send(409 if st == 409 else 429 if st == 429 else 502, (json.dumps(goal) if isinstance(goal, dict) else "the Keep host refused").encode(), "application/json")
                self._send(404, b"not found")
            except (OSError, http.client.HTTPException) as e:
                self._send(502, ("could not reach the Keep host: %s" % e).encode())

        def _host_ok(self):
            if not allowed_host(self.headers.get("Host", ""), port):
                self._send(421, b"misdirected request")
                return False
            return True

        def do_GET(self):
            if not self._host_ok():
                return
            path = self.path.split("?", 1)[0]
            if path == "/threads" or path.startswith("/threads/"):
                return self._thread_route("GET", path)
            if path in ("/memory", "/goals", "/suggestions", "/receipts"):
                return self._person_route("GET", path)
            item = STATIC.get(path)
            if not item:
                return self._send(404, b"not found")
            name, ctype = item
            with open(os.path.join(HERE, name), "rb") as f:
                body = f.read()
            if name == "index.html":
                body = body.replace(b"{{AGENT}}", agent.encode())
            self._send(200, body, ctype)

        def do_DELETE(self):
            if not self._host_ok() or not self._origin_ok():
                return
            path = self.path.split("?", 1)[0]
            if path.startswith("/threads/"):
                return self._thread_route("DELETE", path)
            if path.startswith("/memory/"):
                return self._person_route("DELETE", path)
            self._send(404, b"not found")

        def _change(self, method):
            """PUT and PATCH: only the person routes."""
            if not self._host_ok() or not self._origin_ok():
                return
            path = self.path.split("?", 1)[0]
            if path.startswith(("/memory", "/goals", "/suggestions")):
                return self._person_route(method, path)
            self._send(404, b"not found")

        def do_PUT(self):
            self._change("PUT")

        def do_PATCH(self):
            self._change("PATCH")

        def do_POST(self):
            if not self._host_ok():
                return
            path = self.path.split("?", 1)[0]
            if (
                path in ("/memory", "/goals")
                or (path.startswith("/memory/") and path.endswith(("/accept", "/reject")))
                or (path.startswith("/goals/") and "/plan" in path)
                or (path.startswith("/suggestions/") and path.endswith(("/accept", "/dismiss")))
            ):
                if not self._origin_ok():
                    return
                return self._person_route("POST", path)
            if self.path != "/agui":
                return self._send(404, b"not found")
            if not self._origin_ok():
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
            except ValueError:
                return self._send(400, b"bad content-length")
            if length <= 0 or length > MAX_BODY:
                return self._send(413, b"the request is empty or too large")
            try:
                payload = force_agent(self.rfile.read(length), agent)
            except (ValueError, json.JSONDecodeError) as e:
                return self._send(400, ("bad request: %s" % e).encode())
            conn_cls = http.client.HTTPSConnection if up.scheme == "https" else http.client.HTTPConnection
            conn = conn_cls(up.hostname, up.port or (443 if up.scheme == "https" else 80), timeout=180)
            try:
                headers = {"Content-Type": "application/json", "Accept": "text/event-stream"}
                if token:
                    headers["Authorization"] = "Bearer " + token
                conn.request("POST", "/v1/agui", body=payload, headers=headers)
                resp = conn.getresponse()
                if resp.status != 200:
                    detail = resp.read(4096)
                    return self._send(resp.status, detail, resp.getheader("Content-Type", "text/plain"))
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Security-Policy", CSP)
                self.send_header("X-Content-Type-Options", "nosniff")
                self.send_header("Connection", "close")
                self.end_headers()
                while True:
                    chunk = resp.read1(4096)
                    if not chunk:
                        break
                    self.wfile.write(chunk)
                    self.wfile.flush()
            except (OSError, http.client.HTTPException) as e:
                try:
                    self._send(502, ("could not reach the Keep host: %s" % e).encode())
                except OSError:
                    pass
            finally:
                conn.close()
            self.close_connection = True

    return Handler


def main():
    ap = argparse.ArgumentParser(description="a small web chat for a Keep agent over AG-UI")
    ap.add_argument("--agent", required=True, help="the deployed Keep agent to talk to")
    ap.add_argument("--port", type=int, default=8787)
    ap.add_argument("--user", default="operator", help="whose conversations to list: with the operator token AG-UI runs as 'operator' (the default); with a user token the host scopes to that user and this is ignored")
    args = ap.parse_args()
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,39}", args.agent):
        raise SystemExit("--agent must be a pack name: lowercase letters, digits and '-'")
    upstream = os.environ.get("MERION_API", os.environ.get("KEEP_API", "http://127.0.0.1:9096"))
    token = os.environ.get("MERION_TOKEN", os.environ.get("KEEP_TOKEN", ""))
    srv = ThreadingHTTPServer(("127.0.0.1", args.port), make_handler(upstream, token, args.agent, args.port, args.user))
    print(f"keep-chat: agent '{args.agent}' on {upstream}. Open http://127.0.0.1:{args.port}   (Ctrl-C to stop)", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
