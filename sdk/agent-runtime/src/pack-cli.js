// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { bundlePack, deployPack } from "./pack.js";
import { publicKeyHex } from "./sign.js";

function flagsOf(argv) {
  const out = { _: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (["--run", "--test", "--dry-run"].includes(a)) out[a.slice(2)] = true;
    else if (["--url", "--token", "--out", "--seed-env"].includes(a)) out[a.slice(2)] = argv[++i];
    else if (a.startsWith("--")) throw new Error(`unknown option: ${a}`);
    else out._.push(a);
  }
  return out;
}

/** `fabric-agent pack <deploy|bundle|keys> ...` */
export async function runPackCommand(argv) {
  const [sub, ...rest] = argv;
  try {
    const f = flagsOf(rest);
    const seedVar = f["seed-env"] ?? "MERION_POLICY_SEED";
    // KEEP_POLICY_SEED is the pre-rename name; it is still read when the default variable is in use.
    const seed =
      process.env[seedVar] ||
      (f["seed-env"] ? undefined : process.env.KEEP_POLICY_SEED) ||
      undefined;
    const url = f.url || process.env.FABRIC_AGENT_URL || (process.env.MERION_API ?? process.env.KEEP_API) || "http://127.0.0.1:9096";
    const token = f.token || process.env.FABRIC_AGENT_TOKEN || (process.env.MERION_TOKEN ?? process.env.KEEP_TOKEN);

    if (sub === "keys") {
      if (!seed) throw new Error(`set ${seedVar} to a 32-byte hex seed (openssl rand -hex 32)`);
      console.log(publicKeyHex(seed));
      return;
    }
    const dir = f._[0];
    if (!dir) throw new Error("usage: fabric-agent pack <deploy|bundle> <dir>");

    if (sub === "deploy") {
      const r = await deployPack(dir, { url, token, seed, run: f.run, test: f.test, dryRun: f["dry-run"] });
      for (const s of r.steps) console.log(`${r.dryRun ? "would " : "✓ "}${s}`);
      if (r.test) console.log(`✓ test passed: ${r.test.artifacts.join(", ")}, 0 CONNECT`);
      if (r.cockpit) console.log(`Open ${r.cockpit} in the console`);
      if (r.kind === "agent" && !r.signed && !r.dryRun) {
        console.log(`note: unsigned. Set ${seedVar} if the runtime is in Keep mode.`);
      }
      return;
    }
    if (sub === "bundle") {
      const envelope = await bundlePack(dir, { seed });
      const out = resolve(f.out || `${envelope.name}.keeppack.json`);
      await writeFile(out, JSON.stringify(envelope, null, 2));
      console.log(`Wrote ${out}${envelope.signature ? " (signed)" : " (unsigned)"}`);
      console.log('Upload it in the console: Keep → "Deploy a pack".');
      return;
    }
    throw new Error("usage: fabric-agent pack <deploy|bundle|keys> ...");
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
