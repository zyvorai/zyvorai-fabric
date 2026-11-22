// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { ReactNode } from 'react'
import { ansiToSpans } from '../utils/ansi'

/** Render ANSI console text as colored spans (safe — no HTML injection). */
export function AnsiText({ text, className = '' }: { text: string; className?: string }): ReactNode {
  const spans = ansiToSpans(text)
  return (
    <span className={className}>
      {spans.map((s, i) => (
        <span
          key={i}
          style={{
            color: s.color,
            fontWeight: s.bold ? 600 : undefined,
            opacity: s.dim ? 0.65 : undefined,
          }}
        >
          {s.text}
        </span>
      ))}
    </span>
  )
}
