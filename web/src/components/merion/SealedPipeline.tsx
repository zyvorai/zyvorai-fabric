// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Check, FileSearch, FileText, Lock, X } from 'lucide-react'
import { pipelineStates, type NodeState, type RunStage } from '../../lib/merionView'

interface SealedPipelineProps {
  stage: RunStage
  labels: { cell: string; extract: string; artifact: string; file?: string }
}

const RING: Record<NodeState, string> = {
  todo: 'var(--zf-hairline)',
  active: 'var(--zf-brand)',
  done: 'var(--zf-proof)',
  fail: 'var(--zf-danger)',
}
const WORD: Record<NodeState, string> = { todo: 'waiting', active: 'working', done: 'done', fail: 'failed' }

/** The run as four nodes: your file, the sealed cell (0 CONNECT), the extraction, the artifact. The node that is working
    pulses; a finished one is green with a check; a failure is red with a cross. Each node names its state in words for screen
    readers, so the colour is never the only signal. */
export function SealedPipeline({ stage, labels }: SealedPipelineProps) {
  const states = pipelineStates(stage)
  const nodes = [
    { label: labels.file ?? 'Your file', Icon: FileText },
    { label: labels.cell, Icon: Lock },
    { label: labels.extract, Icon: FileSearch },
    { label: labels.artifact, Icon: FileText },
  ]
  return (
    <ol className="flex items-start" data-testid="sealed-pipeline" data-stage={stage}>
      {nodes.map((n, i) => {
        const st = states[i]
        const linkOn = i > 0 && states[i - 1] === 'done' && st !== 'todo' && st !== 'fail'
        return (
          <li key={n.label} className="flex flex-1 items-start">
            {i > 0 && (
              <svg viewBox="0 0 40 4" className="mt-6 h-1 w-0 min-w-4 flex-1" preserveAspectRatio="none" aria-hidden="true">
                <path
                  d="M0 2H40"
                  stroke={linkOn ? 'var(--zf-proof)' : 'var(--zf-hairline)'}
                  strokeWidth="2"
                  strokeLinecap="round"
                  fill="none"
                  strokeDasharray="5 6"
                  className={st === 'active' ? 'zf-flow' : undefined}
                />
              </svg>
            )}
            <div className="flex w-16 shrink-0 flex-col items-center gap-1.5 text-center">
              <span
                className="relative flex h-12 w-12 items-center justify-center rounded-full border-2 bg-[var(--zf-surface)]"
                style={{ borderColor: RING[st], color: st === 'todo' ? 'var(--zf-muted)' : RING[st] }}
              >
                <n.Icon className="h-5 w-5" aria-hidden="true" />
                {st === 'done' && (
                  <span className="absolute -right-1 -top-1 flex h-4 w-4 items-center justify-center rounded-full" style={{ background: 'var(--zf-proof)' }}>
                    <Check className="h-3 w-3 text-white" aria-hidden="true" strokeWidth={3} />
                  </span>
                )}
                {st === 'fail' && (
                  <span className="absolute -right-1 -top-1 flex h-4 w-4 items-center justify-center rounded-full" style={{ background: 'var(--zf-danger-fill)' }}>
                    <X className="h-3 w-3 text-white" aria-hidden="true" strokeWidth={3} />
                  </span>
                )}
              </span>
              <span className="text-xs leading-tight text-[var(--zf-secondary)]">{n.label}</span>
              <span className="sr-only">{WORD[st]}</span>
            </div>
          </li>
        )
      })}
    </ol>
  )
}
