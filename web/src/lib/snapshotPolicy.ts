// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { SnapshotPolicy, SnapshotPolicyInput } from '../api/snapshotPolicy'

export const KEEP_TAG = 'keep'

/** Editable form state; numbers stay strings so half-typed input is representable. */
export interface PolicyForm {
  enabled: boolean
  scheduleKind: 'interval' | 'cron'
  intervalMinutes: string
  cron: string
  keepLast: string
  maxAgeHours: string
  namePrefix: string
  snapshotType: 'Disk' | 'Full'
}

export const DEFAULT_FORM: PolicyForm = {
  enabled: true,
  scheduleKind: 'interval',
  intervalMinutes: '60',
  cron: '0 3 * * *',
  keepLast: '7',
  maxAgeHours: '',
  namePrefix: 'auto',
  snapshotType: 'Disk',
}

export function policyToForm(p: SnapshotPolicy | null): PolicyForm {
  if (!p) return { ...DEFAULT_FORM }
  const useCron = !!p.cron
  return {
    enabled: p.enabled,
    scheduleKind: useCron ? 'cron' : 'interval',
    intervalMinutes: p.interval_secs ? String(Math.round(p.interval_secs / 60)) : DEFAULT_FORM.intervalMinutes,
    cron: p.cron ?? DEFAULT_FORM.cron,
    keepLast: p.keep_last ? String(p.keep_last) : '',
    maxAgeHours: p.max_age_secs ? String(Math.round(p.max_age_secs / 3600)) : '',
    namePrefix: p.name_prefix,
    snapshotType: p.snapshot_type,
  }
}

function positiveInt(s: string): number | null {
  if (!/^\d+$/.test(s.trim())) return null
  const n = Number(s)
  return Number.isSafeInteger(n) && n > 0 ? n : null
}

/** Client-side mirror of the daemon's rules; the daemon stays authoritative. */
export function validateForm(f: PolicyForm): string[] {
  const errs: string[] = []
  if (f.scheduleKind === 'interval') {
    if (positiveInt(f.intervalMinutes) === null) errs.push('Interval must be a whole number of minutes, at least 1')
  } else if (f.cron.trim().split(/\s+/).filter(Boolean).length !== 5) {
    errs.push('Cron needs 5 fields: minute hour day-of-month month day-of-week (UTC)')
  }
  const keep = f.keepLast.trim()
  const age = f.maxAgeHours.trim()
  if (keep && positiveInt(keep) === null) errs.push('Keep last must be a whole number of at least 1')
  if (age && positiveInt(age) === null) errs.push('Max age must be a whole number of hours, at least 1')
  if (!keep && !age) errs.push('Set at least one retention rule: keep last or max age')
  const prefix = f.namePrefix.trim()
  if (prefix.length < 1 || prefix.length > 32) errs.push('Name prefix must be 1 to 32 characters')
  else if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(prefix)) errs.push('Name prefix may only use letters, digits, . _ - and must start with a letter or digit')
  return errs
}

/** Build the PUT body. Call only when `validateForm` returns no errors. */
export function formToInput(f: PolicyForm): SnapshotPolicyInput {
  const interval = f.scheduleKind === 'interval' ? (positiveInt(f.intervalMinutes) ?? 1) * 60 : null
  return {
    enabled: f.enabled,
    interval_secs: interval,
    cron: f.scheduleKind === 'cron' ? f.cron.trim() : null,
    keep_last: f.keepLast.trim() ? positiveInt(f.keepLast) : null,
    max_age_secs: f.maxAgeHours.trim() ? (positiveInt(f.maxAgeHours) ?? 1) * 3600 : null,
    name_prefix: f.namePrefix.trim(),
    snapshot_type: f.snapshotType,
  }
}

/** Human text for the policy's next run. A time in the past means it is due. */
export function describeNextRun(next: string | null | undefined, enabled: boolean, now: Date = new Date()): string {
  if (!enabled) return 'Not scheduled (policy disabled)'
  if (!next) return 'Not scheduled'
  const t = new Date(next)
  if (Number.isNaN(t.getTime())) return 'Not scheduled'
  const diffMin = Math.round((t.getTime() - now.getTime()) / 60000)
  if (diffMin <= 0) return 'Due now (runs on the next scheduler tick)'
  const rel = diffMin < 60 ? `${diffMin} min` : diffMin < 1440 ? `${Math.round(diffMin / 60)} h` : `${Math.round(diffMin / 1440)} d`
  return `${t.toISOString().replace('T', ' ').slice(0, 16)} UTC (in ${rel})`
}

export function hasKeep(tags?: string[] | null): boolean {
  return !!tags?.includes(KEEP_TAG)
}

/** Tags after toggling `keep`; other tags are preserved. */
export function toggleKeep(tags?: string[] | null): string[] {
  const cur = tags ?? []
  return hasKeep(cur) ? cur.filter((t) => t !== KEEP_TAG) : [...cur, KEEP_TAG]
}
