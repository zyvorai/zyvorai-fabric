// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * Customer deployment pack — readiness / health probe + FABRIC_DOCTOR checklist.
 * Does not run privileged install on the host; proposes commands in the artifact.
 */
import { defineAgent } from "@zyvor/fabric-agent";
import { FabricClient, fabricBaseFromInput } from "../_fabric/client.js";

export type DeployOpInput = {
  fabricBase?: string;
};

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input || {}) as DeployOpInput;
    const fabric = new FabricClient({
      baseUrl: fabricBaseFromInput(input as Record<string, unknown>),
      fetch: ctx.fetch.bind(ctx),
    });

    ctx.emit("deploy-op.started", { fabricBase: fabric.baseUrl });

    const [readyz, health] = await Promise.all([
      fabric.get("/readyz"),
      fabric.get("/health"),
    ]);

    const report = [
      "# Deployment readiness report",
      "",
      `Generated: ${new Date().toISOString()}`,
      `fabricBase: ${fabric.baseUrl}`,
      "",
      "## GET /readyz",
      "```json",
      JSON.stringify(readyz.json ?? { status: readyz.status, body: readyz.text.slice(0, 2000) }, null, 2),
      "```",
      "",
      "## GET /health",
      "```json",
      JSON.stringify(health.json ?? { status: health.status, body: health.text.slice(0, 2000) }, null, 2),
      "```",
      "",
      "## Prerequisites checklist (FABRIC_DOCTOR)",
      "Map findings to [docs/FABRIC_DOCTOR.md](../../../docs/FABRIC_DOCTOR.md):",
      "",
      "- [ ] KVM / nested virt available on target hosts",
      "- [ ] FluxVM listening and `/readyz` green",
      "- [ ] fabricd store + auth configured",
      "- [ ] TLS / reverse proxy for console",
      "- [ ] Operator ran privileged install (agent does **not** install)",
      "",
      "## Proposed host commands (operator runs these)",
      "```bash",
      "# Example — adjust paths for your lab",
      "./scripts/fabric-doctor.sh   # if present on the host",
      "curl -sk https://127.0.0.1:9095/readyz | jq .",
      "curl -sk https://127.0.0.1:9095/health | jq .",
      "```",
      "",
      "No mutating fabricd config toggles by default.",
    ].join("\n");

    ctx.emit("deploy-op.artifact", {
      kind: "readiness-report",
      title: "Customer deploy readiness",
      body: report,
      readyz_ok: readyz.ok,
      health_ok: health.ok,
    });

    return {
      readiness_markdown: report,
      readyz_ok: readyz.ok,
      health_ok: health.ok,
      writes: "none by default",
    };
  },
});
