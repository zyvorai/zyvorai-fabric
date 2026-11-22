// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { defineAgent } from "@zyvor/fabric-agent";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { statfsSync } from "node:fs";
import os from "node:os";

const execFileAsync = promisify(execFile);

// In-guest health check with no external network access: everything here
// runs as a plain Node.js process inside the FluxVM sandbox, so it needs no
// `credential` grant and no `egress_allow_hosts` entry in the deploy
// manifest -- a useful contrast with agent.ts's LLM-calling example.
export default defineAgent({
  async run(ctx) {
    ctx.emit("healthcheck.started", {});

    const disk = statfsSync("/");
    const diskFreeBytes = disk.bfree * disk.bsize;
    const diskTotalBytes = disk.blocks * disk.bsize;

    const memFreeBytes = os.freemem();
    const memTotalBytes = os.totalmem();

    let uptimeOutput = "";
    try {
      const { stdout } = await execFileAsync("uptime", []);
      uptimeOutput = stdout.trim();
    } catch (error) {
      ctx.emit("healthcheck.warning", { step: "uptime", error: String(error) });
    }

    const report = {
      disk: {
        free_bytes: diskFreeBytes,
        total_bytes: diskTotalBytes,
        used_pct: Math.round((1 - diskFreeBytes / diskTotalBytes) * 100),
      },
      memory: {
        free_bytes: memFreeBytes,
        total_bytes: memTotalBytes,
        used_pct: Math.round((1 - memFreeBytes / memTotalBytes) * 100),
      },
      load_avg: os.loadavg(),
      uptime: uptimeOutput,
      checked_at: new Date().toISOString(),
    };

    ctx.emit("healthcheck.completed", report);
    return report;
  },
});
