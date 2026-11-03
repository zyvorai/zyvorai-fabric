// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { fleetSegments, type FleetCounts, type FleetSegment } from '../lib/fleet'

const COLOR: Record<FleetSegment['key'], string> = {
  running: 'var(--zf-success)',
  paused: 'var(--zf-warning)',
  stopped: 'var(--zf-muted)',
  failed: 'var(--zf-danger)',
  other: 'var(--zf-cool)',
}

/** The fleet's state as one proportional bar with a legend of counts. The counts are text, so the bar is a glance and never the
    only signal; only a failed VM is red (a stopped one is a normal state). `shown` is how many the current search or filter
    leaves, when that is fewer than all. */
export function FleetBar({ counts, shown }: { counts: FleetCounts; shown?: number }) {
  const segs = fleetSegments(counts)
  const noun = counts.total === 1 ? 'VM' : 'VMs'
  return (
    <div className="mb-5 space-y-2" data-testid="fleet-bar">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
        <span className="font-medium text-[var(--zf-ink)]">
          {shown !== undefined && shown !== counts.total ? `${shown} of ${counts.total} ${noun}` : `${counts.total} ${noun}`}
        </span>
        {segs.map((s) => (
          <span key={s.key} className="flex items-center gap-1.5 text-[var(--zf-muted)]">
            <span className="h-2 w-2 rounded-full" style={{ background: COLOR[s.key] }} aria-hidden="true" />
            {s.count} {s.label}
          </span>
        ))}
      </div>
      <div className="flex h-1.5 gap-0.5 overflow-hidden rounded-full bg-[var(--zf-hairline)]" aria-hidden="true">
        {segs.map((s) => (
          <span key={s.key} style={{ width: `${s.share * 100}%`, background: COLOR[s.key] }} />
        ))}
      </div>
    </div>
  )
}
