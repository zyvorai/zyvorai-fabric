// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * Infrastructure operations pack — Fabric-facing (not in-guest healthcheck).
 * Reads alerts / VMs / lifecycle; proposes restarts behind approval.
 */
import { defineAgent } from "@zyvor/fabric-agent";
import { FabricClient, fabricBaseFromInput } from "../_fabric/client.js";

export type InfraOpsInput = {
  fabricBase?: string;
  /** When set, propose restart of this VM (POST requires approval). */
  restartVm?: string;
  /** Optional remediation payload for POST /api/lifecycle/remediations. */
  remediation?: { host_id: string; hostname: string; baseline_id: string };
};

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input || {}) as InfraOpsInput;
    const fabric = new FabricClient({
      baseUrl: fabricBaseFromInput(input as Record<string, unknown>),
      fetch: ctx.fetch.bind(ctx),
    });

    ctx.emit("infra-ops.started", { fabricBase: fabric.baseUrl });

    const [alerts, vms, compliance] = await Promise.all([
      fabric.get("/api/system/alerts"),
      fabric.get("/api/vms"),
      fabric.get("/api/lifecycle/compliance"),
    ]);

    const timeline = [
      "# Incident timeline",
      "",
      `Checked at: ${new Date().toISOString()}`,
      "",
      "## System alerts",
      "```json",
      JSON.stringify(alerts.json ?? { status: alerts.status, body: alerts.text.slice(0, 2000) }, null, 2),
      "```",
      "",
      "## VMs",
      "```json",
      JSON.stringify(vms.json ?? { status: vms.status }, null, 2).slice(0, 8000),
      "```",
      "",
      "## Lifecycle compliance",
      "```json",
      JSON.stringify(compliance.json ?? { status: compliance.status }, null, 2).slice(0, 4000),
      "```",
    ].join("\n");

    const proposedFix: Record<string, unknown> = {
      reads_ok: alerts.ok && vms.ok,
      restartVm: input.restartVm || null,
      remediation: input.remediation || null,
      note: "Mutating Fabric calls require host approval (fabric-api requires_approval + Keep ask).",
    };

    ctx.emit("infra-ops.artifact", {
      kind: "incident-timeline",
      title: "Infra ops timeline",
      body: timeline,
      proposed_fix: proposedFix,
    });

    let mutate: unknown = null;
    if (input.restartVm) {
      ctx.emit("infra-ops.mutate.planned", { action: "restart", vm: input.restartVm });
      const res = await fabric.post(`/api/vms/${encodeURIComponent(input.restartVm)}/restart`);
      mutate = { action: "restart", status: res.status, ok: res.ok, body: res.json ?? res.text };
      ctx.emit("infra-ops.mutate.done", mutate);
    } else if (input.remediation) {
      ctx.emit("infra-ops.mutate.planned", { action: "remediation", ...input.remediation });
      const res = await fabric.post("/api/lifecycle/remediations", input.remediation);
      mutate = { action: "remediation", status: res.status, ok: res.ok, body: res.json ?? res.text };
      ctx.emit("infra-ops.mutate.done", mutate);
    }

    return {
      timeline_markdown: timeline,
      proposed_fix: proposedFix,
      mutate,
      evidence: "Post timeline via POST /v1/artifacts; advance goal step for approval gating.",
    };
  },
});
