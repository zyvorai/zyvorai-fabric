// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { AnimatedNumber, Donut } from './ui'
import { LiveFabric } from './LiveFabric'
import { LoadBar } from './LoadBar'
import { fleetCounts } from '../lib/fleet'
import { SAMPLE_FLEET } from '../lib/sampleFleet'

const NAV = ['Dashboard', 'Virtual Machines', 'Network', 'Storage', 'Merion']
const ROWS: Array<[string, number, number]> = [
  ['web-01', 38, 42],
  ['api-01', 61, 55],
  ['db-primary', 83, 71],
]

/** A picture of the console for the marketing page, built from the same pieces as the real dashboard (the live fleet graphic,
    the fleet donut, the load bars) on an invented fleet, so what the page shows is what the product looks like. It is labelled
    as example data, and a screen reader gets one description of it instead of its parts. */
export function ConsoleMock() {
  const fleet = fleetCounts(SAMPLE_FLEET)
  return (
    <figure className="m-0">
      <div
        role="img"
        aria-label="Example of the Zyvor Fabric console: a fleet of eight virtual machines, six running, with the fleet status, a live map of the machines and their CPU and memory load."
        className="overflow-hidden rounded-[20px] border border-[var(--zf-hairline)] bg-[var(--zf-canvas)] text-left shadow-[var(--zf-shadow-3)]"
      >
        <div className="flex items-center gap-2 border-b border-[var(--zf-hairline)] bg-[var(--zf-surface)] px-4 py-2.5" aria-hidden="true">
          <span className="h-2.5 w-2.5 rounded-full bg-[var(--zf-danger)]" />
          <span className="h-2.5 w-2.5 rounded-full bg-[var(--zf-warning)]" />
          <span className="h-2.5 w-2.5 rounded-full bg-[var(--zf-success)]" />
          <span className="ml-3 rounded-full bg-[var(--zf-canvas)] px-3 py-0.5 text-xs text-[var(--zf-muted)]">fabric.example.internal/app</span>
        </div>
        <div className="flex" aria-hidden="true">
          <nav className="hidden w-44 shrink-0 border-r border-[var(--zf-hairline)] bg-[var(--zf-surface)] p-3 sm:block">
            {NAV.map((n, i) => (
              <div
                key={n}
                className="mb-1 rounded-md px-2 py-1.5 text-xs text-[var(--zf-secondary)]"
                style={i === 0 ? { background: 'var(--zf-active-tint)', boxShadow: 'inset 3px 0 0 var(--zf-brand)', fontWeight: 600, color: 'var(--zf-ink)' } : undefined}
              >
                {n}
              </div>
            ))}
          </nav>
          <div className="min-w-0 flex-1 space-y-3 p-4">
            <div className="grid grid-cols-3 gap-3">
              {([['VMs', fleet.total], ['Running', fleet.running], ['Alerts', 0]] as const).map(([label, n]) => (
                <div key={label} className="rounded-xl border border-[var(--zf-hairline)] bg-[var(--zf-surface)] p-3">
                  <div className="text-[11px] text-[var(--zf-muted)]">{label}</div>
                  <div className="text-2xl font-semibold tracking-[-0.03em] text-[var(--zf-ink)]"><AnimatedNumber value={n} /></div>
                </div>
              ))}
            </div>
            <div className="grid grid-cols-[1fr_auto] gap-3 sm:grid-cols-[1fr_9rem_9rem]">
              <div className="rounded-xl border border-[var(--zf-hairline)] bg-[var(--zf-surface)] p-3">
                <div className="mb-2 text-xs font-semibold text-[var(--zf-ink)]">Virtual machines</div>
                <div className="space-y-2">
                  {ROWS.map(([name, cpu, mem]) => (
                    <div key={name} className="flex items-center justify-between gap-2 text-xs text-[var(--zf-secondary)]">
                      <span className="w-20 truncate">{name}</span>
                      <LoadBar label={`${name} CPU`} value={cpu} />
                      <span className="hidden sm:inline"><LoadBar label={`${name} memory`} value={mem} /></span>
                    </div>
                  ))}
                </div>
              </div>
              <div className="hidden rounded-xl border border-[var(--zf-hairline)] bg-[var(--zf-surface)] p-2 sm:block"><LiveFabric vms={SAMPLE_FLEET} decorative /></div>
              <div className="flex items-center justify-center rounded-xl border border-[var(--zf-hairline)] bg-[var(--zf-surface)] p-2">
                <Donut
                  size={96}
                  thickness={11}
                  center={String(fleet.total)}
                  centerLabel="VMs"
                  segments={[
                    { label: 'Running', value: fleet.running, color: 'var(--zf-success)' },
                    { label: 'Paused', value: fleet.paused, color: 'var(--zf-warning)' },
                    { label: 'Stopped', value: fleet.stopped, color: 'var(--zf-muted)' },
                  ]}
                />
              </div>
            </div>
          </div>
        </div>
      </div>
      <figcaption className="mt-3 text-center text-xs text-[var(--zf-muted)]">Example data. The console you sign in to shows your own fleet.</figcaption>
    </figure>
  )
}
