// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * Migration operator — Fabric /api/migrations + GuestKit inspect/rescue.
 * Transiva / hypersdk inventory is out-of-band until wired separately.
 */
import { defineAgent } from "@zyvor/fabric-agent";
import { FabricClient, fabricBaseFromInput } from "../_fabric/client.js";

export type MigrationOpInput = {
  fabricBase?: string;
  /** VM name for GuestKit inspect (must be stopped for disk access). */
  inspectVm?: string;
  /** Optional create-migration body for POST /api/migrations (ask). */
  createMigration?: Record<string, unknown>;
  /** Cancel migration id (ask). */
  cancelMigrationId?: string;
  /** Rescue body for POST /api/vms/{name}/rescue (ask). */
  rescue?: { vm: string; body: Record<string, unknown> };
};

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input || {}) as MigrationOpInput;
    const fabric = new FabricClient({
      baseUrl: fabricBaseFromInput(input as Record<string, unknown>),
      fetch: ctx.fetch.bind(ctx),
    });

    ctx.emit("migration-op.started", { fabricBase: fabric.baseUrl });

    const [readiness, history] = await Promise.all([
      fabric.get("/api/migrations/readiness"),
      fabric.get("/api/migrations"),
    ]);

    let inspect: unknown = null;
    if (input.inspectVm) {
      const res = await fabric.get(`/api/vms/${encodeURIComponent(input.inspectVm)}/inspect`);
      inspect = { status: res.status, ok: res.ok, body: res.json ?? res.text };
      ctx.emit("migration-op.inspect", inspect);
    }

    const wavePlan = [
      "# Migration wave plan",
      "",
      `Generated: ${new Date().toISOString()}`,
      "",
      "## Readiness",
      "```json",
      JSON.stringify(readiness.json ?? { status: readiness.status, body: readiness.text.slice(0, 2000) }, null, 2),
      "```",
      "",
      "## History",
      "```json",
      JSON.stringify(history.json ?? { status: history.status }, null, 2).slice(0, 6000),
      "```",
      "",
      "## GuestKit inspect",
      "```json",
      JSON.stringify(inspect, null, 2),
      "```",
      "",
      "## Cutover checklist",
      "- [ ] Preflight readiness green",
      "- [ ] Wave order confirmed",
      "- [ ] Rollback path documented",
      "- [ ] Operator approval for create/cancel/rescue",
      "",
      "> Transiva/hypersdk inventory is out-of-band; this pack uses Fabric migration APIs only.",
    ].join("\n");

    ctx.emit("migration-op.artifact", {
      kind: "migration-wave-plan",
      title: "Wave plan + preflight",
      body: wavePlan,
    });

    const mutates: unknown[] = [];
    if (input.createMigration) {
      const res = await fabric.post("/api/migrations", input.createMigration);
      mutates.push({ action: "create", status: res.status, ok: res.ok, body: res.json ?? res.text });
    }
    if (input.cancelMigrationId) {
      const id = encodeURIComponent(input.cancelMigrationId);
      const res = await fabric.post(`/api/migrations/${id}/cancel`);
      mutates.push({ action: "cancel", status: res.status, ok: res.ok, body: res.json ?? res.text });
    }
    if (input.rescue) {
      const res = await fabric.post(
        `/api/vms/${encodeURIComponent(input.rescue.vm)}/rescue`,
        input.rescue.body,
      );
      mutates.push({ action: "rescue", status: res.status, ok: res.ok, body: res.json ?? res.text });
    }
    for (const m of mutates) ctx.emit("migration-op.mutate.done", m);

    return {
      wave_plan_markdown: wavePlan,
      mutates,
      note: "Transiva inventory out-of-band until wired; Fabric /api/migrations + GuestKit only.",
    };
  },
});
