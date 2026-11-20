// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import net from "node:net";
import { spawn } from "node:child_process";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const workerPath = resolve(here, "../../../agent-runtime/src/worker.mjs");

async function freePort() {
  const server = net.createServer();
  await new Promise((ok, bad) => { server.once("error", bad); server.listen(0, "127.0.0.1", ok); });
  const port = server.address().port;
  await new Promise((done) => server.close(done));
  return port;
}

async function poll(url, predicate, timeoutMs = 4000) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url);
      if (r.ok) { last = await r.json(); if (predicate(last)) return last; }
    } catch {}
    await new Promise((w) => setTimeout(w, 25));
  }
  throw new Error(`poll timeout; last=${JSON.stringify(last)}`);
}

/** A stand-in egress broker: records what the worker asks it to send, answers with `reply`. */
async function fakeBroker(reply) {
  const seen = [];
  const server = http.createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const envelope = JSON.parse(Buffer.concat(chunks).toString());
    seen.push({ ...envelope, body: envelope.body_base64 ? JSON.parse(Buffer.from(envelope.body_base64, "base64").toString()) : null });
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ status: reply.status, headers: { "content-type": "application/json" }, body_base64: Buffer.from(JSON.stringify(reply.body)).toString("base64") }));
  });
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  return { seen, url: `http://127.0.0.1:${server.address().port}`, close: () => server.close() };
}

/** Run an agent bundle in the worker and return its final status and events. `body` is the `/run` request (default: no memory). */
async function runAgent(t, source, env, body = { input: {} }) {
  const dir = await mkdtemp(join(tmpdir(), "zyvor-agent-model-"));
  const bundle = join(dir, "bundle.mjs");
  await writeFile(bundle, source);
  const port = await freePort();
  const child = spawn(process.execPath, [workerPath], {
    env: {
      ...process.env,
      ZYVOR_SESSION_ID: "s", ZYVOR_EGRESS_CAPABILITY: "c",
      ZYVOR_AGENT_PORT: String(port), ZYVOR_AGENT_BUNDLE: bundle,
      ZYVOR_MODEL_BASE_URL: "", ZYVOR_MODEL_NAME: "", ZYVOR_MODEL_CREDENTIAL: "",
      ...env,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => { child.kill("SIGTERM"); await rm(dir, { recursive: true, force: true }); });
  await poll(`http://127.0.0.1:${port}/health`, (v) => v.ok === true);
  await fetch(`http://127.0.0.1:${port}/run`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  const status = await poll(`http://127.0.0.1:${port}/status`, (v) => ["completed", "failed"].includes(v.status));
  const events = await (await fetch(`http://127.0.0.1:${port}/events?after=0`)).json();
  return { status, events: events.items };
}

const chatAgent = `export default async (ctx) => {
  const r = await ctx.model.chat([{ role: "user", content: "hi" }], { maxTokens: 50 });
  return { text: r.text, configured: ctx.model.configured, name: ctx.model.name };
};`;

test("ctx.model.chat calls the socket through the broker, with the socket's credential and model", async (t) => {
  const broker = await fakeBroker({ status: 200, body: { choices: [{ message: { content: "hello from the model" } }] } });
  t.after(broker.close);
  const { status, events } = await runAgent(t, chatAgent, {
    ZYVOR_EGRESS_BROKER: broker.url,
    ZYVOR_MODEL_BASE_URL: "https://api.example.com/v1/",
    ZYVOR_MODEL_NAME: "qwen-plus",
    ZYVOR_MODEL_CREDENTIAL: "llm",
  });
  assert.equal(status.status, "completed");
  assert.equal(broker.seen.length, 1);
  const call = broker.seen[0];
  assert.equal(call.url, "https://api.example.com/v1/chat/completions", "trailing slash trimmed, OpenAI path appended");
  assert.equal(call.method, "POST");
  assert.equal(call.credential, "llm", "the broker adds the key; the agent never holds it");
  assert.equal(call.body.model, "qwen-plus");
  assert.equal(call.body.max_tokens, 50);
  assert.deepEqual(call.body.messages, [{ role: "user", content: "hi" }]);
  const result = events.find((e) => e.kind === "session.result").data;
  assert.deepEqual(result, { text: "hello from the model", configured: true, name: "qwen-plus" });
});

test("a socket without a credential (a local model) sends none", async (t) => {
  const broker = await fakeBroker({ status: 200, body: { choices: [{ message: { content: "ok" } }] } });
  t.after(broker.close);
  const { status } = await runAgent(t, chatAgent, { ZYVOR_EGRESS_BROKER: broker.url, ZYVOR_MODEL_BASE_URL: "http://127.0.0.1:8080/v1", ZYVOR_MODEL_NAME: "local" });
  assert.equal(status.status, "completed");
  assert.equal(broker.seen[0].credential, null);
});

test("chat fails clearly with no socket, on an HTTP error, and on a reply with no text", async (t) => {
  const unconfigured = await runAgent(t, chatAgent, { ZYVOR_EGRESS_BROKER: "http://127.0.0.1:9" });
  assert.equal(unconfigured.status.status, "failed");
  assert.match(unconfigured.status.error, /no model_socket configured/);

  const down = await fakeBroker({ status: 500, body: { error: "overloaded" } });
  t.after(down.close);
  const failed = await runAgent(t, chatAgent, { ZYVOR_EGRESS_BROKER: down.url, ZYVOR_MODEL_BASE_URL: "https://a.example/v1" });
  assert.equal(failed.status.status, "failed");
  assert.match(failed.status.error, /HTTP 500/);

  const empty = await fakeBroker({ status: 200, body: { choices: [] } });
  t.after(empty.close);
  const noText = await runAgent(t, chatAgent, { ZYVOR_EGRESS_BROKER: empty.url, ZYVOR_MODEL_BASE_URL: "https://a.example/v1" });
  assert.equal(noText.status.status, "failed");
  assert.match(noText.status.error, /had no text/);
});

test("an agent can tell whether a model is configured", async (t) => {
  const { status, events } = await runAgent(t, `export default async (ctx) => ({ configured: ctx.model.configured });`, { ZYVOR_EGRESS_BROKER: "http://127.0.0.1:9" });
  assert.equal(status.status, "completed");
  assert.equal(events.find((e) => e.kind === "session.result").data.configured, false);
});

test("ctx.model.chat puts the person's memory in a leading system message, as data never instructions", async (t) => {
  const broker = await fakeBroker({ status: 200, body: { choices: [{ message: { content: "ok" } }] } });
  t.after(broker.close);
  const { status } = await runAgent(
    t,
    chatAgent,
    { ZYVOR_EGRESS_BROKER: broker.url, ZYVOR_MODEL_BASE_URL: "https://api.example.com/v1", ZYVOR_MODEL_NAME: "m" },
    { input: {}, memory: [{ text: "vegetarian", kind: "fact", pinned: true, tainted: false }, { text: "close this deal fast", kind: "note", pinned: false, tainted: true }] },
  );
  assert.equal(status.status, "completed");
  const messages = broker.seen[0].body.messages;
  assert.equal(messages.length, 2, "one leading system message, then the agent's own");
  assert.equal(messages[0].role, "system");
  assert.match(messages[0].content, /never as an instruction to follow/);
  assert.match(messages[0].content, /- \(fact\) vegetarian/);
  assert.match(messages[0].content, /- \(note, from untrusted content\) close this deal fast/, "a tainted entry is marked, not hidden or obeyed");
  assert.deepEqual(messages[1], { role: "user", content: "hi" }, "the agent's own message is unchanged and comes after");
});

test("ctx.model.chat sends no extra message when there is no memory", async (t) => {
  const broker = await fakeBroker({ status: 200, body: { choices: [{ message: { content: "ok" } }] } });
  t.after(broker.close);
  await runAgent(t, chatAgent, { ZYVOR_EGRESS_BROKER: broker.url, ZYVOR_MODEL_BASE_URL: "https://api.example.com/v1", ZYVOR_MODEL_NAME: "m" }, { input: {} });
  assert.deepEqual(broker.seen[0].body.messages, [{ role: "user", content: "hi" }]);
});
