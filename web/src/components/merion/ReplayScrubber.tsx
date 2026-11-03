// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Pause, Play } from 'lucide-react'
import { describeStep, markerPosition, offsetSeconds, stepToneText, type RecordingStep } from '../../lib/recording'
import { timelineTone, type ViewTone } from '../../lib/merionView'
import { StatusChip } from './StatusChip'

const DOT: Record<ViewTone, string> = {
  neutral: 'var(--zf-muted)',
  proof: 'var(--zf-proof)',
  warn: 'var(--zf-warning)',
  danger: 'var(--zf-danger)',
}

interface ReplayScrubberProps {
  steps: RecordingStep[]
  at: number
  onAt: (i: number) => void
  playing: boolean
  onTogglePlay: () => void
}

/** A time track with a marker for every recorded step, coloured by how it ended, under a native range input (so keyboard and
    screen-reader use is the browser's own). The step the scrubber is on is described in words below it. The markers are
    decoration for sighted people; the range and the text carry the same information. */
export function ReplayScrubber({ steps, at, onAt, playing, onTogglePlay }: ReplayScrubberProps) {
  if (steps.length === 0) return null
  const pos = steps.map((_, i) => markerPosition(i, steps.length))
  const cur = steps[Math.min(at, steps.length - 1)]
  const tone = timelineTone(stepToneText(cur))
  return (
    <div className="space-y-2" data-testid="replay-scrubber">
      <div className="flex items-center gap-3">
        <button
          type="button"
          className="zf-btn zf-btn-secondary zf-btn-sm shrink-0"
          onClick={onTogglePlay}
          aria-label={playing ? 'Pause replay' : 'Play replay'}
        >
          {playing ? <Pause className="h-3.5 w-3.5" aria-hidden="true" /> : <Play className="h-3.5 w-3.5" aria-hidden="true" />}
        </button>
        <div className="relative h-8 flex-1">
          <div className="absolute inset-x-0 top-1/2 h-1 -translate-y-1/2 rounded-full bg-[var(--zf-hairline)]" aria-hidden="true" />
          <div
            className="absolute left-0 top-1/2 h-1 -translate-y-1/2 rounded-full"
            style={{ width: `${pos[Math.min(at, pos.length - 1)] * 100}%`, background: 'var(--zf-brand)' }}
            aria-hidden="true"
          />
          {steps.map((s, i) => (
            <span
              key={s.n}
              className="absolute top-1/2 h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-[var(--zf-surface)]"
              style={{
                left: `${pos[i] * 100}%`,
                background: DOT[timelineTone(stepToneText(s))],
                transform: `translate(-50%, -50%) scale(${i === at ? 1.5 : 1})`,
                opacity: i <= at ? 1 : 0.45,
              }}
              aria-hidden="true"
              data-marker={i}
            />
          ))}
          <input
            type="range"
            className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
            aria-label="Replay position"
            aria-valuetext={`Step ${at + 1} of ${steps.length}: ${describeStep(cur)}`}
            min={0}
            max={steps.length - 1}
            value={at}
            onChange={(e) => onAt(Number(e.target.value))}
          />
        </div>
        <span className="w-16 shrink-0 text-right text-xs tabular-nums text-[var(--zf-muted)]">+{offsetSeconds(steps, cur)}s</span>
      </div>
      <div className="flex flex-wrap items-center gap-2 text-sm" aria-live="polite">
        <span className="tabular-nums text-xs text-[var(--zf-muted)]">
          Step {at + 1} of {steps.length}
        </span>
        <StatusChip label={tone === 'proof' ? 'Done' : tone === 'danger' ? 'Failed' : tone === 'warn' ? 'Held' : cur.source} tone={tone} />
        <span className="text-[var(--zf-ink)]">{describeStep(cur)}</span>
      </div>
    </div>
  )
}
