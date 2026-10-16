import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, stat, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:http";
import { collectEvidence, verifyEvidence } from "../src/evidence.js";
import { run } from "../src/evidence-cli.js";

const ID = "d564e4fe-3658-4b13-9336-45cbbe1a89d1";
const OTHER = "f2e640a0-8c75-4730-9522-366677442598";

function client({ chainOk = true, audit = [{ session_id: ID, seq: 4, action: "send" }],
  receipts = [{ session_id: ID, id: "receipt-1" }, { session_id: OTHER, id: "receipt-2" }] } = {}) {
  const calls = [];
  return {
    calls,
    evidence: {
      cockpit: async (sessionId) => { calls.push(["cockpit", sessionId]); return { session_id: sessionId, status: "completed" }; },
      exportAudit: async (options) => {
        calls.push(["audit", options]);
        return { export: true, chain: { chain_ok: chainOk, entries: 7 }, items: audit };
      },
      receipts: async (options) => { calls.push(["receipts", options]); return receipts; },
    },
  };
}

test("one scoped snapshot links cockpit, audit and receipts without serializing the export token", async () => {
  const sdk = client();
  const bundle = await collectEvidence(sdk, { sessionId: ID, exportToken: "secret-export", capturedAt: () => "2026-09-29T00:00:00Z" });
  assert.deepEqual(sdk.calls, [
    ["cockpit", ID],
    ["audit", { exportToken: "secret-export", sessionId: ID, limit: 5000 }],
    ["receipts", { limit: 500 }],
  ]);
  assert.deepEqual(bundle.payload.receipts.map((r) => r.id), ["receipt-1"]);
  assert.equal(bundle.payload.source.receipts_complete, false);
  assert.ok(!JSON.stringify(bundle).includes("secret-export"));
  assert.deepEqual(verifyEvidence(bundle).findings, []);
  bundle.payload.cockpit.status = "running";
  assert.deepEqual(verifyEvidence(bundle).findings, ["payload checksum mismatch"]);
});

test("rejects broken runtime chain and cross-session audit records", async () => {
  await assert.rejects(collectEvidence(client({ chainOk: false }), { sessionId: ID, exportToken: "x" }), /verified audit export/);
  await assert.rejects(collectEvidence(client({ audit: [{ session_id: OTHER }] }),
    { sessionId: ID, exportToken: "x" }), /another session/);
  await assert.rejects(collectEvidence(client(), { sessionId: "../bad", exportToken: "x" }), /session UUID/);
});

test("saturated windows are labeled; a valid checksum is not represented as a signature", async () => {
  const sdk = client({ audit: Array.from({ length: 5000 }, (_, seq) => ({ session_id: ID, seq })),
    receipts: Array.from({ length: 500 }, (_, n) => ({ session_id: ID, id: `${n}` })) });
  const bundle = await collectEvidence(sdk, { sessionId: ID, exportToken: "x" });
  assert.equal(bundle.payload.source.audit_window_saturated, true);
  assert.equal(bundle.payload.source.receipt_window_saturated, true);
  assert.equal(verifyEvidence(bundle).verification, "local-checksum-and-shape-only");
});

test("CLI creates a private file and refuses to overwrite it", async () => {
  const dir = await mkdtemp(join(tmpdir(), "keep-evidence-"));
  const path = join(dir, "proof.json");
  const messages = [];
  try {
    const opts = { env: { MERION_API_TOKEN: "operator", MERION_EXPORT_TOKEN: "export" },
      clientFactory: () => client(), write: (s) => messages.push(s) };
    assert.equal(await run(["collect", "--session", ID, "--out", path], opts), 0);
    assert.equal((await stat(path)).mode & 0o777, 0o600);
    assert.equal(await run(["verify", path], opts), 0);
    const original = await readFile(path, "utf8");
    await assert.rejects(run(["collect", "--session", ID, "--out", path], opts), { code: "EEXIST" });
    assert.equal(await readFile(path, "utf8"), original);
    await writeFile(path, original.replace("completed", "running"));
    assert.equal(await run(["verify", path], opts), 1);
    assert.ok(messages.at(-1).includes("payload checksum mismatch"));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI talks to the runtime HTTP contract with separate bearer and export headers", async () => {
  const observed = [];
  const server = createServer((req, res) => {
    observed.push({ path: req.url, bearer: req.headers.authorization, export: req.headers["x-keep-export-token"] });
    res.setHeader("content-type", "application/json");
    if (req.headers.authorization !== "Bearer operator") {
      res.statusCode = 401;
      res.end(JSON.stringify({ error: "unauthorized" }));
      return;
    }
    if (req.url === `/v1/sessions/${ID}/cockpit`) res.end(JSON.stringify({ session_id: ID, status: "completed" }));
    else if (req.url?.startsWith("/v1/export/audit?")) {
      if (req.headers["x-keep-export-token"] !== "audit-only") {
        res.statusCode = 403;
        res.end(JSON.stringify({ error: "missing export capability" }));
      } else res.end(JSON.stringify({ export: true, chain: { chain_ok: true, entries: 1 },
        items: [{ session_id: ID, seq: 0 }] }));
    } else if (req.url?.startsWith("/v1/receipts?")) res.end(JSON.stringify({ items: [{ session_id: ID, id: "r1" }] }));
    else { res.statusCode = 404; res.end("{}"); }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const dir = await mkdtemp(join(tmpdir(), "keep-http-evidence-"));
  const path = join(dir, "snapshot.json");
  try {
    const address = server.address();
    const code = await run(["collect", "--session", ID, "--out", path,
      "--url", `http://127.0.0.1:${address.port}`],
    { env: { MERION_API_TOKEN: "operator", MERION_EXPORT_TOKEN: "audit-only" }, write: () => {} });
    assert.equal(code, 0);
    assert.equal(observed.length, 3);
    assert.ok(observed.every((req) => req.bearer === "Bearer operator"));
    assert.equal(observed[1].export, "audit-only");
    assert.ok(observed.every((req) => !req.path.includes("audit-only")));
    const snapshot = JSON.parse(await readFile(path, "utf8"));
    assert.equal(verifyEvidence(snapshot).ok, true);
  } finally {
    await new Promise((resolve) => server.close(resolve));
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI still accepts the pre-rename KEEP_* token names", async () => {
  const dir = await mkdtemp(join(tmpdir(), "keep-evidence-legacy-"));
  const path = join(dir, "proof.json");
  try {
    const opts = { env: { KEEP_API_TOKEN: "operator", KEEP_EXPORT_TOKEN: "export" },
      clientFactory: () => client(), write: () => {} };
    assert.equal(await run(["collect", "--session", ID, "--out", path], opts), 0);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
