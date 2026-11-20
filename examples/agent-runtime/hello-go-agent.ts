// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { defineAgent } from "@zyvor/fabric-agent";
import { execFile } from "node:child_process";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

// The program this agent writes. Returned verbatim so a caller can see the
// source that was compiled, not a paraphrase of it.
const source = `package main

import "fmt"

func main() {
	fmt.Println("hello")
}
`;

// Credential-free coding agent. It writes a Go file inside the guest, runs
// it with the template's Go toolchain, and returns both the source and the
// program's output. No model key and no egress grant are required.
export default defineAgent({
  async run(ctx) {
    ctx.emit("hello.started", {});
    const dir = await mkdtemp(join(tmpdir(), "zyvor-hello-"));
    const file = join(dir, "hello.go");
    await writeFile(file, source, "utf8");

    let stdout = "";
    let stderr = "";
    try {
      const result = await execFileAsync("go", ["run", file], { timeout: 45_000 });
      stdout = result.stdout;
      stderr = result.stderr;
    } catch (error) {
      const failed = error as { stdout?: string; stderr?: string; message?: string };
      ctx.emit("hello.failed", {
        error: failed.message || String(error),
        stderr: failed.stderr || "",
      });
      throw error;
    }

    const report = {
      language: "go",
      path: file,
      source,
      stdout,
      stderr,
    };
    ctx.emit("hello.completed", report);
    return report;
  },
});
