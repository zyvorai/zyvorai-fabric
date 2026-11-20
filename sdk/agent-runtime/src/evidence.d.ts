import type { Fabric } from "./index.js";

export declare const EVIDENCE_FORMAT: "zyvor-merion-evidence-v1";

export interface EvidencePayload {
  format: typeof EVIDENCE_FORMAT;
  session_id: string;
  captured_from: string;
  captured_until: string;
  source: {
    audit_export: true;
    runtime_chain_ok: true;
    global_chain_entries: number | null;
    audit_limit: number;
    receipt_limit: number;
    audit_window_saturated: boolean;
    receipt_window_saturated: boolean;
    receipts_complete: false;
  };
  cockpit: unknown;
  audit: unknown[];
  receipts: unknown[];
}

export interface EvidenceBundle {
  payload: EvidencePayload;
  integrity: { algorithm: "sha256"; digest: string };
}

export declare function collectEvidence(client: Fabric, options: {
  sessionId: string;
  exportToken: string;
  capturedAt?: () => string;
}): Promise<EvidenceBundle>;

/** Local checksum and shape only. Does not prove authenticity or completeness. */
export declare function verifyEvidence(bundle: unknown): {
  ok: boolean;
  findings: string[];
  verification: "local-checksum-and-shape-only";
};
