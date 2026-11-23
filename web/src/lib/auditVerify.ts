// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Plain-language summary of an audit chain verification report.

import type { AuditVerifyReport } from '../api/audit'

export type VerifyTone = 'ok' | 'broken' | 'neutral'

export interface VerifySummary {
  tone: VerifyTone
  headline: string
  detail: string
}

const REASONS: Record<string, string> = {
  hash_mismatch: 'An event was modified after it was written',
  prev_hash_mismatch: 'Events were reordered or replaced',
  sequence_gap: 'One or more events were removed',
  duplicate_seq: 'An event sequence number appears twice',
  truncated: 'Newer events were removed from the end of the log',
  head_missing: 'The chain head record is missing',
  head_mismatch: 'The newest event does not match the chain head',
  anchor_mismatch: 'An event does not match the retention anchor',
  unchained_inside_chain: 'An event without a hash is dated inside the chain',
  unreadable_record: 'A stored event cannot be read',
}

export function describeVerify(r: AuditVerifyReport): VerifySummary {
  if (r.status === 'not_chained') {
    return {
      tone: 'neutral',
      headline: 'Nothing to verify: the hash chain is not active',
      detail: `${r.total} event${r.total === 1 ? '' : 's'} stored without hashes. Set audit chain = true in the daemon config to start one.`,
    }
  }
  if (r.ok) {
    return {
      tone: 'ok',
      headline: `Chain intact: ${r.chained} chained event${r.chained === 1 ? '' : 's'} verified`,
      detail:
        r.legacy_unchained > 0
          ? `${r.legacy_unchained} older event${r.legacy_unchained === 1 ? ' predates' : 's predate'} the chain and cannot be verified.`
          : 'Every event links to the one before it.',
    }
  }
  const b = r.first_break
  const where = b?.seq != null ? ` at event ${b.seq}` : ''
  return {
    tone: 'broken',
    headline: `Chain broken${where}`,
    detail: `${REASONS[b?.reason ?? ''] ?? b?.reason ?? 'Integrity check failed'}${b?.detail ? `: ${b.detail}` : ''}`,
  }
}
