#!/usr/bin/env python3
"""scripts/merion-chat.py: the chat proxy forwards the chat run, the conversation list, the person's memory and their goals with the token added server-side, fixes the
agent, streams, reaches a thread, memory entry or goal only if the host lists it for this agent and user, allows only a few fields through, and refuses foreign hosts
and origins. Runs against a fake Keep host; needs no runtime."""
import http.client
import importlib.util
import json
import os
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
spec = importlib.util.spec_from_file_location("keep_chat", os.path.join(ROOT, "scripts", "merion-chat.py"))
keep_chat = importlib.util.module_from_spec(spec)
spec.loader.exec_module(keep_chat)

SEEN = {}
CALLS = []  # (method, path) of every thread call the proxy made to the host
MINE = "11111111-1111-4111-8111-111111111111"
OTHER_AGENT = "22222222-2222-4222-8222-222222222222"
NO_CLIENT_ID = "33333333-3333-4333-8333-333333333333"
NEVER_LISTED = "44444444-4444-4444-8444-444444444444"
THREADS = {"items": [
    {"id": MINE, "agent": "echo-agent", "user_id": "operator", "client_thread_id": "c-1", "title": "hello", "updated_at": "2026-09-27T10:00:00Z", "message_count": 2, "session_id": "s"},
    {"id": OTHER_AGENT, "agent": "someone-else", "user_id": "operator", "client_thread_id": "c-2", "title": "no", "updated_at": "x", "message_count": 1},
    {"id": NO_CLIENT_ID, "agent": "echo-agent", "user_id": "operator", "client_thread_id": None, "title": "api-made", "updated_at": "x", "message_count": 0},
]}
LIST_STATUS = [200]
MEM_ITEM = "55555555-5555-4555-8555-555555555555"
MEM_PROPOSAL = "66666666-6666-4666-8666-666666666666"
GOAL_MINE = "77777777-7777-4777-8777-777777777777"
GOAL_OTHER_AGENT = "88888888-8888-4888-8888-888888888888"
GOAL_NOPLAN = "99999999-9999-4999-8999-999999999999"
SUG_MINE = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
SUG_TAINTED = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
SUGGESTIONS = {"enabled": True, "decided": [{"id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc", "title": "old"}],
               "pending": [{"id": SUG_MINE, "title": "Book the dentist", "reason": "a gap on Friday", "agent": "echo-agent", "proposed_by": "x", "session_id": "leak", "fingerprint": "leak", "tainted": False},
                           {"id": SUG_TAINTED, "title": "Wire money", "reason": "", "agent": "echo-agent", "tainted": True},
                           {"id": "not-a-uuid", "title": "bad"}]}
MEMORY = {"enabled": True,
          "items": [{"id": MEM_ITEM, "text": "vegetarian", "kind": "fact", "pinned": True, "tainted": False, "origin": "user", "source": {"session_id": "s"}, "created_at": "x"}],
          "proposals": [{"id": MEM_PROPOSAL, "text": "likes aisles", "kind": "note", "pinned": False, "tainted": True, "origin": "agent"}]}
GOALS = {"items": [
    {"id": GOAL_MINE, "agent": "echo-agent", "user_id": "ana", "title": "Trip", "status": "open", "autorun": True, "updated_at": "t", "max_attempts": 3, "session_id": "leak",
     "plan": [{"id": "s1", "title": "a", "status": "done", "detail": "done", "attempts": 1, "input": {"secret": 1}, "session_id": "s"}]},
    {"id": GOAL_OTHER_AGENT, "agent": "someone-else", "user_id": "ana", "title": "No", "status": "open", "autorun": False, "plan": []},
    {"id": GOAL_NOPLAN, "agent": "echo-agent", "user_id": "ana", "title": "Lisbon", "status": "open", "autorun": False, "plan": [], "planning_session_id": None, "updated_at": "t",
     "proposed_plan": {"session_id": "leak", "tainted": True, "proposed_at": "t", "steps": [{"title": "Find flights", "input": {"message": "secret"}, "requires_approval": False}, {"title": "Book", "requires_approval": True}]}},
    {"id": "not-a-uuid", "agent": "echo-agent", "title": "bad", "plan": []},
]}
RECEIPTS = {"items": [
    {"id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd", "at": "2026-09-27T10:00:00Z", "user_id": "ana", "session_id": "leak", "agent": "mail-compose", "credential": "gmail-send",
     "method": "POST", "url": "https://gmail.googleapis.com/gmail/v1/users/me/messages/send", "body_bytes": 812, "body_sha256": "leak-digest",
     "approval_id": "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee", "idempotency_key": "leak-key", "status": 200, "fingerprint": "leak-fp"},
    {"id": "ffffffff-ffff-4fff-8fff-ffffffffffff", "at": "2026-09-27T09:00:00Z", "agent": "x", "method": "POST", "url": "not a url", "status": 502, "approval_id": None},
    {"id": "not-a-uuid", "url": "https://x.example/"},
]}
BODIES = []   # (method, path, body) of every write the proxy made to the host
FIRST_SENT = threading.Event()
RELEASE = threading.Event()


class Upstream(BaseHTTPRequestHandler):
    who = {"role": "user", "user_id": "ana", "scopes": ["read", "run"]}

    def log_message(self, *a):
        pass

    def _json(self, code, value):
        raw = json.dumps(value).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        CALLS.append(("GET", self.path, self.headers.get("Authorization")))
        if self.path == "/v1/whoami":
            return self._json(200, self.who)
        if self.path.startswith("/v1/memory?"):
            return self._json(200, MEMORY)
        if self.path.startswith("/v1/goals?"):
            return self._json(LIST_STATUS[0], GOALS)
        if self.path.startswith("/v1/suggestions?"):
            return self._json(200, SUGGESTIONS)
        if self.path.startswith("/v1/receipts?"):
            return self._json(200, RECEIPTS)
        if self.path.startswith("/v1/threads?"):
            return self._json(LIST_STATUS[0], THREADS)
        if self.path == f"/v1/threads/{MINE}/messages":
            return self._json(200, {"items": [
                {"id": "msg-1", "role": "user", "text": "<b>hi</b>", "created_at": "2026-09-27T10:00:00Z", "session_id": "s", "event_seq": 3},
                {"id": "msg-2", "role": "assistant", "text": "hello", "created_at": "2026-09-27T10:00:05Z", "session_id": "s", "event_seq": 9},
            ]})
        self._json(404, {"error": "not found"})

    def do_DELETE(self):
        CALLS.append(("DELETE", self.path, self.headers.get("Authorization")))
        self.send_response(204)
        self.end_headers()

    def _write(self, method):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")) or 0)
        data = json.loads(body) if body else None
        BODIES.append((method, self.path, data))
        CALLS.append((method, self.path, self.headers.get("Authorization")))
        if self.path.startswith("/v1/memory/settings"):
            return self._json(200, {"enabled": data["enabled"]})
        if self.path.startswith("/v1/memory/") and self.path.split("?")[0].endswith(("/accept", "/reject")):
            return self._json(200, {"id": self.path.split("/")[3]})
        if self.path.startswith("/v1/memory"):
            if data["text"] == "token ghp_x":
                return self._json(400, {"error": "looks like a secret"})
            return self._json(201, {"id": MEM_ITEM, "text": data["text"], "kind": data["kind"], "pinned": data.get("pinned", False), "tainted": False, "origin": "user", "source": {}})
        if self.path.startswith("/v1/suggestions/settings"):
            return self._json(200, {"enabled": data["enabled"]})
        if self.path.startswith("/v1/suggestions/"):
            return self._json(200, {"suggestion": {"id": self.path.split("/")[3]}})
        if self.path.startswith("/v1/goals/") and "/plan" in self.path:
            if self.path.endswith("/plan"):
                return self._json(202 if GOALS.get("_plan_ok", True) else 400, {"goal_id": "x"} if GOALS.get("_plan_ok", True) else {"error": "no planner agent"})
            return self._json(200, {"id": self.path.split("/")[3]})
        if self.path == "/v1/goals":
            if data["title"] == "quota":
                return self._json(429, {"error": "at most 5"})
            return self._json(201, {"id": GOAL_MINE, "agent": data["agent"], "title": data["title"], "status": "open", "autorun": data["autorun"], "plan": [{"id": f"s{i+1}", "title": p["title"], "status": "pending", "attempts": 0} for i, p in enumerate(data["plan"])], "user_id": data.get("user_id", "ana")})
        if self.path.startswith("/v1/goals/"):
            if data.get("autorun") is True and GOALS["items"][0].get("_conflict"):
                return self._json(409, {"error": "cancelled"})
            return self._json(200, {**GOALS["items"][0], **data})
        self._json(404, {"error": "no"})

    def do_PUT(self):
        self._write("PUT")

    def do_PATCH(self):
        self._write("PATCH")

    def do_POST(self):
        if self.path.startswith("/v1/memory") or self.path.startswith("/v1/goals") or self.path.startswith("/v1/suggestions"):
            return self._write("POST")
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        data = json.loads(body)
        SEEN["auth"] = self.headers.get("Authorization")
        SEEN["path"] = self.path
        SEEN["body"] = data
        if data.get("threadId") == "ended":
            self.send_response(409)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"error":"this thread\'s session has ended; start a new thread"}')
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        self.wfile.write(b'data: {"type":"RUN_STARTED","threadId":"t","runId":"r"}\n\n')
        self.wfile.flush()
        FIRST_SENT.set()
        RELEASE.wait(5)  # the second event is held back until the test has seen the first one arrive
        self.wfile.write(b'data: {"type":"RUN_FINISHED","threadId":"t","runId":"r","result":"ok"}\n\n')
        self.wfile.flush()


def serve(handler, port=0):
    srv = ThreadingHTTPServer(("127.0.0.1", port), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


class ChatProxy(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.up = serve(Upstream)
        probe = ThreadingHTTPServer(("127.0.0.1", 0), BaseHTTPRequestHandler)
        cls.port = probe.server_address[1]
        probe.server_close()
        handler = keep_chat.make_handler(f"http://127.0.0.1:{cls.up.server_address[1]}", "SECRET-TOKEN", "echo-agent", cls.port)
        cls.proxy = serve(handler, cls.port)

    @classmethod
    def tearDownClass(cls):
        cls.proxy.shutdown()
        cls.up.shutdown()

    def req(self, method, path, body=None, headers=None, port=None):
        port = port or self.port
        c = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
        h = {"Host": f"127.0.0.1:{port}"}
        h.update(headers or {})
        if isinstance(body, (dict, list)):
            body = json.dumps(body).encode()
            h.setdefault("Content-Length", str(len(body)))
            h.setdefault("Content-Type", "application/json")
        c.request(method, path, body=body, headers=h)
        r = c.getresponse()
        return r, r.read(), c

    def sent(self, method, path_prefix):
        return [b for b in BODIES if b[0] == method and b[1].startswith(path_prefix)]

    def test_the_page_is_served_with_the_agent_and_a_strict_csp(self):
        r, body, _ = self.req("GET", "/")
        self.assertEqual(r.status, 200)
        self.assertIn(b"echo-agent", body)
        self.assertNotIn(b"{{AGENT}}", body)
        self.assertIn("default-src 'none'", r.getheader("Content-Security-Policy"))
        self.assertEqual(self.req("GET", "/app.js")[0].status, 200)
        self.assertEqual(self.req("GET", "/app.css")[0].status, 200)

    def test_only_the_page_and_agui_exist(self):
        for path in ["/v1/sessions", "/v1/agents", "/../etc/passwd", "/agui", "/app.js/../x"]:
            r, _, _ = self.req("GET", path)
            self.assertEqual(r.status, 404, path)
        r, _, _ = self.req("POST", "/v1/sessions", b"{}", {"Content-Length": "2"})
        self.assertEqual(r.status, 404)

    def test_the_token_is_added_on_the_server_and_the_agent_is_fixed(self):
        FIRST_SENT.clear(); RELEASE.set()
        payload = json.dumps({"threadId": "t1", "runId": "r1", "messages": [{"role": "user", "content": "hi"}], "forwardedProps": {"agent": "other-agent", "x": 1}}).encode()
        r, body, _ = self.req("POST", "/agui", payload, {"Content-Type": "application/json", "Content-Length": str(len(payload))})
        self.assertEqual(r.status, 200)
        self.assertEqual(SEEN["auth"], "Bearer SECRET-TOKEN")
        self.assertEqual(SEEN["path"], "/v1/agui")
        self.assertEqual(SEEN["body"]["forwardedProps"], {"agent": "echo-agent"}, "the page cannot choose another agent or pass other props")
        self.assertEqual(SEEN["body"]["threadId"], "t1")
        self.assertEqual(SEEN["body"]["messages"][0]["content"], "hi")
        self.assertNotIn(b"SECRET-TOKEN", body)
        self.assertNotIn("SECRET-TOKEN", json.dumps(dict(r.getheaders())))

    def test_events_are_streamed_as_they_arrive(self):
        FIRST_SENT.clear(); RELEASE.clear()
        payload = json.dumps({"threadId": "t2", "runId": "r2", "messages": [{"role": "user", "content": "hi"}]}).encode()
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        c.request("POST", "/agui", body=payload, headers={"Host": f"127.0.0.1:{self.port}", "Content-Length": str(len(payload))})
        r = c.getresponse()
        first = r.read1(4096)  # must arrive while the upstream is still holding back the second event
        self.assertIn(b"RUN_STARTED", first)
        self.assertFalse(RELEASE.is_set())
        RELEASE.set()
        rest = b""
        deadline = time.time() + 5
        while b"RUN_FINISHED" not in rest and time.time() < deadline:
            rest += r.read1(4096)
        self.assertIn(b"RUN_FINISHED", rest)
        c.close()

    def test_a_foreign_host_or_origin_is_refused(self):
        r, _, _ = self.req("GET", "/", headers={"Host": "evil.example"})
        self.assertEqual(r.status, 421, "DNS rebinding")
        payload = b'{"threadId":"t","runId":"r","messages":[]}'
        r, _, _ = self.req("POST", "/agui", payload, {"Origin": "http://evil.example", "Content-Length": str(len(payload))})
        self.assertEqual(r.status, 403, "a cross-site post")
        r, _, _ = self.req("POST", "/agui", payload, {"Host": "evil.example", "Content-Length": str(len(payload))})
        self.assertEqual(r.status, 421)

    def test_bad_requests_are_refused_before_the_host_is_called(self):
        r, _, _ = self.req("POST", "/agui", b"", {"Content-Length": "0"})
        self.assertEqual(r.status, 413)
        big = b"x" * (keep_chat.MAX_BODY + 1)
        r, _, _ = self.req("POST", "/agui", big, {"Content-Length": str(len(big))})
        self.assertEqual(r.status, 413)
        for bad in (b"not json", b"[1,2]"):
            r, _, _ = self.req("POST", "/agui", bad, {"Content-Length": str(len(bad))})
            self.assertEqual(r.status, 400, bad)

    def test_an_error_from_the_host_is_passed_through(self):
        payload = b'{"threadId":"ended","runId":"r","messages":[{"role":"user","content":"x"}]}'
        r, body, _ = self.req("POST", "/agui", payload, {"Content-Length": str(len(payload))})
        self.assertEqual(r.status, 409)
        self.assertIn(b"has ended", body)

    def test_the_page_lists_only_this_agents_continuable_conversations(self):
        CALLS.clear()
        r, body, _ = self.req("GET", "/threads")
        self.assertEqual(r.status, 200)
        items = json.loads(body)["items"]
        self.assertEqual([t["id"] for t in items], [MINE], "another agent's thread and one with no client id are not shown")
        self.assertEqual(set(items[0]), {"id", "client_thread_id", "title", "updated_at", "message_count"}, "nothing else of the host's record reaches the page")
        method, path, auth = CALLS[0]
        self.assertEqual(method, "GET")
        self.assertIn("agent=echo-agent", path)
        self.assertIn("user_id=operator", path)
        self.assertEqual(auth, "Bearer SECRET-TOKEN")
        self.assertNotIn(b"SECRET-TOKEN", body)

    def test_messages_are_read_only_for_a_listed_thread_and_carry_only_role_text_and_time(self):
        CALLS.clear()
        r, body, _ = self.req("GET", f"/threads/{MINE}/messages")
        self.assertEqual(r.status, 200)
        self.assertEqual(json.loads(body)["items"], [{"role": "user", "text": "<b>hi</b>", "at": "2026-09-27T10:00:00Z"}, {"role": "assistant", "text": "hello", "at": "2026-09-27T10:00:05Z"}])
        for other in (OTHER_AGENT, NO_CLIENT_ID, NEVER_LISTED):
            CALLS.clear()
            r, _, _ = self.req("GET", f"/threads/{other}/messages")
            self.assertEqual(r.status, 404, other)
            self.assertFalse([c for c in CALLS if other in c[1]], "an unlisted thread is never requested from the host")

    def test_forgetting_a_thread_needs_the_host_to_list_it_and_a_same_origin_request(self):
        CALLS.clear()
        r, _, _ = self.req("DELETE", f"/threads/{MINE}")
        self.assertEqual(r.status, 204)
        self.assertIn(("DELETE", f"/v1/threads/{MINE}", "Bearer SECRET-TOKEN"), CALLS)
        CALLS.clear()
        for other in (OTHER_AGENT, NEVER_LISTED):
            r, _, _ = self.req("DELETE", f"/threads/{other}")
            self.assertEqual(r.status, 404, other)
        self.assertFalse([c for c in CALLS if c[0] == "DELETE"], "nothing was deleted on the host")
        r, _, _ = self.req("DELETE", f"/threads/{MINE}", headers={"Origin": "http://evil.example"})
        self.assertEqual(r.status, 403, "a cross-site delete")
        r, _, _ = self.req("DELETE", f"/threads/{MINE}", headers={"Host": "evil.example"})
        self.assertEqual(r.status, 421)

    def test_thread_paths_cannot_reach_anything_else(self):
        for path in ["/threads/../v1/sessions", "/threads/not-a-uuid", f"/threads/{MINE}/other", f"/threads/{MINE}/messages/x", "/threads//messages", "/threadsx"]:
            r, _, _ = self.req("GET", path)
            self.assertEqual(r.status, 404, path)
        r, _, _ = self.req("DELETE", f"/threads/{MINE}/messages")
        self.assertEqual(r.status, 404, "messages cannot be deleted one by one, or the thread through that path")
        r, _, _ = self.req("DELETE", "/v1/sessions/x")
        self.assertEqual(r.status, 404)
        r, _, _ = self.req("POST", "/threads", b"{}", {"Content-Length": "2"})
        self.assertEqual(r.status, 404, "the page cannot create threads except by chatting")

    def test_a_host_that_cannot_list_threads_is_a_502_not_an_open_door(self):
        LIST_STATUS[0] = 500
        try:
            self.assertEqual(self.req("GET", "/threads")[0].status, 502)
            CALLS.clear()
            self.assertEqual(self.req("GET", f"/threads/{MINE}/messages")[0].status, 502)
            self.assertEqual(self.req("DELETE", f"/threads/{MINE}")[0].status, 502)
            self.assertFalse([c for c in CALLS if c[0] == "DELETE" or "messages" in c[1]])
        finally:
            LIST_STATUS[0] = 200

    def test_memory_is_read_as_the_person_the_host_names_and_only_allowed_fields_reach_the_page(self):
        CALLS.clear()
        r, body, _ = self.req("GET", "/memory")
        self.assertEqual(r.status, 200)
        v = json.loads(body)
        self.assertEqual((v["enabled"], [i["id"] for i in v["items"]], [i["id"] for i in v["proposals"]]), (True, [MEM_ITEM], [MEM_PROPOSAL]))
        self.assertEqual(set(v["items"][0]), {"id", "text", "kind", "pinned", "tainted", "origin"}, "no source, no timestamps")
        self.assertTrue(v["proposals"][0]["tainted"])
        mem = [c for c in CALLS if c[1].startswith("/v1/memory")]
        self.assertIn("user_id=ana", mem[0][1], "the owner is the user the host says the token is (learned once from /v1/whoami)")
        self.assertEqual(mem[0][2], "Bearer SECRET-TOKEN")
        self.assertNotIn(b"SECRET-TOKEN", body)

    def test_memory_changes_are_validated_and_only_named_fields_are_forwarded(self):
        BODIES.clear()
        r, _, _ = self.req("PUT", "/memory/settings", {"enabled": False})
        self.assertEqual(r.status, 204)
        self.assertEqual(self.sent("PUT", "/v1/memory/settings")[0][2], {"enabled": False})
        for bad in ({"enabled": "yes"}, {}, {"enabled": 1}):
            self.assertEqual(self.req("PUT", "/memory/settings", bad)[0].status, 400, bad)
        r, body, _ = self.req("POST", "/memory", {"text": "prefers window seats", "kind": "preference", "pinned": True, "user_id": "evil", "origin": "agent", "tainted": False})
        self.assertEqual(r.status, 201)
        self.assertEqual(self.sent("POST", "/v1/memory")[0][2], {"text": "prefers window seats", "kind": "preference", "pinned": True}, "nothing but text, kind and pinned goes on")
        for bad in ({"text": "  "}, {"text": "x", "kind": "mood"}, {"text": "y" * 2001}, {"kind": "note"}, [1]):
            self.assertEqual(self.req("POST", "/memory", bad)[0].status, 400, bad)
        r, body, _ = self.req("POST", "/memory", {"text": "token ghp_x"})
        self.assertEqual(r.status, 400, "the host's refusal of a credential is passed on")
        self.assertEqual(self.req("POST", "/memory", b"not json", {"Content-Length": "8"})[0].status, 400)

    def test_a_memory_entry_is_touched_only_if_the_host_lists_it_and_accepting_needs_a_proposal(self):
        BODIES.clear(); CALLS.clear()
        self.assertEqual(self.req("DELETE", f"/memory/{MEM_ITEM}")[0].status, 204)
        self.assertEqual(self.req("DELETE", f"/memory/{MEM_PROPOSAL}")[0].status, 204, "a proposal can be deleted too")
        self.assertEqual(self.req("POST", f"/memory/{MEM_PROPOSAL}/accept")[0].status, 204)
        self.assertEqual(self.req("POST", f"/memory/{MEM_PROPOSAL}/reject")[0].status, 204)
        CALLS.clear()
        for path, method in [(f"/memory/{NEVER_LISTED}", "DELETE"), (f"/memory/{NEVER_LISTED}/accept", "POST"), (f"/memory/{MEM_ITEM}/accept", "POST"), (f"/memory/{MEM_ITEM}/reject", "POST")]:
            self.assertEqual(self.req(method, path)[0].status, 404, path)
        self.assertFalse([c for c in CALLS if c[0] in ("DELETE", "POST")], "nothing unlisted (or already accepted) reached the host")
        for path, method in [("/memory/not-a-uuid", "DELETE"), (f"/memory/{MEM_ITEM}/other", "POST"), (f"/memory/{MEM_ITEM}", "GET"), (f"/memory/{MEM_ITEM}", "PATCH")]:
            self.assertEqual(self.req(method, path, {} if method in ("PATCH", "POST") else None)[0].status, 404, path)

    def test_goals_are_listed_for_this_agent_with_only_what_the_page_shows(self):
        CALLS.clear()
        r, body, _ = self.req("GET", "/goals")
        self.assertEqual(r.status, 200)
        items = json.loads(body)["items"]
        self.assertEqual([g["id"] for g in items], [GOAL_MINE, GOAL_NOPLAN], "another agent's goal and one with a bad id are not shown")
        self.assertEqual(set(items[0]), {"id", "title", "status", "autorun", "updated_at", "plan", "planning", "proposed"})
        self.assertEqual(set(items[0]["plan"][0]), {"id", "title", "status", "detail", "attempts"}, "no step input, no session id")
        g = [c for c in CALLS if c[1].startswith("/v1/goals?")][0][1]
        self.assertIn("agent=echo-agent", g)

    def test_a_goal_is_created_for_this_agent_from_a_title_and_steps(self):
        BODIES.clear()
        r, body, _ = self.req("POST", "/goals", {"title": " Weekend trip ", "steps": ["Find flights", "", "Book"], "autorun": True, "agent": "other", "user_id": "evil", "plan": [{"input": 1}], "session_id": "x", "allow_hosts": ["a"]})
        self.assertEqual(r.status, 201)
        sent = self.sent("POST", "/v1/goals")[0][2]
        self.assertEqual(sent, {"title": "Weekend trip", "agent": "echo-agent", "autorun": True, "plan": [{"title": "Find flights"}, {"title": "Book"}]}, "agent fixed, user token sends no user_id, nothing else forwarded")
        self.assertEqual(json.loads(body)["plan"][1]["title"], "Book")
        for bad in ({"title": "", "steps": ["a"]}, {"title": "t", "steps": [], "autorun": True}, {"title": "t", "steps": ["a"] * 21}, {"title": "t", "steps": ["a" * 201]}, {"title": "t" * 121, "steps": ["a"]}, {"title": "t", "steps": "a"}, [1]):
            self.assertEqual(self.req("POST", "/goals", bad)[0].status, 400, bad)
        self.assertEqual(self.req("POST", "/goals", {"title": "quota", "steps": ["a"], "autorun": True})[0].status, 429, "the host's limit is passed on")

    def test_a_goal_can_only_be_paused_resumed_or_cancelled_and_only_if_listed(self):
        BODIES.clear(); CALLS.clear()
        self.assertEqual(self.req("PATCH", f"/goals/{GOAL_MINE}", {"autorun": False})[0].status, 200)
        self.assertEqual(self.req("PATCH", f"/goals/{GOAL_MINE}", {"status": "cancelled"})[0].status, 200)
        self.assertEqual([b[2] for b in self.sent("PATCH", "/v1/goals/")], [{"autorun": False}, {"status": "cancelled"}])
        BODIES.clear()
        for bad in ({"status": "done"}, {"plan": []}, {"autorun": True, "plan": []}, {"autorun": "yes"}, {}, {"session_id": "x"}, {"allow_hosts": ["x"]}):
            self.assertEqual(self.req("PATCH", f"/goals/{GOAL_MINE}", bad)[0].status, 400, bad)
        for other in (GOAL_OTHER_AGENT, NEVER_LISTED):
            self.assertEqual(self.req("PATCH", f"/goals/{other}", {"autorun": False})[0].status, 404, other)
        self.assertFalse(BODIES, "nothing else reached the host")
        self.assertEqual(self.req("DELETE", f"/goals/{GOAL_MINE}")[0].status, 404, "the page cannot delete goals")
        self.assertEqual(self.req("GET", f"/goals/{GOAL_MINE}")[0].status, 404)

    def test_a_proposed_plan_reaches_the_page_as_titles_only(self):
        r, body, _ = self.req("GET", "/goals")
        g = [g for g in json.loads(body)["items"] if g["id"] == GOAL_NOPLAN][0]
        self.assertEqual(g["proposed"], {"tainted": True, "steps": [{"title": "Find flights", "requires_approval": False}, {"title": "Book", "requires_approval": True}]}, "no step input, no session id")
        self.assertEqual(g["planning"], False)

    def test_planning_is_asked_for_and_accepted_only_for_a_listed_goal_with_a_filtered_body(self):
        BODIES.clear(); CALLS.clear()
        self.assertEqual(self.req("POST", f"/goals/{GOAL_NOPLAN}/plan", None, {"Content-Length": "0"})[0].status, 200)
        self.assertEqual(self.sent("POST", f"/v1/goals/{GOAL_NOPLAN}/plan")[-1][2], {}, "no planner can be named from the page")
        r, _, _ = self.req("POST", f"/goals/{GOAL_NOPLAN}/plan/accept", {"confirm_tainted": True, "autorun": True, "agent": "evil", "plan": [1], "planner": "evil"})
        self.assertEqual(r.status, 200)
        self.assertEqual(self.sent("POST", f"/v1/goals/{GOAL_NOPLAN}/plan/accept")[-1][2], {"confirm_tainted": True, "autorun": True}, "only the two switches are forwarded")
        self.req("POST", f"/goals/{GOAL_NOPLAN}/plan/accept", {"confirm_tainted": "yes", "autorun": 1})
        self.assertEqual(self.sent("POST", f"/v1/goals/{GOAL_NOPLAN}/plan/accept")[-1][2], {}, "only a real true counts")
        self.assertEqual(self.req("POST", f"/goals/{GOAL_NOPLAN}/plan/reject", None, {"Content-Length": "0"})[0].status, 200)
        n = len(BODIES)
        for other in (GOAL_OTHER_AGENT, "44444444-4444-4444-8444-444444444444", "not-a-uuid"):
            for tail in ("/plan", "/plan/accept", "/plan/reject"):
                self.assertIn(self.req("POST", f"/goals/{other}{tail}", None, {"Content-Length": "0"})[0].status, (404,), (other, tail))
        self.assertEqual(len(BODIES), n, "nothing reached the host for a goal it does not list for this agent")
        self.assertEqual(self.req("POST", f"/goals/{GOAL_NOPLAN}/plan/accept", [1])[0].status, 400)
        self.assertEqual(self.req("POST", f"/goals/{GOAL_NOPLAN}/plan/delete", None, {"Content-Length": "0"})[0].status, 404)

    def test_the_host_saying_there_is_no_planner_reaches_the_page(self):
        GOALS["_plan_ok"] = False
        try:
            r, body, _ = self.req("POST", f"/goals/{GOAL_NOPLAN}/plan", None, {"Content-Length": "0"})
            self.assertEqual(r.status, 400)
            self.assertIn(b"no planner agent", body)
        finally:
            GOALS.pop("_plan_ok", None)

    def test_suggestions_are_read_with_only_what_the_page_shows_and_decided_only_if_listed(self):
        r, body, _ = self.req("GET", "/suggestions")
        v = json.loads(body)
        self.assertEqual(r.status, 200)
        self.assertEqual(v["enabled"], True)
        self.assertEqual([s["id"] for s in v["pending"]], [SUG_MINE, SUG_TAINTED], "a bad id is not shown")
        self.assertEqual(set(v["pending"][0]), {"id", "title", "reason", "agent", "tainted"}, "no session id, no digest")
        self.assertNotIn("decided", v)
        BODIES.clear()
        self.assertEqual(self.req("PUT", "/suggestions/settings", {"enabled": False})[0].status, 204)
        self.assertEqual(self.sent("PUT", "/v1/suggestions/settings")[-1][2], {"enabled": False})
        for bad in ({"enabled": "yes"}, {}, [1]):
            self.assertEqual(self.req("PUT", "/suggestions/settings", bad)[0].status, 400, bad)
        self.assertEqual(self.req("POST", f"/suggestions/{SUG_MINE}/accept", None, {"Content-Length": "0"})[0].status, 200)
        self.assertEqual(self.sent("POST", f"/v1/suggestions/{SUG_MINE}/accept")[-1][2], {}, "an untainted accept forwards no options")
        self.assertEqual(self.req("POST", f"/suggestions/{SUG_TAINTED}/accept", {"confirm_tainted": True, "x": 1})[0].status, 200)
        self.assertEqual(self.sent("POST", f"/v1/suggestions/{SUG_TAINTED}/accept")[-1][2], {"confirm_tainted": True})
        self.assertEqual(self.req("POST", f"/suggestions/{SUG_MINE}/dismiss", {"confirm_tainted": True})[0].status, 200)
        self.assertEqual(self.sent("POST", f"/v1/suggestions/{SUG_MINE}/dismiss")[-1][2], {}, "dismiss forwards nothing")
        n = len(BODIES)
        for other in ("44444444-4444-4444-8444-444444444444", "cccccccc-cccc-4ccc-8ccc-cccccccccccc", "not-a-uuid"):
            for action in ("accept", "dismiss"):
                self.assertEqual(self.req("POST", f"/suggestions/{other}/{action}", None, {"Content-Length": "0"})[0].status, 404, (other, action))
        self.assertEqual(len(BODIES), n, "nothing reached the host for a suggestion it does not list as pending")
        self.assertEqual(self.req("DELETE", f"/suggestions/{SUG_MINE}")[0].status, 404)

    def test_suggestion_and_plan_writes_need_the_same_origin_and_host(self):
        for method, path, body in [("PUT", "/suggestions/settings", {"enabled": True}), ("POST", f"/suggestions/{SUG_MINE}/accept", {}),
                                   ("POST", f"/suggestions/{SUG_MINE}/dismiss", {}), ("POST", f"/goals/{GOAL_NOPLAN}/plan", {}), ("POST", f"/goals/{GOAL_NOPLAN}/plan/accept", {})]:
            self.assertEqual(self.req(method, path, body, {"Origin": "http://evil.example"})[0].status, 403, (method, path))
            self.assertEqual(self.req(method, path, body, {"Host": "evil.example"})[0].status, 421, (method, path))

    def test_receipts_are_read_for_the_person_with_only_what_the_page_shows(self):
        CALLS.clear()
        r, body, _ = self.req("GET", "/receipts")
        self.assertEqual(r.status, 200)
        items = json.loads(body)["items"]
        self.assertEqual([i["id"] for i in items], ["dddddddd-dddd-4ddd-8ddd-dddddddddddd", "ffffffff-ffff-4fff-8fff-ffffffffffff"], "a bad id is not shown")
        self.assertEqual(set(items[0]), {"id", "at", "agent", "credential", "method", "host", "path", "status", "approved"}, "no body digest, no key, no session, no query")
        self.assertEqual((items[0]["host"], items[0]["path"], items[0]["approved"], items[0]["status"]), ("gmail.googleapis.com", "/gmail/v1/users/me/messages/send", True, 200))
        self.assertEqual((items[1]["host"], items[1]["approved"]), ("", False), "a malformed url is shown as nothing, not as an error")
        for leak in (b"leak-digest", b"leak-key", b"leak-fp", b"leak"):
            self.assertNotIn(leak, body)
        c = [c for c in CALLS if c[1].startswith("/v1/receipts?")][0][1]
        self.assertIn("limit=50", c)
        for method in ("POST", "PUT", "PATCH", "DELETE"):
            self.assertEqual(self.req(method, "/receipts", {})[0].status, 404, method)

    def test_every_write_needs_the_same_origin_and_host(self):
        for method, path, body in [("PUT", "/memory/settings", {"enabled": True}), ("POST", "/memory", {"text": "x"}), ("POST", f"/memory/{MEM_PROPOSAL}/accept", {}),
                                   ("DELETE", f"/memory/{MEM_ITEM}", None), ("POST", "/goals", {"title": "t", "steps": ["a"]}), ("PATCH", f"/goals/{GOAL_MINE}", {"autorun": False})]:
            r, _, _ = self.req(method, path, body, {"Origin": "http://evil.example"})
            self.assertEqual(r.status, 403, f"{method} {path} cross-site")
            r, _, _ = self.req(method, path, body, {"Host": "evil.example"})
            self.assertEqual(r.status, 421, f"{method} {path} rebinding")

    def test_with_the_operator_token_the_page_is_scoped_to_the_named_user_and_creates_goals_for_them(self):
        class OpUpstream(Upstream):
            who = {"role": "operator"}
        op_up = serve(OpUpstream)
        probe = ThreadingHTTPServer(("127.0.0.1", 0), BaseHTTPRequestHandler)
        port = probe.server_address[1]
        probe.server_close()
        proxy = serve(keep_chat.make_handler(f"http://127.0.0.1:{op_up.server_address[1]}", "OP-TOKEN", "echo-agent", port, "dana"), port)
        try:
            CALLS.clear(); BODIES.clear()
            self.assertEqual(self.req("GET", "/memory", port=port)[0].status, 200)
            self.assertIn("user_id=dana", [c for c in CALLS if c[1].startswith("/v1/memory")][0][1], "the operator token is scoped to --user")
            self.assertEqual(self.req("POST", "/goals", {"title": "t", "steps": ["a"]}, port=port)[0].status, 201)
            self.assertEqual(self.sent("POST", "/v1/goals")[0][2]["user_id"], "dana", "the goal is created for that user")
            CALLS.clear()
            self.assertEqual(self.req("GET", "/goals", port=port)[0].status, 200)
            listed = [c for c in CALLS if c[1].startswith("/v1/goals?")]
            self.assertTrue(listed and "user_id=dana" in listed[0][1], "the operator token lists only that user's goals")
            self.assertEqual(listed[0][2], "Bearer OP-TOKEN")
        finally:
            proxy.shutdown(); op_up.shutdown()

    def test_a_host_that_cannot_say_who_the_token_is_is_a_502(self):
        class Anonymous(Upstream):
            who = {"role": "nobody"}
        up = serve(Anonymous)
        probe = ThreadingHTTPServer(("127.0.0.1", 0), BaseHTTPRequestHandler)
        port = probe.server_address[1]
        probe.server_close()
        proxy = serve(keep_chat.make_handler(f"http://127.0.0.1:{up.server_address[1]}", "T", "echo-agent", port), port)
        try:
            self.assertEqual(self.req("GET", "/memory", port=port)[0].status, 502)
            self.assertEqual(self.req("GET", "/goals", port=port)[0].status, 502)
        finally:
            proxy.shutdown(); up.shutdown()

    def test_the_goal_and_memory_projections(self):
        self.assertEqual(keep_chat.project_memory(None), {"enabled": False, "items": [], "proposals": []})
        self.assertEqual(keep_chat.project_goals({"items": [{"id": "x"}]}, "a"), [])
        with self.assertRaises(ValueError):
            keep_chat.clean_goal_request({"title": "t", "steps": [], "autorun": True})
        self.assertEqual(keep_chat.clean_goal_request({"title": "t", "steps": []}), {"title": "t", "steps": [], "autorun": False}, "no steps: an agent can plan it")
        self.assertEqual(keep_chat.clean_goal_request({"title": "t", "steps": ["a", " "]}), {"title": "t", "steps": ["a"], "autorun": False})

    def test_force_agent_and_host_checks(self):
        self.assertEqual(json.loads(keep_chat.force_agent(b'{"a":1,"forwardedProps":{"agent":"z"}}', "y")), {"a": 1, "forwardedProps": {"agent": "y"}})
        with self.assertRaises(ValueError):
            keep_chat.force_agent(b'[1]', "y")
        self.assertTrue(keep_chat.allowed_host("127.0.0.1:8787", 8787))
        self.assertTrue(keep_chat.allowed_host("localhost:8787", 8787))
        self.assertFalse(keep_chat.allowed_host("127.0.0.1:9999", 8787))
        self.assertFalse(keep_chat.allowed_host("evil.example", 8787))


if __name__ == "__main__":
    unittest.main(verbosity=2)
