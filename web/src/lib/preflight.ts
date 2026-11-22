// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

/** Types and pure helpers for `POST /api/vms/{name}/preflight`. */

export type PreflightSeverity = 'blocker' | 'warning' | 'info'

export interface PreflightFinding {
  id: string
  severity: PreflightSeverity
  message: string
  hint: string
}

export interface PreflightResult {
  vm: string
  backend: string
  can_start: boolean
  summary: { blockers: number; warnings: number; infos: number }
  findings: PreflightFinding[]
}

export const SEVERITY_ORDER: readonly PreflightSeverity[] = ['blocker', 'warning', 'info']

export const SEVERITY_LABEL: Record<PreflightSeverity, string> = {
  blocker: 'Blockers',
  warning: 'Warnings',
  info: 'Notes',
}

export interface PreflightGroup {
  severity: PreflightSeverity
  label: string
  findings: PreflightFinding[]
}

/** Group findings by severity (blockers first), dropping empty groups. Unknown severities count as info. */
export function groupFindings(findings: PreflightFinding[]): PreflightGroup[] {
  const known = new Set<string>(SEVERITY_ORDER)
  return SEVERITY_ORDER.map((severity) => ({
    severity,
    label: SEVERITY_LABEL[severity],
    findings: findings.filter((f) => (known.has(f.severity) ? f.severity : 'info') === severity),
  })).filter((g) => g.findings.length > 0)
}

export type PreflightTone = 'ok' | 'warn' | 'blocked'

/** Derive the headline from the findings, so it can never disagree with the list shown under it. */
export function preflightVerdict(findings: PreflightFinding[]): { tone: PreflightTone; text: string } {
  const blockers = findings.filter((f) => f.severity === 'blocker').length
  const warnings = findings.filter((f) => f.severity === 'warning').length
  if (blockers > 0) {
    return { tone: 'blocked', text: `Cannot start: ${plural(blockers, 'blocker')}${warnings ? `, ${plural(warnings, 'warning')}` : ''}` }
  }
  if (warnings > 0) {
    return { tone: 'warn', text: `Can start, with ${plural(warnings, 'warning')}` }
  }
  return { tone: 'ok', text: 'Ready to start: no problems found' }
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`
}
