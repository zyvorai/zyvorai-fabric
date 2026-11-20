# Tutorial 11: Agent Runtime Quickstart

Deploy your first durable, FluxVM-sandboxed TypeScript agent end to end:
build a Node.js-capable FluxVM template (the one step nothing else in this
repo walks you through), deploy an agent, and drive a session through the
Agent Runtime API.

**Level:** Intermediate
**Time:** 40 minutes
**Prerequisites:** A Linux host with real KVM (`/dev/kvm`), `fluxvm serve`
running and reachable, Rust toolchain, Node.js 20+ locally (for the SDK/CLI),
`curl`, `jq`, `sudo` access on the FluxVM host.

> **Known limitation.** `POST /v1/sandboxes` always creates a FluxVM
> `flux-vm`-backend sandbox (its own in-tree microVM hypervisor), regardless
> of a template's `backend` field. This backend is less mature than FluxVM's
> QEMU backend for general-purpose images — confirmed live that it hangs
> bringing up a second vCPU, and separately hangs during guest kernel boot at
> virtio-mmio device probe on at least one general-purpose Ubuntu+Node.js
> image, even with a single vCPU. This tutorial's Steps 1-3 (build, register,
> deploy) all work today and are worth doing on their own — they're the
> actual undocumented gap. Step 4 (running a full session to completion)
> currently depends on that upstream fix landing; see
> [agent-runtime/README.md](../../agent-runtime/README.md#host-requirements)
> for the latest status before assuming it will complete.

GitHub Actions does not have KVM or FluxVM. The agent-runtime workflow
still runs a full session there: [`agent-runtime/tests/session-ci.sh`](../../agent-runtime/tests/session-ci.sh)
stores guest files and starts the Node worker on the runner. That covers
the ops agent, MCP, schedules, webhooks, loops, delegation, a harness
approval, and the hello-go agent. It is not a substitute for Step 4 on a
real FluxVM template.

---

## What You Will Learn

1. Build a FluxVM sandbox template with Node.js baked in, the way that
   actually works (not via `apt`/`nodesource` inside the image-build chroot —
   that path has no network access, a real bug found building this tutorial)
2. Register the template and the in-guest FluxVM agent so `/v1/sandboxes`
   can use it
3. Deploy an agent with `fabric-agent deploy`
4. Drive a session through the Agent Runtime HTTP API: create, stream
   events, steer, inspect

---

## Step 1: Build a Node.js FluxVM template image

FluxVM's `build-image` customizes a base disk image via `guestkit`
(mount + chroot), but its `packages`/`commands` split has a sharp edge: DNS
is only staged into the guest for the `packages` step, not for `commands` —
so anything in `commands` that needs the network (like curling the
`nodesource` setup script and running `apt-get install nodejs` from it)
fails with `Temporary failure resolving 'archive.ubuntu.com'`. The fix is to
download Node.js on the **host** (where DNS definitely works) and extract it
inside the guest with a purely local `tar -xf` — no network needed in the
chroot at all.

```bash
# On the FluxVM host
curl -fsSL -o /tmp/node20.tar.xz \
  https://nodejs.org/dist/v20.18.1/node-v20.18.1-linux-x64.tar.xz

# The guest agent binary + its systemd unit get baked in too, so the
# resulting sandbox has vsock exec/fs already working the moment it boots.
cat > /tmp/node22-agent-build.json <<EOF
{
  "source": "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img",
  "output": "/var/lib/fluxvm/images/node22-agent.qcow2",
  "format": "qcow2",
  "size_gib": 8,
  "hostname": "node22-agent",
  "commands": [
    "tar -xf /root/node20.tar.xz -C /usr/local --strip-components=1",
    "chmod +x /usr/local/bin/fluxvm-guest-agent",
    "/usr/local/bin/node --version",
    "rm -f /root/node20.tar.xz"
  ],
  "copy_in": [
    {"src": "/tmp/node20.tar.xz", "dest": "/root/node20.tar.xz"},
    {"src": "/path/to/fluxvm/target/release/fluxvm-guest-agent", "dest": "/usr/local/bin/fluxvm-guest-agent"},
    {"src": "/path/to/fluxvm/systemd/fluxvm-guest-agent.service", "dest": "/etc/systemd/system/fluxvm-guest-agent.service"}
  ],
  "enable_services": ["fluxvm-guest-agent"]
}
EOF

sudo fluxvm --config /etc/fluxvm.toml build-image --spec /tmp/node22-agent-build.json
```

Substitute `/path/to/fluxvm` with your FluxVM checkout — build it first with
`cargo build --release --bin fluxvm-guest-agent` if you haven't already.
This step takes a few minutes (image download + `qemu-img convert` +
guestkit customization).

---

## Step 2: Register the template

```bash
sudo mkdir -p /var/lib/fluxvm/templates/node22-agent
sudo tee /var/lib/fluxvm/templates/node22-agent/spec.json > /dev/null <<'EOF'
{
  "name": "node22-agent",
  "backend": "qemu",
  "image": "/var/lib/fluxvm/images/node22-agent.qcow2",
  "vcpus": 1,
  "memory_mib": 2048,
  "network": {"mode": "tap", "netns": true, "mac": "52:54:00:9f:a9:1e"},
  "agent": {"enabled": true}
}
EOF

curl -s http://127.0.0.1:7788/v1/templates | jq .
```

Two gotchas confirmed live, neither obvious from the API alone:

- `vcpus` **must be 1**. `POST /v1/sandboxes` always uses the `flux-vm`
  backend (see the Known limitation note above) regardless of what
  `backend` says here, and that backend's SMP bring-up hangs with 2+ vCPUs.
- `network.mac` is **required** with `netns: true` — omitting it fails with
  `netns networking requires an explicit MAC address`. Any locally
  administered address works (`52:54:00:xx:xx:xx` is the common QEMU/libvirt
  convention).

---

## Step 3: Deploy an agent

```bash
cd sdk/agent-runtime && npm install --no-audit --no-fund

# credentials.json: only the env var *name* Agent Runtime should read the
# secret from at request time -- the value itself never goes in this file.
cat > /tmp/credentials.json <<'EOF'
{
  "anthropic": {
    "host": "api.anthropic.com",
    "header": "x-api-key",
    "env": "ANTHROPIC_API_KEY",
    "allowed_methods": ["POST"],
    "path_prefixes": ["/v1/"]
  }
}
EOF

export ZYVOR_AGENT_API_TOKEN="a-real-token-here"   # required: agent-runtime refuses to start without one
export ZYVOR_AGENT_CREDENTIALS_FILE=/tmp/credentials.json
export ANTHROPIC_API_KEY="sk-..."
cd agent-runtime && cargo run --release &

FABRIC_AGENT_URL=http://127.0.0.1:9096 \
FABRIC_AGENT_TOKEN="$ZYVOR_AGENT_API_TOKEN" \
../sdk/agent-runtime/src/cli.js deploy ../examples/agent-runtime/agent.ts \
  --name research-agent \
  --template node22-agent \
  --credential anthropic \
  --allow-host api.anthropic.com \
  --ttl 300
```

A successful deploy prints `Deployed research-agent@<version>` with a
`sha256:` digest.

---

## Step 4: Create and drive a session

```bash
H=http://127.0.0.1:9096
AUTH=(-H "Authorization: Bearer $ZYVOR_AGENT_API_TOKEN")

SESSION=$(curl -sS "${AUTH[@]}" -X POST "$H/v1/sessions" \
  -H 'content-type: application/json' \
  -d '{"agent":"research-agent","input":{"prompt":"Compare KVM and Firecracker"}}')
echo "$SESSION" | jq .
SESSION_ID=$(echo "$SESSION" | jq -r .id)

# Stream events (Ctrl-C to stop)
curl -sSN "${AUTH[@]}" "$H/v1/sessions/$SESSION_ID/events"

# Steer mid-run
curl -sS "${AUTH[@]}" -X POST "$H/v1/sessions/$SESSION_ID/steer" \
  -H 'content-type: application/json' \
  -d '{"message":{"instruction":"Also cover Cloud Hypervisor"}}'

# Inspect state at any point
curl -sS "${AUTH[@]}" "$H/v1/sessions/$SESSION_ID" | jq .
```

If the session sticks in `"status": "creating"` past a minute or two, that's
the Known limitation above, not a misconfiguration — check
`curl -s http://127.0.0.1:7788/v1/vms/<sandbox_id> | jq .status` and the
FluxVM daemon's own log for the guest boot state.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `preparing network namespace: netns networking requires an explicit MAC address` | Add `network.mac` to the template `spec.json` (Step 2) |
| `apt-get install nodejs` fails with `Temporary failure resolving ...` during `build-image` | DNS isn't staged for the `commands` step — use the host-download + local `tar -xf` approach in Step 1 instead |
| `connecting to vsock proxy socket ... No such file or directory` right after session create | Expected transiently on a cold start; `agent-runtime` retries this internally now. If it persists past `ZYVOR_AGENT_GUEST_START_TIMEOUT_SECS` (default 30s), the guest never finished booting — see the Known limitation |
| `ZYVOR_AGENT_API_TOKEN is not set` on `agent-runtime` startup | Set it, or explicitly opt out with `ZYVOR_AGENT_ALLOW_NO_AUTH=1` (only for a loopback-only dev instance) |
| `Could not resolve "@zyvor/fabric-agent"` from `fabric-agent deploy` | Fixed as of the version that shipped alongside this tutorial — update if you see this |

---

## What You Accomplished

- Built a FluxVM sandbox template with Node.js and the guest agent baked in,
  working around the `build-image` `commands`-step DNS gap
- Registered it with the correct `netns` MAC and single-vCPU settings
- Deployed a real agent bundle and understood the credential-descriptor model
- Exercised the full session API surface: create, stream, steer, inspect

## Next Steps

1. Reference: [agent-runtime/README.md](../../agent-runtime/README.md) — full HTTP API, warm pools, hibernation, security model
2. Fan-out and idempotency: `request_id` and `sessions.createMany()` in the README's "Reliable retries and fan-out" section
3. Container workloads on the same FluxVM host: [Container Groups](../container-groups.md)
