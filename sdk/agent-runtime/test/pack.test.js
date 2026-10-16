// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { createPublicKey, verify } from "node:crypto";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { bundlePack, deployPack, loadPack, useCaseSpec } from "../src/pack.js";
import { publicKeyHex, seedFromHex, signBytes } from "../src/sign.js";

const SEED = "07".repeat(32);

async function packDir(files) {
  const dir = await mkdtemp(join(tmpdir(), "merionpack-"));
  for (const [name, content] of Object.entries(files)) {
    await mkdir(join(dir, name, ".."), { recursive: true });
    await writeFile(join(dir, name), content);
  }
  return dir;
}

/** A fetch that records calls and answers from a small routing table. */
function fakeFetch(routes) {
  const calls = [];
  const impl = async (url, init) => {
    calls.push({ url, method: init.method, headers: init.headers, body: init.body });
    const key = `${init.method} ${new URL(url).pathname}`;
    const answer = routes[key] ?? { status: 404, json: { error: `no route ${key}` } };
    return { ok: answer.status < 400, status: answer.status, text: async () => JSON.stringify(answer.json ?? {}) };
  };
  return { impl, calls };
}

const AGENT_PACK = {
  "pack.json": JSON.stringify({
    name: "my-agent",
    manifest: { template: "agent-node", egress_mode: "deny", confinement: "strict" },
    goal: { title: "Do it", text: "Do the thing." },
  }),
  "agent.ts": "export default { name: 'my-agent' };\n",
  "merion.policy.yaml": "version: 1\ndefault_egress: deny\n",
};

test("the Node signer matches the Rust one (same fixture as policy.rs)", () => {
  const sig = signBytes(Buffer.from("version: 1\ndefault_egress: deny\n"), SEED);
  assert.equal(
    sig,
    "5665d748703e5d6db9c7a2b7f9bf1623c33dc4d77767527ea4a4b530a01e36429a30f5a0b6998e3ca286cab2f1103f2f4a5540add46c04fa3d82686748492508",
  );
  assert.equal(publicKeyHex(SEED), "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c");
  assert.throws(() => seedFromHex("nope"), /64 hex/);
});

test("loadPack defaults to an agent pack and rejects bad packs", async () => {
  const dir = await packDir(AGENT_PACK);
  const info = await loadPack(dir);
  assert.equal(info.kind, "agent");
  assert.equal(info.name, "my-agent");
  await assert.rejects(loadPack(await packDir({})), /no pack.json/);
  await assert.rejects(loadPack(await packDir({ "pack.json": "{" })), /not valid JSON/);
  await assert.rejects(loadPack(await packDir({ "pack.json": '{"name":"Bad Name"}' })), /"name"/);
  await assert.rejects(loadPack(await packDir({ "pack.json": '{"name":"x","kind":"shell"}' })), /kind/);
});

test("a use-case spec drops pack-only fields and inlines the sample file", async () => {
  const dir = await packDir({
    "pack.json": JSON.stringify({
      kind: "usecase",
      name: "invoice-check",
      title: "Invoice check",
      accepts: ["txt"],
      extract: "text",
      summary: [{ kind: "stats" }],
      sample_file: "sample.txt",
    }),
    "sample.txt": "Total: 10\n",
  });
  const spec = await useCaseSpec(await loadPack(dir));
  assert.deepEqual(Object.keys(spec).sort(), ["accepts", "extract", "id", "sample", "summary", "title"]);
  assert.deepEqual(spec.sample, { filename: "sample.txt", text: "Total: 10\n" });
});

test("deploying a use case saves the spec and --test requires 0 CONNECT", async () => {
  const dir = await packDir({
    "pack.json": JSON.stringify({
      kind: "usecase", name: "uc", title: "UC", accepts: ["txt"], extract: "text", summary: [{ kind: "stats" }],
    }),
  });
  const ok = fakeFetch({
    "POST /v1/demos": { status: 201, json: { id: "uc" } },
    "POST /v1/demos/uc": { status: 201, json: { egress_connects: 0, session_id: "s1", artifacts: [{ title: "summary.md" }] } },
  });
  const r = await deployPack(dir, { url: "http://rt", token: "t", test: true, fetchImpl: ok.impl });
  assert.deepEqual(r.test, { egress_connects: 0, artifacts: ["summary.md"], session_id: "s1" });
  assert.equal(ok.calls[0].headers.authorization, "Bearer t");
  assert.equal(JSON.parse(ok.calls[0].body).id, "uc");

  const bad = fakeFetch({
    "POST /v1/demos": { status: 201, json: {} },
    "POST /v1/demos/uc": { status: 201, json: { egress_connects: 3 } },
  });
  await assert.rejects(deployPack(dir, { test: true, fetchImpl: bad.impl }), /egress_connects=3/);

  const rejected = fakeFetch({ "POST /v1/demos": { status: 400, json: { error: "bad spec" } } });
  await assert.rejects(deployPack(dir, { fetchImpl: rejected.impl }), /bad spec/);
});

test("an agent pack is signed over the exact bytes it sends", async () => {
  const dir = await packDir(AGENT_PACK);
  const f = fakeFetch({
    "POST /v1/agents": { status: 201, json: { name: "my-agent", version: "v1" } },
    "PUT /v1/agents/my-agent/policy": { status: 200, json: {} },
    "POST /v1/sessions": { status: 201, json: { id: "sess-1" } },
    "POST /v1/goals": { status: 201, json: { id: "goal-1" } },
  });
  const r = await deployPack(dir, { url: "http://rt/", seed: SEED, run: true, fetchImpl: f.impl });
  const [deploy, policy, session, goal] = f.calls;

  // The header signature verifies against the exact body string that was sent.
  const pub = createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(publicKeyHex(SEED), "hex")]),
    format: "der",
    type: "spki",
  });
  assert.equal(
    verify(null, Buffer.from(deploy.body), pub, Buffer.from(deploy.headers["x-merion-manifest-signature"], "hex")),
    true,
  );
  assert.equal(JSON.parse(deploy.body).manifest.template, "agent-node");
  assert.equal(policy.headers["x-merion-policy-signature"], signBytes(Buffer.from(AGENT_PACK["merion.policy.yaml"]), SEED));
  assert.equal(JSON.parse(session.body).agent, "my-agent");
  assert.equal(JSON.parse(goal.body).session_id, "sess-1");
  assert.equal(r.cockpit, "/app/keep/sess-1");
  assert.equal(r.signed, true);
});

test("without a seed the deploy is unsigned, and --dry-run makes no calls", async () => {
  const dir = await packDir(AGENT_PACK);
  const f = fakeFetch({ "POST /v1/agents": { status: 201, json: {} }, "PUT /v1/agents/my-agent/policy": { status: 200 } });
  const r = await deployPack(dir, { fetchImpl: f.impl });
  assert.equal(r.signed, false);
  assert.equal(f.calls[0].headers["x-merion-manifest-signature"], undefined);

  const d = fakeFetch({});
  const dry = await deployPack(dir, { seed: SEED, dryRun: true, fetchImpl: d.impl });
  assert.equal(d.calls.length, 0);
  assert.equal(dry.dryRun, true);
  assert.match(dry.steps.join("\n"), /deploy agent my-agent \(signed\)/);
});

test("a manifest without a template is refused before anything is sent", async () => {
  const dir = await packDir({ ...AGENT_PACK, "pack.json": JSON.stringify({ name: "x", manifest: {} }) });
  const f = fakeFetch({});
  await assert.rejects(deployPack(dir, { fetchImpl: f.impl }), /template/);
  assert.equal(f.calls.length, 0);
});

test("bundlePack carries the exact signed bytes for the console upload", async () => {
  const dir = await packDir(AGENT_PACK);
  const env = await bundlePack(dir, { seed: SEED });
  assert.equal(env.format, "merionpack/1");
  assert.equal(env.signature, signBytes(Buffer.from(env.deploy_json), SEED));
  assert.equal(env.policy_signature, signBytes(Buffer.from(env.policy), SEED));
  assert.equal(JSON.parse(env.deploy_json).name, "my-agent");
  assert.deepEqual(env.goal, { title: "Do it", description: "Do the thing." });
  const unsigned = await bundlePack(dir, {});
  assert.equal(unsigned.signature, null);
  await assert.rejects(
    bundlePack(await packDir({ "pack.json": JSON.stringify({ kind: "usecase", name: "u" }) }), {}),
    /only agent packs/,
  );
});

test("a built-in pack has nothing to deploy but can be tested", async () => {
  const dir = await packDir({ "pack.json": JSON.stringify({ kind: "builtin", name: "csv-clean" }) });
  const f = fakeFetch({
    "POST /v1/demos/csv-clean": { status: 201, json: { egress_connects: 0, artifacts: [{ title: "clean.csv" }] } },
  });
  const plain = await deployPack(dir, { fetchImpl: f.impl });
  assert.match(plain.steps[0], /built-in use case: nothing to deploy/);
  assert.equal(f.calls.length, 0);
  const tested = await deployPack(dir, { test: true, fetchImpl: f.impl });
  assert.deepEqual(tested.test.artifacts, ["clean.csv"]);
});
