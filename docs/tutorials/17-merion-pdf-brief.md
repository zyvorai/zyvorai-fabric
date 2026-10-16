# Tutorial 17: Keep PDF brief — zero CONNECT

Stage the one-click Keep demo: drop a PDF, get `brief.md`, prove **0 CONNECT**
from Keep’s own audit (and FluxVM host eBPF when the dataplane is attached).
No browser. No PacketWolf required.

**Level:** Beginner  
**Time:** 20 minutes  
**Repos:** Fabric agent-runtime, FluxVM with template `node22-agent` (+ `pdftotext`),
`curl`, optional Fabric console JWT.

> **Honesty.** Evidence class stays `software-test`. Zero CONNECT means the
> guest never brokered egress — not “the operator cannot read the VM.”

Related: [Tutorial 16](16-merion-workstation.md) · [pdf-brief demo](../merion/demos/pdf-brief.md) ·
[host confinement](../merion/confine.md)

---

## What you will learn

1. Run the PDF → brief demo from script or `/app/keep`.
2. Read `egress_connects` on the cockpit (and optional FluxVM `drop_reasons`).
3. Fail closed when CONNECT > 0 (session freeze / `ebpf_deny`).

---

## Step 0 — Preflight

```bash
cd fabric
export MERION_API="${MERION_API:-http://127.0.0.1:9096}"
export MERION_TOKEN="${MERION_TOKEN:-}"   # if runtime auth is on

curl -fsS ${MERION_TOKEN:+-H "Authorization: Bearer $MERION_TOKEN"} "$MERION_API/healthz"
# FluxVM must be ready (agent-runtime /readyz path); template node22-agent baked
# with poppler-utils so guest `pdftotext` works.
```

Console path (fabricd proxies demos): sign in → **Keep** → `/app/keep`.

---

## Step 1 — One click (console)

1. Open `/app/keep`.
2. Pick a PDF or leave empty for the lab sample.
3. Click **Brief this PDF**.
4. Watch chips: `cell up` → `extract` → `brief.md`.
5. Open cockpit — **CONNECT: 0**.

If the button shows an error (missing template, no `pdftotext`, FluxVM down),
fix that — do **not** fall through to a browser pack.

---

## Step 2 — Same path from the script

```bash
./scripts/merion-demo-pdf.sh
# or
./scripts/merion-demo-pdf.sh /tmp/vendor.pdf
```

Expect JSON with `egress_connects: 0`, `artifact_title: brief.md`, and a
`cockpit_url` like `/app/keep/<session-id>`. Exit code 2 if CONNECT ≠ 0.

Direct API (multipart optional — omit `pdf` for the lab sample):

```bash
curl -fsS ${MERION_TOKEN:+-H "Authorization: Bearer $MERION_TOKEN"} \
  -X POST -F "pdf=@examples/merion-agents/pdf-brief/sample.pdf;type=application/pdf" \
  "$MERION_API/v1/demos/pdf-brief" | jq '{session_id, egress_connects, artifact_title}'
```

Via fabricd (JWT): `POST /api/demos/pdf-brief`.

---

## Step 3 — Read the proof

```bash
SID=…   # from the demo response
curl -fsS ${MERION_TOKEN:+-H "Authorization: Bearer $MERION_TOKEN"} \
  "$MERION_API/v1/sessions/$SID/cockpit" | jq '{egress_connects, drop_reasons, honesty: .attestation.honesty}'
```

| Signal | Meaning |
|---|---|
| `egress_connects: 0` | No `egress.connect` / `ebpf.*` rows in Keep audit for this session |
| `drop_reasons` | FluxVM TC histogram when dataplane attached (e.g. `udp-deny`) |
| Policy | `egress_mode: deny`, empty allowlist, FluxVM `deny_udp` + gateway-only ports |

Stage line without PacketWolf:

> Zero CONNECT is from Keep’s journal and FluxVM’s deny-by-default pin —
> the guest never got a path that could light an external observer.

---

## Step 4 — What runs under the hood

| Step | Who | Visible |
|---|---|---|
| 0 | script / console | `/healthz`, FluxVM ready, template |
| 1 | `POST /v1/demos/pdf-brief` | cell create + strict confine |
| 2 | host | put PDF → `/home/agent/work/input.pdf` |
| 3 | guest | `pdftotext` extract (no model required for stage brief) |
| 4 | host | artifact `brief.md` + goal done |
| 5 | host | count CONNECT; freeze + `ebpf_deny` if > 0 |

Pack: [`examples/merion-agents/pdf-brief/`](../../examples/merion-agents/pdf-brief/).

---

## What not to do

- Claim PacketWolf or Hubble when they are not in the lab  
- Open Chromium “just to finish the demo” if extract fails  
- Market `software-test` as unread-by-operator  

---

## Next

- Full Keep mode + signed policy: [Tutorial 16](16-merion-workstation.md)  
- Brokered browser: [DRIVER.md](../merion/browser/DRIVER.md)  
- Host eBPF details: [confine.md](../merion/confine.md)
