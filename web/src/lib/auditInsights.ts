// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Pure anomaly summary over audit log entries (same shape as GET /api/audit/logs).
// Heuristics only: every threshold is an explicit option so operators can tune it.

export interface AuditEntry {
  id: string
  timestamp: string
  user: string
  action: string
  resource_type: string
  resource_name: string
  status: 'success' | 'failed'
  ip_address?: string | null
}

export interface WorkingHours {
  /** First working hour, inclusive (0-23). */
  startHour: number
  /** End hour, exclusive (1-24). */
  endHour: number
  /** Days counted as working days, 0 = Sunday .. 6 = Saturday. */
  workDays: number[]
  /** Offset of the operators' local time from UTC, in minutes (e.g. 330 for IST). */
  utcOffsetMinutes: number
}

export interface InsightOptions {
  /** Burst: this many events by one actor inside the window raises an insight. */
  burstThreshold: number
  burstWindowMinutes: number
  /** Failed-auth streak: this many consecutive failed auth events by one actor. */
  authFailureStreak: number
  /** Which actions count as authentication events. */
  authActionPattern: RegExp
  /** Which actions count as privileged. */
  privilegedActionPattern: RegExp
  /** Working hours for the off-hours check; null disables the check. */
  workingHours: WorkingHours | null
  /** An actor or IP whose first ever event is within this many hours of the newest event is "new". */
  newWithinHours: number
}

export const DEFAULT_INSIGHT_OPTIONS: InsightOptions = {
  burstThreshold: 20,
  burstWindowMinutes: 5,
  authFailureStreak: 5,
  authActionPattern: /login|logout|auth|token|password|mfa|session/i,
  privilegedActionPattern: /delete|destroy|purge|role|permission|policy|secret|credential|api[_-]?key|user|config|export|restore|admin/i,
  workingHours: { startHour: 8, endHour: 20, workDays: [1, 2, 3, 4, 5], utcOffsetMinutes: 0 },
  newWithinHours: 24,
}

export type InsightKind = 'burst' | 'auth_failures' | 'off_hours_privileged' | 'new_actor' | 'new_ip'
export type InsightSeverity = 'critical' | 'warning' | 'info'

export interface AuditInsight {
  kind: InsightKind
  severity: InsightSeverity
  actor: string
  title: string
  detail: string
  count: number
  firstAt: string
  lastAt: string
  eventIds: string[]
}

const SEVERITY_RANK: Record<InsightSeverity, number> = { critical: 0, warning: 1, info: 2 }
const MIN = 60_000
const HOUR = 60 * MIN

interface Ev { e: AuditEntry; t: number }

function prepare(entries: AuditEntry[]): Ev[] {
  const evs: Ev[] = []
  for (const e of entries) {
    const t = Date.parse(e.timestamp)
    if (!Number.isNaN(t)) evs.push({ e, t })
  }
  return evs.sort((a, b) => a.t - b.t || (a.e.id < b.e.id ? -1 : a.e.id > b.e.id ? 1 : 0))
}

const iso = (t: number) => new Date(t).toISOString()

function byActor(evs: Ev[]): Map<string, Ev[]> {
  const m = new Map<string, Ev[]>()
  for (const ev of evs) {
    const list = m.get(ev.e.user)
    if (list) list.push(ev)
    else m.set(ev.e.user, [ev])
  }
  return m
}

function bursts(evs: Ev[], o: InsightOptions): AuditInsight[] {
  const out: AuditInsight[] = []
  const windowMs = o.burstWindowMinutes * MIN
  for (const [actor, list] of byActor(evs)) {
    let bestLen = 0
    let bestStart = 0
    let lo = 0
    for (let hi = 0; hi < list.length; hi++) {
      while (list[hi].t - list[lo].t > windowMs) lo++
      if (hi - lo + 1 > bestLen) { bestLen = hi - lo + 1; bestStart = lo }
    }
    if (bestLen >= o.burstThreshold) {
      const win = list.slice(bestStart, bestStart + bestLen)
      out.push({
        kind: 'burst',
        severity: 'warning',
        actor,
        title: `Burst of activity by ${actor}`,
        detail: `${bestLen} events within ${o.burstWindowMinutes} minute${o.burstWindowMinutes === 1 ? '' : 's'} (threshold ${o.burstThreshold}).`,
        count: bestLen,
        firstAt: iso(win[0].t),
        lastAt: iso(win[win.length - 1].t),
        eventIds: win.map((w) => w.e.id),
      })
    }
  }
  return out
}

function authFailures(evs: Ev[], o: InsightOptions): AuditInsight[] {
  const out: AuditInsight[] = []
  for (const [actor, list] of byActor(evs)) {
    let run: Ev[] = []
    const flush = () => {
      if (run.length >= o.authFailureStreak) {
        out.push({
          kind: 'auth_failures',
          severity: run.length >= o.authFailureStreak * 2 ? 'critical' : 'warning',
          actor,
          title: `${run.length} failed sign-in attempts in a row for ${actor}`,
          detail: `Consecutive failed authentication events with no success in between (threshold ${o.authFailureStreak}).`,
          count: run.length,
          firstAt: iso(run[0].t),
          lastAt: iso(run[run.length - 1].t),
          eventIds: run.map((r) => r.e.id),
        })
      }
      run = []
    }
    for (const ev of list) {
      if (!o.authActionPattern.test(ev.e.action)) continue
      if (ev.e.status === 'failed') run.push(ev)
      else flush()
    }
    flush()
  }
  return out
}

function offHours(evs: Ev[], o: InsightOptions): AuditInsight[] {
  const wh = o.workingHours
  if (!wh) return []
  const out: AuditInsight[] = []
  for (const [actor, list] of byActor(evs)) {
    const hits = list.filter((ev) => {
      if (!o.privilegedActionPattern.test(ev.e.action)) return false
      const local = new Date(ev.t + wh.utcOffsetMinutes * MIN)
      const hour = local.getUTCHours()
      const day = local.getUTCDay()
      return !(wh.workDays.includes(day) && hour >= wh.startHour && hour < wh.endHour)
    })
    if (hits.length > 0) {
      const actions = [...new Set(hits.map((h) => h.e.action))].sort()
      out.push({
        kind: 'off_hours_privileged',
        severity: 'warning',
        actor,
        title: `Privileged actions by ${actor} outside working hours`,
        detail: `${hits.length} event${hits.length === 1 ? '' : 's'} (${actions.join(', ')}) outside ${wh.startHour}:00-${wh.endHour}:00 on configured working days.`,
        count: hits.length,
        firstAt: iso(hits[0].t),
        lastAt: iso(hits[hits.length - 1].t),
        eventIds: hits.map((h) => h.e.id),
      })
    }
  }
  return out
}

function firstSeen(evs: Ev[], o: InsightOptions): AuditInsight[] {
  if (evs.length === 0) return []
  const newest = evs[evs.length - 1].t
  const recentStart = newest - o.newWithinHours * HOUR
  // Without history older than the window every actor would look new, so say nothing.
  if (evs[0].t >= recentStart) return []
  const out: AuditInsight[] = []
  const seen = (key: (ev: Ev) => string | null, kind: InsightKind, label: string) => {
    const first = new Map<string, Ev[]>()
    for (const ev of evs) {
      const k = key(ev)
      if (!k) continue
      const l = first.get(k)
      if (l) l.push(ev)
      else first.set(k, [ev])
    }
    for (const [k, list] of first) {
      if (list[0].t < recentStart) continue
      out.push({
        kind,
        severity: 'info',
        actor: kind === 'new_actor' ? k : list[0].e.user,
        title: `First time seeing ${label} ${k}`,
        detail: `First event at ${iso(list[0].t)}, within the last ${o.newWithinHours} hours; earlier history exists without it.`,
        count: list.length,
        firstAt: iso(list[0].t),
        lastAt: iso(list[list.length - 1].t),
        eventIds: list.map((l) => l.e.id),
      })
    }
  }
  seen((ev) => ev.e.user || null, 'new_actor', 'actor')
  seen((ev) => ev.e.ip_address || null, 'new_ip', 'IP address')
  return out
}

/** Compute insights. Deterministic: depends only on the entries and options. */
export function computeAuditInsights(entries: AuditEntry[], options: Partial<InsightOptions> = {}): AuditInsight[] {
  const o: InsightOptions = { ...DEFAULT_INSIGHT_OPTIONS, ...options }
  const evs = prepare(entries)
  const all = [...bursts(evs, o), ...authFailures(evs, o), ...offHours(evs, o), ...firstSeen(evs, o)]
  return all.sort((a, b) =>
    SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity] ||
    (a.firstAt < b.firstAt ? -1 : a.firstAt > b.firstAt ? 1 : 0) ||
    (a.title < b.title ? -1 : a.title > b.title ? 1 : 0),
  )
}

export function insightsHeadline(insights: AuditInsight[]): string {
  if (insights.length === 0) return 'No unusual activity found'
  const c = insights.filter((i) => i.severity === 'critical').length
  const w = insights.filter((i) => i.severity === 'warning').length
  const parts: string[] = []
  if (c) parts.push(`${c} critical`)
  if (w) parts.push(`${w} warning${w === 1 ? '' : 's'}`)
  const rest = insights.length - c - w
  if (rest) parts.push(`${rest} to note`)
  return parts.join(', ')
}
