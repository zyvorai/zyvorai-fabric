#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""FluxVM stand-in for agent-runtime CI.

Stores guest files and runs the Node worker or harness on this machine.
It is not a microVM. Provider APIs are never contacted.
"""

from __future__ import annotations

import base64
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import threading
import urllib.error
import urllib.request
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = os.environ.get("SANDBOX_STUB_ROOT") or tempfile.mkdtemp(prefix="zyvor-sandbox-stub-")
os.makedirs(ROOT, exist_ok=True)
LOCK = threading.Lock()
SANDBOXES: dict[str, dict] = {}
CHANGESETS: dict[str, dict] = {}
CHANGESET_FIXTURE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures", "fluxvm-changeset.json")


def record(kind: str, row: dict) -> None:
    """Append one JSON line to <ROOT>/<kind>.jsonl, so an e2e can check what the runtime asked FluxVM to do."""
    with open(os.path.join(ROOT, f"{kind}.jsonl"), "a") as handle:
        handle.write(json.dumps(row) + "\n")
ENV_QUOTED = re.compile(r"\b([A-Z][A-Z0-9_]*)='([^']*)'")
ENV_PLAIN = re.compile(r"\b([A-Z][A-Z0-9_]*)=([^\s']+)")


def log(message: str) -> None:
    print(f"sandbox-stub: {message}", file=sys.stderr, flush=True)


def kill_proc(proc: subprocess.Popen | None) -> None:
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=3)


def sandbox_file(sandbox_id: str, guest_path: str) -> str:
    relative = guest_path.lstrip("/")
    path = os.path.join(ROOT, sandbox_id, relative)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    return path


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args) -> None:
        log(fmt % args)

    def _read_json(self) -> dict:
        length = int(self.headers.get("Content-Length", "0") or "0")
        raw = self.rfile.read(length) if length else b""
        if not raw:
            return {}
        return json.loads(raw.decode())

    def _send(self, status: int, payload) -> None:
        if isinstance(payload, (dict, list)):
            body = json.dumps(payload).encode()
            content_type = "application/json"
        elif isinstance(payload, bytes):
            body = payload
            content_type = "application/json"
        else:
            body = str(payload).encode()
            content_type = "text/plain"
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:  # noqa: N802
        self._route("POST")

    def do_GET(self) -> None:  # noqa: N802
        self._route("GET")

    def do_DELETE(self) -> None:  # noqa: N802
        self._route("DELETE")

    def _route(self, method: str) -> None:
        path = self.path.split("?", 1)[0]
        if method == "POST" and path == "/v1/sandboxes":
            body = self._read_json()
            sandbox_id = str(uuid.uuid4())
            with LOCK:
                SANDBOXES[sandbox_id] = {
                    "port": body.get("http_proxy_port"),
                    "proc": None,
                    "name": body.get("name", ""),
                }
            record("created", {"vm": sandbox_id, "name": body.get("name", "")})
            self._send(
                200,
                # `simulated` makes the runtime label every run here as not sealed (see agent-runtime/src/demos.rs `run_badge`).
                {"id": sandbox_id, "guest_ip": "127.0.0.1", "status": "running", "simulated": True},
            )
            return

        # Speculative execution. The changeset body is a response captured from a real FluxVM
        # (tests/fixtures/fluxvm-changeset.json); only ids and the state machine are the stub's.
        # Like FluxVM: paths are required for a VM sandbox, and apply before approve is a 409.
        spec = re.fullmatch(r"/v1/sandboxes/([^/]+)/speculate", path)
        if method == "POST" and spec:
            body = self._read_json()
            if not body.get("paths"):
                self._send(400, {"error": "paths is required for a VM sandbox"})
                return
            changeset = json.load(open(CHANGESET_FIXTURE))
            changeset.update(
                id=str(uuid.uuid4()),
                sandbox_id=spec.group(1),
                command=body.get("command", ""),
                paths=body["paths"],
                state="pending",
            )
            with LOCK:
                CHANGESETS[changeset["id"]] = changeset
            record("changeset", {"op": "speculate", "id": changeset["id"]})
            self._send(200, changeset)
            return
        verb = re.fullmatch(r"/v1/sandboxes/([^/]+)/changesets/([^/]+)/(approve|reject|apply)", path)
        if method == "POST" and verb:
            with LOCK:
                changeset = CHANGESETS.get(verb.group(2))
                if changeset is None:
                    self._send(404, {"error": "changeset not found"})
                    return
                op = verb.group(3)
                allowed = {"approve": "pending", "reject": "pending", "apply": "approved"}
                if changeset["state"] != allowed[op]:
                    self._send(409, {"error": f"cannot {op} a {changeset['state']} changeset"})
                    return
                changeset["state"] = {"approve": "approved", "reject": "rejected", "apply": "applied"}[op]
            record("changeset", {"op": op, "id": changeset["id"]})
            self._send(200, changeset)
            return

        write = re.fullmatch(r"/v1/sandboxes/([^/]+)/fs/write", path)
        if method == "POST" and write:
            body = self._read_json()
            dest = sandbox_file(write.group(1), body["path"])
            with open(dest, "wb") as handle:
                handle.write(base64.b64decode(body.get("content_base64", "")))
            os.chmod(dest, 0o755)
            self._send(200, {"ok": True})
            return

        process = re.fullmatch(r"/v1/sandboxes/([^/]+)/process", path)
        if method == "POST" and process:
            body = self._read_json()
            command = body.get("command", "")
            self._send(200, self._process(process.group(1), command))
            return

        # self.path keeps the query string so events?after=N reaches the guest.
        proxy = re.match(r"^/v1/sandboxes/([^/]+)/http/(\d+)/(.*)$", self.path)
        if proxy and method in {"GET", "POST"}:
            self._proxy(method, int(proxy.group(2)), proxy.group(3))
            return

        vm = re.fullmatch(r"/v1/vms/([^/]+)", path)
        if vm and method == "DELETE":
            record("deleted", {"vm": vm.group(1)})
            self._delete(vm.group(1))
            self._send(200, {"ok": True})
            return
        if method == "POST" and (
            path.endswith("/pause") or path.endswith("/resume") or path.endswith("/snapshot")
        ):
            if method == "POST" and path.endswith("/snapshot"):
                self._read_json()
            self._send(200, {"ok": True})
            return
        if method == "GET" and vm:
            self._send(200, {"id": vm.group(1), "status": "running"})
            return
        # The runtime pings the in-guest agent before it starts the worker.
        if method == "POST" and re.fullmatch(r"/v1/vms/([^/]+)/agent/ping", path):
            self._read_json()
            self._send(200, {"ok": True})
            return
        # Keep demo support: readiness, network policy, freeze and drop reasons.
        if method == "GET" and path == "/v1/security/capabilities":
            self._send(200, {"snp_present": False, "tdx_present": False})
            return
        policy = re.fullmatch(r"/v1/vms/([^/]+)/network/policy", path)
        if method in {"POST", "PUT"} and policy:
            body = self._read_json()
            with LOCK:
                name = SANDBOXES.get(policy.group(1), {}).get("name", "")
            # Test hook: a sandbox whose name contains "noconfine" cannot be given a policy.
            if "noconfine" in name:
                self._send(500, {"error": "policy refused (test hook)"})
                return
            with LOCK:
                SANDBOXES.setdefault(policy.group(1), {})["policy"] = body
            record("policies", {"vm": policy.group(1), "name": name, "policy": body})
            self._send(200, {"ok": True})
            return
        freeze = re.fullmatch(r"/v1/vms/([^/]+)/(freeze|thaw)", path)
        if method == "POST" and freeze:
            with LOCK:
                SANDBOXES.setdefault(freeze.group(1), {})["frozen"] = freeze.group(2) == "freeze"
            self._send(200, {"ok": True})
            return
        if method == "GET" and re.fullmatch(r"/v1/vms/([^/]+)/network/drop-reasons", path):
            self._send(200, {"drops": []})
            return
        self._send(404, {"error": "not found"})

    def _demo_command(self, sandbox_id: str, command: str) -> dict | None:
        """Run the fixed extract commands the Keep demos send, against this
        sandbox's own directory. Anything else is refused, so the stub can never
        become a general shell."""
        guest = "/home/agent/work"
        work = os.path.join(ROOT, sandbox_id, guest.lstrip("/"))
        if command.strip() == f"mkdir -p {guest}":
            os.makedirs(work, exist_ok=True)
            return {"stdout": ""}
        allowed = (
            re.fullmatch(rf"pdftotext -layout {re.escape(guest)}/input\.pdf - 2>/dev/null \| head -c \d+", command)
            or re.fullmatch(rf"tesseract {re.escape(guest)}/input\.img stdout -l eng --psm 4 2>/dev/null \| head -c \d+", command)
            or re.fullmatch(r"command -v (pdftotext|tesseract)", command)
            or re.fullmatch(rf"head -c \d+ {re.escape(guest)}/input\.(txt|log|json|csv)", command)
            or re.fullmatch(rf"node {re.escape(guest)}/extract\.mjs {re.escape(guest)}/input\.(html|eml|docx|xlsx|pptx)", command)
        )
        if not allowed:
            return None
        local = command.replace(guest, work)
        result = subprocess.run(["bash", "-c", local], capture_output=True, timeout=30, check=False)
        return {"stdout": result.stdout.decode("utf-8", "replace"), "exit_code": result.returncode}

    def _process(self, sandbox_id: str, command: str) -> dict:
        if "ip route" in command:
            return {"stdout": "127.0.0.1\n"}
        demo = self._demo_command(sandbox_id, command)
        if demo is not None:
            return demo
        if "worker.mjs" not in command:
            return {"stdout": ""}
        env = os.environ.copy()
        for key, value in ENV_QUOTED.findall(command):
            env[key] = value
        for key, value in ENV_PLAIN.findall(command):
            env.setdefault(key, value)
        bundle = sandbox_file(sandbox_id, "/opt/zyvor/agent/bundle.mjs")
        worker = sandbox_file(sandbox_id, "/opt/zyvor/worker.mjs")
        env["ZYVOR_AGENT_BUNDLE"] = bundle
        port = int(env.get("ZYVOR_AGENT_PORT", "8080"))
        log_handle = open(os.path.join(ROOT, f"{sandbox_id}.log"), "ab")
        with LOCK:
            for other in SANDBOXES.values():
                if other.get("port") == port:
                    kill_proc(other.get("proc"))
                    other["proc"] = None
            proc = subprocess.Popen(
                ["node", worker],
                env=env,
                stdout=log_handle,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            record = SANDBOXES.setdefault(sandbox_id, {})
            record["port"] = port
            record["proc"] = proc
            record["log"] = log_handle
        log(f"started node pid={proc.pid} port={port} runtime={env.get('ZYVOR_AGENT_RUNTIME', 'node')}")
        return {"stdout": ""}

    def _delete(self, sandbox_id: str) -> None:
        with LOCK:
            record = SANDBOXES.get(sandbox_id)
            if record:
                kill_proc(record.get("proc"))
                record["proc"] = None

    def _proxy(self, method: str, port: int, rest: str) -> None:
        url = f"http://127.0.0.1:{port}/{rest}"
        length = int(self.headers.get("Content-Length", "0") or "0")
        data = self.rfile.read(length) if length else None
        request = urllib.request.Request(url, data=data, method=method)
        if data is not None:
            request.add_header("Content-Type", self.headers.get("Content-Type", "application/json"))
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                body = response.read()
                self._send(response.status, body)
        except urllib.error.HTTPError as error:
            self._send(error.code, error.read())
        except (urllib.error.URLError, TimeoutError, ConnectionError) as error:
            self._send(503, {"error": str(error)})


def main() -> None:
    host = os.environ.get("SANDBOX_STUB_HOST", "127.0.0.1")
    port = int(os.environ.get("SANDBOX_STUB_PORT", "17788"))

    def shutdown(_signum, _frame) -> None:
        with LOCK:
            for record in SANDBOXES.values():
                kill_proc(record.get("proc"))
        raise SystemExit(0)

    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)
    server = ThreadingHTTPServer((host, port), Handler)
    log(f"listening on http://{host}:{port} root={ROOT}")
    server.serve_forever()


if __name__ == "__main__":
    main()
