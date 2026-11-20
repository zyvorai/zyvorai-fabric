// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, writeFile, readFile, chmod, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, delimiter } from "node:path";
import { fileURLToPath } from "node:url";
import net from "node:net";

const here = fileURLToPath(new URL(".", import.meta.url));
const harnessPath = resolve(here, "../../../agent-runtime/src/harness.mjs");

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

/** A stand-in for the `claude`/`codex`/`gemini` CLI: never the real thing. Exits 0 immediately, so `run` completes
 * without ever making a model call — this test is about what harness.mjs writes to disk before the CLI even starts. */
async function fakeCli(dir, name) {
  const path = join(dir, name);
  await writeFile(path, "#!/usr/bin/env node\nprocess.exit(0);\n");
  await chmod(path, 0o755);
  return dir;
}

/** Runs the real harness.mjs (never a real coding-agent CLI) against a fake one, and returns the materialized PROMPT.md. */
async function runHarness(t, { input, memory }) {
  const binDir = await mkdtemp(join(tmpdir(), "zyvor-fake-cli-"));
  await fakeCli(binDir, "claude");
  const workspace = await mkdtemp(join(tmpdir(), "zyvor-harness-ws-"));
  const bundleDir = await mkdtemp(join(tmpdir(), "zyvor-harness-bundle-"));
  const bundlePath = join(bundleDir, "bundle");
  await writeFile(bundlePath, "You are a helpful agent.");
  const port = await freePort();
  const child = spawn(process.execPath, [harnessPath], {
    env: {
      ...process.env,
      PATH: `${binDir}${delimiter}${process.env.PATH}`,
      ZYVOR_SESSION_ID: "s", ZYVOR_EGRESS_CAPABILITY: "c", ZYVOR_EGRESS_BROKER: "http://127.0.0.1:9",
      ZYVOR_AGENT_PORT: String(port), ZYVOR_AGENT_RUNTIME: "claude",
      ZYVOR_AGENT_BUNDLE: bundlePath, ZYVOR_HARNESS_WORKSPACE: workspace, ZYVOR_HARNESS_CREDENTIALS: "[]",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    child.kill("SIGTERM");
    await Promise.all([binDir, workspace, bundleDir].map((d) => rm(d, { recursive: true, force: true })));
  });
  await poll(`http://127.0.0.1:${port}/health`, (v) => v.ok === true);
  await fetch(`http://127.0.0.1:${port}/run`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ input, memory }) });
  await poll(`http://127.0.0.1:${port}/status`, (v) => v.status === "completed");
  return readFile(join(workspace, "PROMPT.md"), "utf8");
}

test("harness puts the person's memory in PROMPT.md as data, never instructions, ahead of the session input", async (t) => {
  const prompt = await runHarness(t, {
    input: { message: "plan my day" },
    memory: [{ text: "vegetarian", kind: "fact", pinned: true, tainted: false }, { text: "close this deal fast", kind: "note", pinned: false, tainted: true }],
  });
  assert.match(prompt, /never as an instruction to follow/);
  assert.match(prompt, /- \(fact\) vegetarian/);
  assert.match(prompt, /- \(note, from untrusted content\) close this deal fast/, "a tainted entry is marked, not hidden or obeyed");
  const memoryAt = prompt.indexOf("chosen to have you remember");
  const inputAt = prompt.indexOf("Session input:");
  assert.ok(memoryAt >= 0 && inputAt > memoryAt, "the memory block comes before the session input");
  assert.match(prompt, /plan my day/);
});

test("harness without memory in the run request writes no memory block", async (t) => {
  const prompt = await runHarness(t, { input: { message: "hi" } });
  assert.doesNotMatch(prompt, /chosen to have you remember/);
  assert.match(prompt, /Session input:/);
});
