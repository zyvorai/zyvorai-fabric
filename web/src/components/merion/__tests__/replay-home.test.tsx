// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { ReplayScrubber } from '../ReplayScrubber'
import { SealedPipeline } from '../SealedPipeline'
import { StatusChip } from '../StatusChip'
import { DropZone } from '../DropZone'
import { markerPosition, stepToneText, type RecordingStep } from '../../../lib/recording'

const step = (n: number, secs: number, extra: Partial<RecordingStep> = {}): RecordingStep => ({
  n, source: 'journal', at: new Date(1_700_000_000_000 + secs * 1000).toISOString(), action: 'model.call', phase: 'Performed', ...extra,
})

describe('markerPosition', () => {
  it('spaces steps evenly, so a marker is where the range thumb lands', () => {
    expect([0, 1, 2, 3, 4].map((i) => markerPosition(i, 5))).toEqual([0, 0.25, 0.5, 0.75, 1])
  })
  it('handles one step and out-of-range input without NaN or leaving the track', () => {
    expect(markerPosition(0, 1)).toBe(0)
    expect(markerPosition(0, 0)).toBe(0)
    expect(markerPosition(9, 3)).toBe(1)
    expect(markerPosition(-2, 3)).toBe(0)
  })
})

describe('stepToneText', () => {
  it('reads what a step was and how it ended', () => {
    expect(stepToneText(step(0, 0, { action: 'changeset.apply', phase: 'Failed' }))).toBe('changeset.apply Failed')
    expect(stepToneText({ n: 1, source: 'approval', at: '', kind: 'changeset', status: 'approved' })).toBe('changeset approved')
  })
})

describe('ReplayScrubber', () => {
  const steps = [step(0, 0), step(1, 10, { action: 'egress.drop', phase: 'Denied' }), step(2, 20, { action: 'changeset.apply', phase: 'Failed' })]
  const html = (at: number, playing = false) =>
    renderToStaticMarkup(<ReplayScrubber steps={steps} at={at} onAt={() => {}} playing={playing} onTogglePlay={() => {}} />)

  it('is a native range with a readable value, one marker per step, and the current step in words', () => {
    const h = html(1)
    expect(h).toContain('type="range"')
    expect(h).toContain('aria-label="Replay position"')
    expect(h).toContain('aria-valuetext="Step 2 of 3')
    expect((h.match(/data-marker=/g) ?? []).length).toBe(3)
    expect(h).toContain('Step 2 of 3')
    expect(h).toContain('egress.drop (Denied)')
    expect(h).toContain('Held')
  })

  it('words a failed step as Failed and names the play button for its state', () => {
    expect(html(2)).toContain('>Failed<')
    expect(html(0)).toContain('aria-label="Play replay"')
    expect(html(0, true)).toContain('aria-label="Pause replay"')
  })

  it('renders nothing for an empty recording', () => {
    expect(renderToStaticMarkup(<ReplayScrubber steps={[]} at={0} onAt={() => {}} playing={false} onTogglePlay={() => {}} />)).toBe('')
  })
})

describe('SealedPipeline', () => {
  const labels = { cell: 'Sealed cell', extract: 'Extract', artifact: 'Artifact' }
  it('names each node state in words and shows all four nodes', () => {
    const h = renderToStaticMarkup(<SealedPipeline stage="extract" labels={labels} />)
    expect((h.match(/<li /g) ?? []).length).toBe(4)
    expect(h).toContain('data-stage="extract"')
    expect(h).toContain('>working<')
    expect(h).toContain('>done<')
    expect(h).toContain('>waiting<')
    expect(h).toContain('Sealed cell')
  })
  it('shows a failure as failed and never a finished artifact', () => {
    const h = renderToStaticMarkup(<SealedPipeline stage="fail" labels={labels} />)
    expect(h).toContain('>failed<')
    expect(h.match(/>done</g)?.length).toBe(1) // only the file reached the cell
  })
})

describe('StatusChip', () => {
  it('colours a status from its words and always prints the words', () => {
    expect(renderToStaticMarkup(<StatusChip label="approved" />)).toContain('data-tone="proof"')
    expect(renderToStaticMarkup(<StatusChip label="denied" />)).toContain('data-tone="warn"')
    expect(renderToStaticMarkup(<StatusChip label="Failed" />)).toContain('data-tone="danger"')
    expect(renderToStaticMarkup(<StatusChip label="pending" />)).toContain('data-tone="neutral"')
    expect(renderToStaticMarkup(<StatusChip label="pending" />)).toContain('pending')
  })
  it('lets the caller set the tone', () => {
    expect(renderToStaticMarkup(<StatusChip label="Signed" tone="proof" />)).toContain('data-tone="proof"')
  })
})

describe('DropZone', () => {
  it('shows what is chosen instead of the prompt, and is a real button', () => {
    const chosen = renderToStaticMarkup(<DropZone chosen="a.pdf" prompt="Pick a PDF" onFiles={() => {}} onBrowse={() => {}} />)
    expect(chosen).toContain('a.pdf')
    expect(chosen).not.toContain('Pick a PDF')
    expect(chosen).toContain('type="button"')
    const empty = renderToStaticMarkup(<DropZone chosen={null} prompt="Pick a PDF" hint="up to 32 MB" onFiles={() => {}} onBrowse={() => {}} />)
    expect(empty).toContain('Pick a PDF')
    expect(empty).toContain('up to 32 MB')
  })
  it('is disabled while a run is going', () => {
    expect(renderToStaticMarkup(<DropZone chosen={null} prompt="x" disabled onFiles={() => {}} onBrowse={() => {}} />)).toContain('disabled=""')
  })
})
