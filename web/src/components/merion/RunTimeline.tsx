// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { timelineTone, type ViewTone } from '../../lib/merionView'

export interface TimelineEntry {
  /** What happened, in words. */
  label: string
  /** The raw type or phase, shown small. */
  kind?: string
  at?: string
  source?: string
}

const DOT: Record<ViewTone, string> = {
  neutral: 'var(--zf-muted)',
  proof: 'var(--zf-proof)',
  warn: 'var(--zf-warning)',
  danger: 'var(--zf-danger)',
}
const WORD: Record<ViewTone, string> = { neutral: '', proof: 'Done', warn: 'Held', danger: 'Failed' }

function when(at?: string): string {
  if (!at) return ''
  const d = new Date(at)
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

/** The run as a vertical timeline. Each dot is coloured by outcome and also says it in a word (Done, Held, Failed),
    so colour is never the only signal. Failures win: a failed apply is never drawn as done. */
export function RunTimeline({ entries }: { entries: TimelineEntry[] }) {
  return (
    <ol className="relative ml-2 space-y-4 border-l border-[var(--zf-hairline)] pl-5" data-testid="run-timeline">
      {entries.map((e, i) => {
        const tone = timelineTone(`${e.kind ?? ''} ${e.label}`)
        return (
          <li key={i} className="relative" data-tone={tone}>
            <span
              className="absolute -left-[27px] top-1 h-3 w-3 rounded-full border-2 border-[var(--zf-surface)]"
              style={{ background: DOT[tone], boxShadow: tone === 'danger' ? '0 0 0 3px color-mix(in srgb, var(--zf-danger) 25%, transparent)' : undefined }}
              aria-hidden="true"
            />
            <div className="flex flex-wrap items-baseline gap-x-2 text-sm">
              <span className="text-[var(--zf-ink)]">{e.label}</span>
              {WORD[tone] && (
                <span className="text-xs font-medium" style={{ color: tone === 'danger' ? 'var(--zf-danger-text)' : tone === 'warn' ? 'var(--zf-warn-text)' : 'var(--zf-proof-text)' }}>
                  {WORD[tone]}
                </span>
              )}
            </div>
            <div className="text-xs text-[var(--zf-muted)]">
              {[when(e.at), e.kind, e.source].filter(Boolean).join(' · ')}
            </div>
          </li>
        )
      })}
    </ol>
  )
}
