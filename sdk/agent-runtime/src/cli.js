#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { readFile, writeFile } from "node:fs/promises";
import { basename, extname, resolve } from "node:path";
import { buildManifest } from "./manifest.js";
import { buildBundle } from "./bundle.js";
import { runPackCommand } from "./pack-cli.js";

function usage(exitCode = 0) {
  console.log(`fabric-agent deploy <agent.ts> --name <name> --template <fluxvm-template> [options]
fabric-agent build <agent.ts> [--out <file>]
fabric-agent pack deploy <dir> [--run] [--test] [--dry-run]   deploy a pack.json use case or agent
fabric-agent pack bundle <dir> [--out <file>]                 write a signed <name>.merionpack.json for the console
fabric-agent pack keys                                        print the signer public key for MERION_POLICY_SEED

Options (deploy):
  --allow-host <host>       repeatable egress allow host
  --credential <name>       repeatable host-side credential grant
  --allow-private-network  permit brokered private/link-local destinations
  --runtime <kind>          node (default), claude, codex, or gemini
  --runtime-port <port>     guest worker port (default 8080)
  --ttl <seconds>           default session TTL
  --max-concurrency <n>     cap non-terminal sessions for this agent
  --idle-hibernate <sec>    hibernate only while blocked in ctx.nextSteer()
  --warm-pool <n>           keep n single-use sandboxes prewarmed
  --egress-mode <mode>      deny (default), ask, or sentinel for hosts off the allowlist
  --egress-approval-timeout <sec>  how long ask/sentinel wait for a decision (5-240)
  --home-volume             persistent /home/agent volume (QEMU template; one session at a time)
  --home-volume-name <name> volume name (default: the agent name)
  --home-path <path>        where the volume mounts in the guest (default /home/agent)
  --per-user-home           one volume per session user_id, so one agent serves many users
  --vcpus <n> --memory-mib <n>  sandbox size (give both)
  --confinement <mode>      strict drops all sandbox traffic except to the egress broker/proxy
  --confidential <mode>     auto (use a hardware-encrypted VM if the host has one), or required
  --inner-container <mode>  strict runs the worker unprivileged in a bubblewrap container
  --persistent              allow always-on per-user workstations for this agent
  --browser-port <port>     Chromium remote-debugging port in the guest (read-only tab listing)
  --dlp                     hold requests carrying key/token-shaped strings for approval
  --taint                   taint the session when it reads from an untrusted host
  --trust-host <host>       repeatable host that does not taint (implies --taint)
  --rule <spec>             repeatable egress rule host[:METHODS[:PATHS[:MAXBYTES]]]
  --skill <name[@version]>  repeatable skill to mount
  --skill-scope <scope>     which scoped skills this agent may mount
  --url <url>               Fabric Agent Runtime URL
  --token <token>           Fabric Agent Runtime bearer token

Options (build):
  --out <file>              output path for the bundled .mjs file (default: <entry>.bundle.mjs)

"build" runs the same esbuild bundling step as "deploy" but writes the
bundle to disk instead of deploying it, so it can be uploaded through the
Fabric web console's Deploy agent dialog without running agent-runtime
locally.`);
  process.exit(exitCode);
}

const args = process.argv.slice(2);
const command = args[0];
if (command === "pack") {
  await runPackCommand(args.slice(1));
  process.exit(0);
}
if ((command !== "deploy" && command !== "build") || !args[1]) usage(1);
const entry = resolve(args[1]);
const flags = parseFlags(args.slice(2));

if (command === "deploy") {
  if (!flags.name || !flags.template) usage(1);
  const runtime = flags.runtime || "node";
  if (!["node", "claude", "codex", "gemini"].includes(runtime)) {
    console.error("runtime must be node, claude, codex, or gemini");
    process.exit(1);
  }
  let manifest;
  try {
    manifest = buildManifest(flags, runtime);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
  const bundle = runtime === "node" ? (await buildBundle(entry)).bundle : await readFile(entry);
  const baseUrl = (flags.url || process.env.FABRIC_AGENT_URL || "http://127.0.0.1:9096").replace(/\/$/, "");
  const token = flags.token || process.env.FABRIC_AGENT_TOKEN;
  const response = await fetch(`${baseUrl}/v1/agents`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify({
      name: flags.name,
      bundle_base64: Buffer.from(bundle).toString("base64"),
      manifest,
    }),
  });
  const result = await response.json().catch(() => ({}));
  if (!response.ok) {
    console.error(result.error || `deploy failed with HTTP ${response.status}`);
    process.exit(1);
  }
  console.log(`Deployed ${result.name}@${result.version} (${basename(entry)})`);
  console.log(`sha256:${result.digest_sha256}`);
} else {
  const { bundle } = await buildBundle(entry);
  const ext = extname(entry);
  const defaultOut = `${basename(entry, ext)}.bundle.mjs`;
  const outPath = resolve(flags.out || defaultOut);
  await writeFile(outPath, bundle);
  console.log(`Built ${outPath} (${bundle.length} bytes)`);
}

function parseFlags(argv) {
  const out = { credential: [], allowHost: [], skill: [], taintTrust: [], rule: [], allowPrivateNetwork: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--allow-private-network") {
      out.allowPrivateNetwork = true;
      continue;
    }
    if (arg === "--home-volume") {
      out.homeVolume = true;
      continue;
    }
    if (arg === "--persistent") {
      out.persistent = true;
      continue;
    }
    if (arg === "--dlp") {
      out.dlp = true;
      continue;
    }
    if (arg === "--taint") {
      out.taint = true;
      continue;
    }
    if (arg === "--per-user-home") {
      out.perUserHome = true;
      continue;
    }
    const value = argv[i + 1];
    if (!arg.startsWith("--") || value === undefined || value.startsWith("--")) usage(1);
    i += 1;
    switch (arg) {
      case "--name": out.name = value; break;
      case "--template": out.template = value; break;
      case "--runtime": out.runtime = value; break;
      case "--credential": out.credential.push(value); break;
      case "--allow-host": out.allowHost.push(value); break;
      case "--runtime-port": out.runtimePort = value; break;
      case "--ttl": out.ttl = value; break;
      case "--max-concurrency": out.maxConcurrency = value; break;
      case "--idle-hibernate": out.idleHibernate = value; break;
      case "--warm-pool": out.warmPool = value; break;
      case "--egress-mode": out.egressMode = value; break;
      case "--egress-approval-timeout": out.egressApprovalTimeout = value; break;
      case "--home-volume-name": out.homeVolumeName = value; break;
      case "--home-path": out.homePath = value; break;
      case "--vcpus": out.vcpus = value; break;
      case "--memory-mib": out.memoryMib = value; break;
      case "--confinement": out.confinement = value; break;
      case "--confidential": out.confidential = value; break;
      case "--inner-container": out.innerContainer = value; break;
      case "--browser-port": out.browserPort = value; break;
      case "--trust-host": out.taintTrust.push(value); break;
      case "--rule": out.rule.push(value); break;
      case "--skill": out.skill.push(value); break;
      case "--skill-scope": out.skillScope = value; break;
      case "--url": out.url = value; break;
      case "--token": out.token = value; break;
      case "--out": out.out = value; break;
      default: console.error(`unknown option: ${arg}`); usage(1);
    }
  }
  return out;
}
