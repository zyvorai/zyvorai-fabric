// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react'

type Props = {
  steps: readonly string[]
  current: number
  onStep: (index: number) => void
  trailing?: ReactNode
}

/** Horizontal step buttons for multi-step forms (Create VM, Import, etc.). */
export default function WizardStepper({ steps, current, onStep, trailing }: Props) {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl p-4 border border-[var(--zf-hairline)] flex flex-wrap items-center justify-between gap-3">
      <div className="flex flex-wrap gap-2">
        {steps.map((label, i) => (
          <button
            key={label}
            type="button"
            onClick={() => onStep(i)}
            className={`text-xs px-2.5 py-1.5 rounded-lg transition font-medium ${
              i === current
                ? 'bg-[var(--zf-link)] text-white shadow-sm'
                : i < current
                  ? 'bg-[var(--zf-canvas-alt)] text-[var(--zf-ink)] hover:bg-[var(--zf-hairline)]'
                  : 'bg-[var(--zf-surface)] text-[var(--zf-muted)] hover:bg-black/[0.04]'
            }`}
          >
            {i + 1}. {label}
          </button>
        ))}
      </div>
      {trailing}
    </div>
  )
}
