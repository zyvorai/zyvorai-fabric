// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { buildBundle } from "./bundle.js";
import { signBytes } from "./sign.js";

/** Fields of a pack.json that describe the pack, not the use-case spec itself. */
const PACK_ONLY = new Set(["kind", "name", "manifest", "goal", "entry", "sample_file", "$schema"]);

async function readIfExists(path) {
  try {
    return await readFile(path, "utf8");
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }
}

/**
 * Read `<dir>/pack.json`. A pack is either a declarative `usecase` or a
 * TypeScript `agent`. Older packs (name + manifest + goal, no `kind`) are agents.
 * A `builtin` pack documents a use case the runtime already ships.
 */
export async function loadPack(dir) {
  const root = resolve(dir);
  const raw = await readIfExists(join(root, "pack.json"));
  if (raw === null) throw new Error(`no pack.json in ${root}`);
  let pack;
  try {
    pack = JSON.parse(raw);
  } catch (error) {
    throw new Error(`pack.json is not valid JSON: ${error.message}`);
  }
  if (typeof pack !== "object" || pack === null || Array.isArray(pack)) {
    throw new Error("pack.json must be a JSON object");
  }
  const kind = pack.kind ?? "agent";
  if (!["agent", "usecase", "builtin"].includes(kind)) {
    throw new Error(`pack.json kind must be "usecase", "agent" or "builtin", got ${JSON.stringify(kind)}`);
  }
  const name = pack.name ?? pack.id;
  if (typeof name !== "string" || !/^[a-z0-9][a-z0-9-]{0,39}$/.test(name)) {
    throw new Error('pack.json needs a "name" of 1-40 lowercase letters, digits or "-"');
  }
  return { root, pack, kind, name };
}

/** The use-case spec POSTed to /v1/demos: the pack minus pack-only fields. */
export async function useCaseSpec({ root, pack, name }) {
  const spec = { id: name };
  for (const [key, value] of Object.entries(pack)) {
    if (!PACK_ONLY.has(key)) spec[key] = value;
  }
  spec.id = name;
  if (pack.sample_file) {
    const text = await readFile(join(root, pack.sample_file), "utf8");
    spec.sample = { filename: pack.sample_file.split("/").pop(), text };
  }
  return spec;
}

/**
 * The exact deploy request body for an agent pack. It is serialised once and
 * these bytes are both signed and sent, so the signature always matches.
 */
export async function composeAgentDeploy({ root, pack, name }, bundle) {
  if (typeof pack.manifest !== "object" || pack.manifest === null) {
    throw new Error('an agent pack needs a "manifest" object (template, egress, confinement, ...)');
  }
  if (!pack.manifest.template) throw new Error('pack manifest needs a "template"');
  const body = JSON.stringify({
    name,
    bundle_base64: Buffer.from(bundle).toString("base64"),
    manifest: pack.manifest,
  });
  return body;
}

async function agentBundle(info) {
  const entry = info.pack.entry ?? "agent.ts";
  const { bundle } = await buildBundle(join(info.root, entry));
  return bundle;
}

function goalFor(info) {
  const g = info.pack.goal;
  if (!g) return null;
  const text = g.text ?? g.description ?? "";
  return { title: g.title ?? info.name, description: text };
}

function sigHeader(name, signature) {
  return signature ? { [name]: signature } : {};
}

async function call(fetchImpl, method, url, { token, headers = {}, body } = {}) {
  const response = await fetchImpl(url, {
    method,
    headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...headers },
    body,
  });
  const text = await response.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    /* non-JSON body */
  }
  return { ok: response.ok, status: response.status, json, text };
}

const fail = (what, r) => {
  throw new Error(`${what}: HTTP ${r.status} ${r.json?.error ?? r.text ?? ""}`.trim());
};

/**
 * Deploy a pack to an agent-runtime. Returns a summary and never prints, so it
 * is testable with an injected `fetch`.
 */
export async function deployPack(dir, opts = {}) {
  const {
    url = "http://127.0.0.1:9096",
    token,
    seed,
    run = false,
    test = false,
    dryRun = false,
    fetchImpl = fetch,
  } = opts;
  const base = url.replace(/\/$/, "");
  const info = await loadPack(dir);
  const steps = [];

  if (info.kind === "builtin") {
    steps.push(`${info.name} is a built-in use case: nothing to deploy`);
    if (dryRun || !test) return { kind: "builtin", name: info.name, steps, dryRun };
    steps.push("test on its sample");
    const r = await call(fetchImpl, "POST", `${base}/v1/demos/${encodeURIComponent(info.name)}`, {
      token,
      body: new FormData(),
    });
    if (!r.ok) fail("testing the use case", r);
    if (r.json?.egress_connects !== 0) {
      throw new Error(`test failed closed: egress_connects=${r.json?.egress_connects}`);
    }
    return {
      kind: "builtin",
      name: info.name,
      steps,
      test: {
        egress_connects: 0,
        artifacts: (r.json.artifacts ?? []).map((a) => a.title),
        session_id: r.json.session_id,
      },
    };
  }

  if (info.kind === "usecase") {
    const spec = await useCaseSpec(info);
    steps.push(`save use case ${spec.id}`);
    if (dryRun) return { kind: "usecase", name: info.name, steps, spec, dryRun: true };
    const saved = await call(fetchImpl, "POST", `${base}/v1/demos`, {
      token,
      headers: { "content-type": "application/json" },
      body: JSON.stringify(spec),
    });
    if (!saved.ok) fail("saving the use case", saved);
    const summary = { kind: "usecase", name: info.name, steps, saved: saved.json };
    if (test) {
      steps.push("test on its sample");
      const form = new FormData();
      const r = await call(fetchImpl, "POST", `${base}/v1/demos/${encodeURIComponent(info.name)}`, {
        token,
        body: form,
      });
      if (!r.ok) fail("testing the use case", r);
      if (r.json?.egress_connects !== 0) {
        throw new Error(`test failed closed: egress_connects=${r.json?.egress_connects}`);
      }
      summary.test = {
        egress_connects: 0,
        artifacts: (r.json.artifacts ?? []).map((a) => a.title),
        session_id: r.json.session_id,
      };
    }
    return summary;
  }

  const bundle = await agentBundle(info);
  const body = await composeAgentDeploy(info, bundle);
  const signature = seed ? signBytes(Buffer.from(body), seed) : null;
  const policy = await readIfExists(join(info.root, "merion.policy.yaml"));
  const policySignature = seed && policy ? signBytes(Buffer.from(policy), seed) : null;
  steps.push(`deploy agent ${info.name}${signature ? " (signed)" : " (unsigned)"}`);
  if (policy) steps.push(`apply merion.policy.yaml${policySignature ? " (signed)" : ""}`);
  if (run) steps.push("start a session and goal");
  if (dryRun) {
    return { kind: "agent", name: info.name, steps, signed: Boolean(signature), dryRun: true, bytes: body.length };
  }

  const deployed = await call(fetchImpl, "POST", `${base}/v1/agents`, {
    token,
    headers: { "content-type": "application/json", ...sigHeader("x-merion-manifest-signature", signature) },
    body,
  });
  if (!deployed.ok) fail("deploying the agent", deployed);
  const summary = { kind: "agent", name: info.name, steps, deployed: deployed.json, signed: Boolean(signature) };

  if (policy) {
    const put = await call(fetchImpl, "PUT", `${base}/v1/agents/${encodeURIComponent(info.name)}/policy`, {
      token,
      headers: { "content-type": "application/yaml", ...sigHeader("x-merion-policy-signature", policySignature) },
      body: policy,
    });
    if (!put.ok) fail("applying merion.policy.yaml", put);
    summary.policy = "applied";
  }

  if (run) {
    const session = await call(fetchImpl, "POST", `${base}/v1/sessions`, {
      token,
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ agent: info.name, input: {} }),
    });
    if (!session.ok) fail("starting a session", session);
    summary.session_id = session.json?.id ?? session.json?.session_id;
    const goal = goalFor(info);
    if (goal && summary.session_id) {
      const g = await call(fetchImpl, "POST", `${base}/v1/goals`, {
        token,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ ...goal, agent: info.name, session_id: summary.session_id }),
      });
      if (!g.ok) fail("creating the goal", g);
      summary.goal_id = g.json?.id;
    }
    summary.cockpit = summary.session_id ? `/app/merion/${summary.session_id}` : undefined;
  }
  return summary;
}

/**
 * Build the file the console's "Deploy a pack" drop zone takes. It carries the
 * exact signed deploy bytes as a string, so nothing is re-serialised in transit.
 * The signer seed stays on this machine: the console only uploads the result.
 */
export async function bundlePack(dir, opts = {}) {
  const { seed } = opts;
  const info = await loadPack(dir);
  if (info.kind !== "agent") {
    throw new Error("only agent packs are bundled; deploy a use case from the console form or `pack deploy`");
  }
  const bundle = await agentBundle(info);
  const deploy = await composeAgentDeploy(info, bundle);
  const policy = await readIfExists(join(info.root, "merion.policy.yaml"));
  return {
    format: "merionpack/1",
    name: info.name,
    deploy_json: deploy,
    signature: seed ? signBytes(Buffer.from(deploy), seed) : null,
    policy,
    policy_signature: seed && policy ? signBytes(Buffer.from(policy), seed) : null,
    goal: goalFor(info),
  };
}
