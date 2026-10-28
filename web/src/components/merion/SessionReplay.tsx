// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { getSessionRecording, verifyRecording } from '../../api/agents'
import { describeStep, offsetSeconds, playedSteps, type SignedRecording } from '../../lib/recording'
import { Card } from '../ui'

/** Steps through a session's signed recording. Content is masked on the host, so this shows what happened and when, not what was said. */
export default function SessionReplay({ sessionId }: { sessionId: string }) {
  const [rec, setRec] = useState<SignedRecording | null>(null)
  const [verdict, setVerdict] = useState<string>('')
  const [at, setAt] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const load = async () => {
    setBusy(true)
    setError('')
    try {
      const signed = await getSessionRecording(sessionId)
      const check = await verifyRecording(signed)
      setRec(signed)
      setAt(0)
      setVerdict(check.valid ? `Signature valid · signer ${(check.signer ?? '').slice(0, 12)}…` : `Not valid: ${check.reason ?? 'unknown'}`)
    } catch (e) {
      const text = e instanceof Error ? e.message : String(e)
      setError(/not enabled|404/i.test(text) ? 'Recording is off on this host (no recording key is set).' : text)
    } finally {
      setBusy(false)
    }
  }

  const steps = rec?.bundle.steps ?? []
  const shown = playedSteps(steps, at)
  const download = () => {
    if (!rec) return
    const url = URL.createObjectURL(new Blob([JSON.stringify(rec, null, 2)], { type: 'application/json' }))
    const a = document.createElement('a')
    a.href = url
    a.download = `recording-${sessionId}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  return (
    <Card className="p-4 space-y-3 text-sm">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-base font-semibold text-[var(--zf-ink)]">Replay</h2>
        <div className="flex gap-2">
          {rec && (
            <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" onClick={download}>
              Download signed JSON
            </button>
          )}
          <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" onClick={load} disabled={busy}>
            {busy ? 'Loading…' : rec ? 'Reload' : 'Load recording'}
          </button>
        </div>
      </div>
      <p className="text-xs text-[var(--zf-muted)]">
        A signed record of what the host saw. Message text, request bodies and approval wording are left out on purpose.
      </p>
      {error && <p role="alert" className="text-xs text-[var(--zf-danger)]">{error}</p>}
      {rec && (
        <>
          <p className="text-xs text-[var(--zf-muted)]">{verdict}</p>
          {steps.length === 0 ? (
            <p className="text-xs text-[var(--zf-muted)]">Nothing was recorded for this session.</p>
          ) : (
            <>
              <label className="flex items-center gap-3 text-xs text-[var(--zf-muted)]">
                <span className="tabular-nums w-24">
                  Step {at + 1} of {steps.length}
                </span>
                <input
                  type="range"
                  className="flex-1"
                  aria-label="Replay position"
                  min={0}
                  max={steps.length - 1}
                  value={at}
                  onChange={(e) => setAt(Number(e.target.value))}
                />
                <span className="tabular-nums w-16 text-right">+{offsetSeconds(steps, steps[at])}s</span>
              </label>
              <ol className="space-y-1" data-testid="replay-steps">
                {shown.map((s) => (
                  <li key={s.n} className={s.n === at ? 'font-medium text-[var(--zf-ink)]' : 'text-[var(--zf-muted)]'}>
                    <span className="tabular-nums text-[var(--zf-muted)]">+{offsetSeconds(steps, s)}s</span> · {describeStep(s)}
                  </li>
                ))}
              </ol>
            </>
          )}
        </>
      )}
    </Card>
  )
}
