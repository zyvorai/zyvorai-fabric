// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** Node-only Keep evidence snapshot. The checksum detects accidental changes to a saved file;
 * it is not a signature or independent proof of the runtime's global audit chain. */
import { createHash } from "node:crypto";

export const EVIDENCE_FORMAT = "zyvor-merion-evidence-v1";
const AUDIT_LIMIT = 5000;
const RECEIPT_LIMIT = 500;

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  const encoded = JSON.stringify(value);
  if (encoded === undefined) throw new TypeError("evidence contains an unsupported value");
  return encoded;
}

function digest(payload) {
  return createHash("sha256").update(canonical(payload)).digest("hex");
}

function assertId(value, name) {
  if (typeof value !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)) {
    throw new TypeError(`${name} must be a session UUID`);
  }
}

/** Collect a session-scoped, bounded snapshot using a separate audit export capability.
 * Receipts are returned by a global bounded endpoint and filtered locally; older rows may be absent. */
export async function collectEvidence(client, { sessionId, exportToken, capturedAt = () => new Date().toISOString() }) {
  assertId(sessionId, "sessionId");
  if (typeof exportToken !== "string" || !exportToken) throw new TypeError("exportToken is required");
  if (!client?.evidence?.cockpit || !client.evidence.exportAudit || !client.evidence.receipts) {
    throw new TypeError("a Fabric SDK client with evidence APIs is required");
  }
  const startedAt = capturedAt();
  const cockpit = await client.evidence.cockpit(sessionId);
  if (cockpit?.session_id !== sessionId) throw new Error("cockpit session id mismatch");
  const audit = await client.evidence.exportAudit({ exportToken, sessionId, limit: AUDIT_LIMIT });
  if (audit?.export !== true || audit?.chain?.chain_ok !== true || !Array.isArray(audit.items)) {
    throw new Error("runtime did not return a verified audit export");
  }
  if (audit.items.some((row) => row.session_id !== sessionId)) {
    throw new Error("audit export contains another session");
  }
  const allReceipts = await client.evidence.receipts({ limit: RECEIPT_LIMIT });
  if (!Array.isArray(allReceipts)) throw new Error("runtime did not return receipt items");
  const receipts = allReceipts.filter((row) => row.session_id === sessionId);
  const payload = {
    format: EVIDENCE_FORMAT,
    session_id: sessionId,
    captured_from: startedAt,
    captured_until: capturedAt(),
    source: {
      audit_export: true,
      runtime_chain_ok: true,
      global_chain_entries: audit.chain.entries ?? null,
      audit_limit: AUDIT_LIMIT,
      receipt_limit: RECEIPT_LIMIT,
      audit_window_saturated: audit.items.length === AUDIT_LIMIT,
      receipt_window_saturated: allReceipts.length === RECEIPT_LIMIT,
      // The receipt endpoint is bounded and can have retention/purges: never claim completeness.
      receipts_complete: false,
    },
    cockpit,
    audit: audit.items,
    receipts,
  };
  return { payload, integrity: { algorithm: "sha256", digest: digest(payload) } };
}

/** Verify snapshot shape, session references, and checksum. Returns findings without throwing. */
export function verifyEvidence(bundle) {
  const findings = [];
  const payload = bundle?.payload;
  if (payload?.format !== EVIDENCE_FORMAT) findings.push("unknown evidence format");
  if (bundle?.integrity?.algorithm !== "sha256" ||
      !/^[a-f0-9]{64}$/.test(bundle?.integrity?.digest ?? "")) {
    findings.push("invalid integrity header");
  }
  if (!payload || !Array.isArray(payload.audit) || !Array.isArray(payload.receipts) ||
      payload.cockpit?.session_id !== payload.session_id ||
      payload.audit?.some((row) => row.session_id !== payload.session_id) ||
      payload.receipts?.some((row) => row.session_id !== payload.session_id)) {
    findings.push("session references are inconsistent");
  }
  if (payload?.source?.audit_export !== true || payload?.source?.runtime_chain_ok !== true ||
      payload?.source?.receipts_complete !== false) findings.push("unsupported source claims");
  try {
    if (digest(payload) !== bundle?.integrity?.digest) findings.push("payload checksum mismatch");
  } catch {
    findings.push("payload cannot be checksummed");
  }
  return {
    ok: findings.length === 0,
    findings,
    // A checksum can be replaced along with the file. This is local integrity, not authenticity.
    verification: "local-checksum-and-shape-only",
  };
}
