// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import net from "node:net";

const here = fileURLToPath(new URL(".", import.meta.url));
const workerPath = resolve(here, "../../../agent-runtime/src/worker.mjs");

async function freePort() {
  const server = net.createServer();
  await new Promise((resolveReady, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveReady);
  });
  const port = server.address().port;
  await new Promise((resolveClose) => server.close(resolveClose));
  return port;
}

async function poll(url, predicate, timeoutMs = 3000) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        last = await response.json();
        if (predicate(last)) return last;
      }
    } catch {}
    await new Promise((resolveWait) => setTimeout(resolveWait, 25));
  }
  throw new Error(`poll timeout; last=${JSON.stringify(last)}`);
}

test("worker exposes waiting state only while blocked in nextSteer", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "zyvor-agent-worker-"));
  const bundle = join(dir, "bundle.mjs");
  await writeFile(bundle, `export default async (ctx) => {\n  const value = await ctx.nextSteer({ timeoutMs: 5000 });\n  return { value };\n};\n`);
  const port = await freePort();
  const child = spawn(process.execPath, [workerPath], {
    env: {
      ...process.env,
      ZYVOR_SESSION_ID: "test-session",
      ZYVOR_EGRESS_CAPABILITY: "test-capability",
      ZYVOR_EGRESS_BROKER: "http://127.0.0.1:9",
      ZYVOR_AGENT_PORT: String(port),
      ZYVOR_AGENT_BUNDLE: bundle,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    child.kill("SIGTERM");
    await rm(dir, { recursive: true, force: true });
  });

  await poll(`http://127.0.0.1:${port}/health`, (v) => v.ok === true);
  const run = await fetch(`http://127.0.0.1:${port}/run`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ input: { prompt: "hello" } }),
  });
  assert.equal(run.status, 202);

  const waiting = await poll(`http://127.0.0.1:${port}/status`, (v) => v.status === "waiting");
  assert.equal(waiting.status, "waiting");

  const steer = await fetch(`http://127.0.0.1:${port}/steer`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ message: { focus: "KVM" } }),
  });
  assert.equal(steer.status, 202);

  await poll(`http://127.0.0.1:${port}/status`, (v) => v.status === "completed");
  const events = await (await fetch(`http://127.0.0.1:${port}/events?after=0`)).json();
  const kinds = events.items.map((e) => e.kind);
  assert.ok(kinds.includes("session.waiting"));
  assert.ok(kinds.includes("session.running"));
  assert.ok(kinds.includes("session.result"));
});

test("worker gives the agent its memory as frozen data, keeps it out of the events, and lets it propose", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "zyvor-agent-worker-"));
  const bundle = join(dir, "bundle.mjs");
  await writeFile(bundle, `export default async (ctx) => {
  let frozen = false;
  try { ctx.memory.items[0].text = "changed"; } catch { frozen = true; }
  ctx.memory.propose("likes window seats", "preference");
  return { items: ctx.memory.items.map((i) => i.text), tainted: ctx.memory.items.map((i) => i.tainted), frozen: frozen || ctx.memory.items[0].text !== "changed" };
};\n`);
  const port = await freePort();
  const child = spawn(process.execPath, [workerPath], {
    env: { ...process.env, ZYVOR_SESSION_ID: "s", ZYVOR_EGRESS_CAPABILITY: "c", ZYVOR_EGRESS_BROKER: "http://127.0.0.1:9", ZYVOR_AGENT_PORT: String(port), ZYVOR_AGENT_BUNDLE: bundle },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => { child.kill("SIGTERM"); await rm(dir, { recursive: true, force: true }); });
  await poll(`http://127.0.0.1:${port}/health`, (v) => v.ok === true);
  const run = await fetch(`http://127.0.0.1:${port}/run`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ input: { message: "hi" }, memory: [{ text: "vegetarian", kind: "fact", pinned: true, tainted: false }, { text: "from a web page", kind: "note", pinned: false, tainted: true }] }),
  });
  assert.equal(run.status, 202);
  const events = await poll(`http://127.0.0.1:${port}/events?after=0`, (v) => v.items.some((e) => e.kind === "session.result"));
  const result = events.items.find((e) => e.kind === "session.result").data;
  assert.deepEqual(result.items, ["vegetarian", "from a web page"]);
  assert.deepEqual(result.tainted, [false, true]);
  assert.equal(result.frozen, true, "an agent cannot edit the entries it was given");
  const proposal = events.items.find((e) => e.kind === "memory.propose");
  assert.deepEqual(proposal.data, { text: "likes window seats", kind: "preference" });
  assert.equal(JSON.stringify(events.items.filter((e) => e.kind !== "memory.propose" && e.kind !== "session.result")).includes("vegetarian"), false, "memory is not echoed into the session events");
});

test("worker without memory in the run request gives an empty list", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "zyvor-agent-worker-"));
  const bundle = join(dir, "bundle.mjs");
  await writeFile(bundle, "export default async (ctx) => ({ n: ctx.memory.items.length });\n");
  const port = await freePort();
  const child = spawn(process.execPath, [workerPath], {
    env: { ...process.env, ZYVOR_SESSION_ID: "s", ZYVOR_EGRESS_CAPABILITY: "c", ZYVOR_EGRESS_BROKER: "http://127.0.0.1:9", ZYVOR_AGENT_PORT: String(port), ZYVOR_AGENT_BUNDLE: bundle },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => { child.kill("SIGTERM"); await rm(dir, { recursive: true, force: true }); });
  await poll(`http://127.0.0.1:${port}/health`, (v) => v.ok === true);
  await fetch(`http://127.0.0.1:${port}/run`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ input: {} }) });
  const events = await poll(`http://127.0.0.1:${port}/events?after=0`, (v) => v.items.some((e) => e.kind === "session.result"));
  assert.deepEqual(events.items.find((e) => e.kind === "session.result").data, { n: 0 });
});
