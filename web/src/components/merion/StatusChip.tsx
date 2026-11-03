// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { timelineTone, type ViewTone } from '../../lib/merionView'

const COLOR: Record<ViewTone, string> = {
  neutral: 'var(--zf-muted)',
  proof: 'var(--zf-proof)',
  warn: 'var(--zf-warning)',
  danger: 'var(--zf-danger)',
}
const TEXT: Record<ViewTone, string> = {
  neutral: 'var(--zf-secondary)',
  proof: 'var(--zf-proof-text)',
  warn: 'var(--zf-warn-text)',
  danger: 'var(--zf-danger-text)',
}

/** A status as a pill: a coloured dot and the status in words. The tone comes from the words unless given, so a status the
    code has not seen before still gets a sensible colour, and the colour is never the only signal. */
export function StatusChip({ label, tone }: { label: string; tone?: ViewTone }) {
  const t = tone ?? timelineTone(label)
  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-full border border-[var(--zf-hairline)] px-2 py-0.5 text-xs font-medium"
      style={{ color: TEXT[t] }}
      data-tone={t}
    >
      <span className="h-1.5 w-1.5 rounded-full" style={{ background: COLOR[t] }} aria-hidden="true" />
      {label}
    </span>
  )
}
