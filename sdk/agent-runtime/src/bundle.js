// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { build } from "esbuild";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Bundle an agent entry point into one ESM file (the same step `deploy` and `build` use). */
export async function buildBundle(entryPath) {
  await readFile(entryPath); // fail with a clear local path error before invoking esbuild
  const output = await build({
    entryPoints: [entryPath],
    bundle: true,
    write: false,
    platform: "node",
    format: "esm",
    target: "node20",
    sourcemap: "inline",
    legalComments: "inline",
    // Resolve the SDK by its published package name against this local
    // checkout's own source -- the deployed bundle only ever needs the
    // ctx helpers inlined, and requiring a real npm publish before anyone
    // can deploy an agent would make the documented workflow unusable.
    alias: { "@zyvor/fabric-agent": resolve(dirname(fileURLToPath(import.meta.url)), "index.js") },
  });
  return { entry: entryPath, bundle: output.outputFiles[0].contents };
}
