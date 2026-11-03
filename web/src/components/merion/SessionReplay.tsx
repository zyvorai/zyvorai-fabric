// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react'
import { getSessionRecording, verifyRecording } from '../../api/agents'
import { describeStep, playedSteps, type SignedRecording } from '../../lib/recording'
import { Card } from '../ui'
import { ReplayScrubber } from './ReplayScrubber'
import { RunTimeline } from './RunTimeline'
import { StatusChip } from './StatusChip'

/** Steps through a session's signed recording. Content is masked on the host, so this shows what happened and when, not what was said. */
export default function SessionReplay({ sessionId }: { sessionId: string }) {
  const [rec, setRec] = useState<SignedRecording | null>(null)
  const [verdict, setVerdict] = useState<string>('')
  const [valid, setValid] = useState(false)
  const [at, setAt] = useState(0)
  const [busy, setBusy] = useState(false)
  const [playing, setPlaying] = useState(false)
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
      setValid(check.valid)
    } catch (e) {
      const text = e instanceof Error ? e.message : String(e)
      setError(/not enabled|404/i.test(text) ? 'Recording is off on this host (no recording key is set).' : text)
    } finally {
      setBusy(false)
    }
  }

  const steps = rec?.bundle.steps ?? []
  // Playing steps forward once a second. A person starts it; nothing plays on load.
  useEffect(() => {
    if (!playing) return
    const id = window.setInterval(() => setAt((i) => Math.min(i + 1, steps.length - 1)), 1000)
    return () => window.clearInterval(id)
  }, [playing, steps.length])
  // ...and it stops by itself on the last step.
  useEffect(() => {
    if (playing && steps.length > 0 && at >= steps.length - 1) setPlaying(false)
  }, [playing, at, steps.length])
  const shown = playedSteps(steps, at)
  // Pressing play on the last step starts over from the first.
  const togglePlay = () => {
    if (!playing && at >= steps.length - 1) setAt(0)
    setPlaying(!playing)
  }
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
      {error && <p role="alert" className="text-xs text-[var(--zf-danger-text)]">{error}</p>}
      {rec && (
        <>
          <p className="flex items-center gap-2 text-xs text-[var(--zf-muted)]">
            <StatusChip label={valid ? 'Signed' : 'Not valid'} tone={valid ? 'proof' : 'danger'} />
            {verdict}
          </p>
          {steps.length === 0 ? (
            <p className="text-xs text-[var(--zf-muted)]">Nothing was recorded for this session.</p>
          ) : (
            <>
              <ReplayScrubber steps={steps} at={at} onAt={setAt} playing={playing} onTogglePlay={togglePlay} />
              <div data-testid="replay-steps">
                <RunTimeline entries={shown.map((s) => ({ label: describeStep(s), kind: s.source, at: s.at }))} />
              </div>
            </>
          )}
        </>
      )}
    </Card>
  )
}
