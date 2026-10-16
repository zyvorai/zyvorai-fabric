#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { readFile, open, link, unlink, stat } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { randomUUID } from "node:crypto";
import { pathToFileURL } from "node:url";
import { Fabric } from "./index.js";
import { collectEvidence, verifyEvidence } from "./evidence.js";

const HELP = `Usage:
  keep-evidence collect --session UUID --out FILE [--url URL]
  keep-evidence verify FILE

Collect requires MERION_API_TOKEN and MERION_EXPORT_TOKEN in the environment.
The output is sensitive and is created with mode 0600. Existing files are never overwritten.
Verification checks local checksum and references, not authenticity or completeness.`;

function options(args) {
  const opts = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i];
    if (!key?.startsWith("--") || !args[i + 1] || args[i + 1].startsWith("--") || key in opts) {
      throw new Error("invalid or duplicate option");
    }
    opts[key] = args[i + 1];
  }
  for (const key of Object.keys(opts)) {
    if (!["--session", "--out", "--url"].includes(key)) throw new Error(`unknown option ${key}`);
  }
  return opts;
}

/** Create a private file without following or replacing an existing destination. */
export async function savePrivate(path, contents) {
  const temp = join(dirname(path), `.${basename(path)}.${randomUUID()}.tmp`);
  let file;
  try {
    file = await open(temp, "wx", 0o600);
    await file.writeFile(contents, "utf8");
    await file.sync();
    await file.close();
    file = undefined;
    await link(temp, path); // exclusive: EEXIST if an output (including a symlink) exists
  } finally {
    if (file) await file.close();
    await unlink(temp).catch((e) => { if (e.code !== "ENOENT") throw e; });
  }
}

export async function run(args, { env = process.env, write = (s) => process.stdout.write(s),
  clientFactory = (settings) => new Fabric(settings) } = {}) {
  const [command, ...rest] = args;
  if (command === "verify" && rest.length === 1) {
    if ((await stat(rest[0])).size > 64 * 1024 * 1024) throw new Error("bundle exceeds 64 MiB");
    const bundle = JSON.parse(await readFile(rest[0], "utf8"));
    const result = verifyEvidence(bundle);
    write(`${JSON.stringify(result)}\n`);
    return result.ok ? 0 : 1;
  }
  if (command === "collect") {
    const opts = options(rest);
    if (!opts["--session"] || !opts["--out"]) throw new Error("--session and --out are required");
    if (!(env.MERION_API_TOKEN ?? env.KEEP_API_TOKEN) || !(env.MERION_EXPORT_TOKEN ?? env.KEEP_EXPORT_TOKEN)) {
      throw new Error("MERION_API_TOKEN and MERION_EXPORT_TOKEN are required");
    }
    const client = clientFactory({ baseUrl: opts["--url"] ?? "http://127.0.0.1:9096", token: (env.MERION_API_TOKEN ?? env.KEEP_API_TOKEN) });
    const bundle = await collectEvidence(client, { sessionId: opts["--session"], exportToken: (env.MERION_EXPORT_TOKEN ?? env.KEEP_EXPORT_TOKEN) });
    await savePrivate(opts["--out"], `${JSON.stringify(bundle, null, 2)}\n`);
    write(`Saved ${opts["--out"]}; audit window saturated: ${bundle.payload.source.audit_window_saturated}; ` +
      `receipt window saturated: ${bundle.payload.source.receipt_window_saturated}\n`);
    return 0;
  }
  throw new Error(HELP);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  run(process.argv.slice(2)).then((code) => { process.exitCode = code; }).catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
