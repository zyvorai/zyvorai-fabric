#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""
Minimal FluxVM HTTP stub for Fabric CI / local e2e (no KVM).

Usage:
  python3 tests/fixtures/fluxvm-stub.py
  FLUXVM_STUB_PORT=7788 python3 tests/fixtures/fluxvm-stub.py
"""

from __future__ import annotations

import json
import os
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlparse

PORT = int(os.environ.get("FLUXVM_STUB_PORT", "7788"))
RECV_ID = "00000000-0000-0000-0000-000000000001"
VM_ID = "11111111-1111-1111-1111-111111111111"

VMS: dict[str, dict] = {}


def vm_record(name: str, status: str = "running") -> dict:
    vid = VMS.get(name, {}).get("id", str(uuid.uuid4()))
    return {
        "id": vid,
        "name": name,
        "backend": "qemu",
        "status": status,
        "pid": 4242,
        "created_at": "2026-01-01T00:00:00Z",
        "expires_at": None,
        "workspace": "/tmp/fluxvm-stub",
        "disk": "/tmp/fluxvm-stub/disk.qcow2",
        "seed_disk": None,
        "tap_name": "tap0",
        "control_socket": "/tmp/fluxvm-stub/qmp.sock",
        "log_path": "/tmp/fluxvm-stub/vm.log",
        "error": None,
        "request": {
            "name": name,
            "backend": "qemu",
            "image": "/tmp/base.qcow2",
            "vcpus": 1,
            "memory_mib": 512,
        },
        "qga_socket": None,
        "virtiofsd_pids": [],
        "dhcp_leasefile": None,
    }


DP_STATUS = {
    "mode": "ebpf",
    "required": True,
    "attached": True,
    "interface": "tap0",
    "identity": 42,
    "pin_dir": "/sys/fs/bpf/fluxvm/vms/stub",
    "schema_version": 9,
    "schema_compatible": True,
    "policy_synced": True,
    "pod_ingress_required": False,
    "pod_ingress_attached": False,
    "policy": {
        "default_allow": False,
        "allow_cidrs": [],
        "allow_ports": [],
        "max_egress_mbps": None,
        "max_egress_pps": None,
        "sample_rate": 0,
        "deny_cidrs": [],
        "allow_icmp": True,
        "groups": [],
        "labels": [],
        "allow_fqdns": [],
        "entities": [],
        "audit_mode": False,
    },
}

DP_STATS = {
    "allowed_packets": 10,
    "allowed_bytes": 1000,
    "dropped_packets": 1,
    "dropped_bytes": 64,
    "pod_policy": {
        "egress": {"allowed": 5, "dropped": 1, "audited": 0},
        "ingress": {"allowed": 5, "dropped": 1, "audited": 0},
        "directional": False,
    },
}

DROP_REASONS = {
    "items": [
        {
            "identity": 42,
            "family": 4,
            "source": "10.0.0.1",
            "destination": "10.0.0.2",
            "source_port": 12345,
            "destination_port": 80,
            "protocol": 6,
            "reason_code": 8,
            "reason": "default-deny",
            "action": "drop",
            "packets": 3,
            "bytes": 192,
            "last_seen_ns": 1,
        }
    ]
}


class Handler(BaseHTTPRequestHandler):
    def _json(self, code: int, body) -> None:
        if isinstance(body, (bytes, bytearray)):
            raw = body
        else:
            raw = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def _read_json(self):
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length else b""
        if not raw:
            return {}
        try:
            return json.loads(raw.decode())
        except Exception:
            return {}

    def log_message(self, *_args) -> None:
        return

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path
        qs = parse_qs(parsed.query)

        if path in ("/readyz", "/healthz"):
            self._json(
                200,
                {
                    "ok": True,
                    "kvm": True,
                    "secure_containers": {
                        "available": False,
                        "shim_installed": False,
                        "guest_image_present": False,
                    },
                    "dataplane": {
                        "mode": "ebpf",
                        "required": True,
                        "default_allow": False,
                        "bpf_object_present": True,
                        "pin_root_present": True,
                        "bpffs_present": True,
                        "ok": True,
                    },
                },
            )
            return

        if path == "/v1/runtime/capabilities":
            self._json(
                200,
                {
                    "apiVersion": "runtime.fluxvm.zyvor.io/v1",
                    "scope": "node-local",
                    "orchestrationOwner": "zyvor-fabric",
                    "migration": [
                        {
                            "backend": "qemu",
                            "live": True,
                            "preCopy": True,
                            "postCopy": True,
                            "multifd": True,
                            "requiresSharedStorage": True,
                            "transports": ["tcp", "unix"],
                        }
                    ],
                    "snapshot": [],
                },
            )
            return

        if path == "/v1/vms":
            name = (qs.get("name") or [None])[0]
            if name:
                if name in VMS:
                    self._json(200, {"items": [vm_record(name, VMS[name]["status"])]})
                else:
                    self._json(200, {"items": []})
                return
            items = [vm_record(n, meta["status"]) for n, meta in VMS.items()]
            self._json(200, {"items": items})
            return

        if path.startswith("/v1/vms/"):
            parts = path.strip("/").split("/")
            # /v1/vms/{id}/...
            if len(parts) >= 3:
                vid = parts[2]
                name = next((n for n, m in VMS.items() if m["id"] == vid), None)
                if name is None and vid == VM_ID:
                    name = "stub-vm"
                    VMS.setdefault(name, {"id": VM_ID, "status": "running"})
                if "/network/status" in path:
                    self._json(200, DP_STATUS)
                    return
                if "/network/stats" in path:
                    self._json(200, DP_STATS)
                    return
                if "/network/flows" in path:
                    self._json(200, {"items": []})
                    return
                if "/network/drop-reasons" in path:
                    self._json(200, DROP_REASONS)
                    return
                if "/network/pod-policy" in path:
                    self._json(200, None)
                    return
                if "/network/effective" in path:
                    self._json(200, DP_STATUS["policy"])
                    return
                if "/network/migration/" in path:
                    self._json(
                        200,
                        {
                            "vm_id": vid,
                            "identity": 42,
                            "phase": "running",
                            "generation": 0,
                            "dataplane_schema_version": 9,
                            "schema_compatible": True,
                        },
                    )
                    return
                if "/migration/status" in path or path.endswith("/migration"):
                    self._json(
                        200,
                        {
                            "phase": "none",
                            "status": "idle",
                            "ram_transferred": None,
                            "ram_remaining": None,
                            "ram_total": None,
                            "total_time_ms": None,
                            "downtime_ms": None,
                            "error": None,
                        },
                    )
                    return
                if path.endswith("/stats") or path.endswith("/pressure") or path.endswith("/cpuset"):
                    self._json(200, {"cpu_usage_percent": 1.0, "memory_usage_bytes": 1, "disk_read_bytes": 0, "disk_write_bytes": 0})
                    return
                if name:
                    self._json(200, vm_record(name, VMS[name]["status"]))
                    return

        if path.startswith("/v1/migration/receivers/"):
            self._json(
                200,
                {
                    "id": RECV_ID,
                    "status": "receiving",
                    "port": 4444,
                    "expires_at": "2026-01-01T01:00:00Z",
                },
            )
            return

        if path.startswith("/v1/network/"):
            if path.endswith("/groups") or "/groups" in path:
                self._json(200, {"items": []})
                return
            if "ipcache" in path or "observe" in path or "health" in path or "identities" in path:
                self._json(
                    200,
                    {
                        "mode": "ebpf",
                        "required": True,
                        "default_allow": False,
                        "bpf_object_present": True,
                        "pin_root_present": True,
                        "bpffs_present": True,
                        "cilium_socket_present": False,
                        "groups": 0,
                        "policies": 0,
                        "ipcache_entries": 0,
                        "ok": True,
                        "items": [],
                    },
                )
                return
            if "services" in path:
                self._json(200, {"items": []})
                return

        self._json(200, {"status": "ok", "version": "stub"})

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        body = self._read_json()

        if path == "/v1/vms":
            name = body.get("name") or f"vm-{len(VMS)+1}"
            vid = str(uuid.uuid4())
            VMS[name] = {"id": vid, "status": "running"}
            self._json(201, vm_record(name, "running"))
            return

        if path == "/v1/migration/receivers":
            if not body.get("spec"):
                self._json(422, {"error": "missing field `spec`"})
                return
            self._json(
                201,
                {
                    "id": RECV_ID,
                    "status": "receiving",
                    "port": 4444,
                    "expires_at": "2026-01-01T01:00:00Z",
                },
            )
            return

        if path.endswith("/activate"):
            rec = vm_record("recv-activated", "running")
            rec["id"] = RECV_ID
            self._json(200, rec)
            return

        if "/pause" in path:
            for meta in VMS.values():
                if meta["id"] in path or True:
                    meta["status"] = "paused"
            # mark matching id
            for name, meta in VMS.items():
                if meta["id"] in path:
                    meta["status"] = "paused"
                    self._json(200, vm_record(name, "paused"))
                    return
            self._json(200, {"status": "paused"})
            return

        if "/resume" in path or "/start" in path:
            for name, meta in VMS.items():
                if meta["id"] in path:
                    meta["status"] = "running"
                    self._json(200, vm_record(name, "running"))
                    return
            self._json(200, {"status": "running"})
            return

        if path.endswith("/qga/ping"):
            self._json(200, {"ok": True})
            return

        if "/qga/" in path:
            self._json(502, {"error": "qga unavailable on stub"})
            return

        if "/network/pod-policy" in path:
            self._json(400, {"error": "VM has no Pod identity"})
            return

        if "/agent" in path:
            self._json(200, {"result": "pong"})
            return

        self._json(200, {"status": "ok"})

    def do_DELETE(self) -> None:
        path = urlparse(self.path).path
        if "/network/pod-policy" in path:
            self._json(200, {"ok": True})
            return
        if path.startswith("/v1/migration/receivers/"):
            self.send_response(204)
            self.end_headers()
            return
        if path.startswith("/v1/vms/"):
            parts = path.strip("/").split("/")
            if len(parts) >= 3:
                vid = parts[2]
                for name, meta in list(VMS.items()):
                    if meta["id"] == vid or name == vid:
                        del VMS[name]
                        break
        self.send_response(204)
        self.end_headers()

    def do_PUT(self) -> None:
        self._read_json()
        self._json(200, {"status": "ok"})


def main() -> None:
    server = HTTPServer(("127.0.0.1", PORT), Handler)
    print(f"fluxvm-stub listening on 127.0.0.1:{PORT}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
