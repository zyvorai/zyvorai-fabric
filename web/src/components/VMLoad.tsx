// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { LoadBar } from './LoadBar'

/** A running VM's CPU and memory as two bars with their names beside them. Nothing for a VM that is not running or has no reading
    yet: a stopped VM has no load, and "0%" would claim it was measured. */
export function VMLoad({
  name,
  state,
  load,
}: {
  name: string
  state: string
  load: { cpu: number; mem: number | null } | undefined
}) {
  if (state !== 'running' || !load) return null
  return (
    <div className="space-y-1" data-testid="vm-load">
      <div className="flex items-center gap-2"><span className="w-7 text-xs text-[var(--zf-muted)]">CPU</span><LoadBar label={`${name} CPU`} value={load.cpu} /></div>
      <div className="flex items-center gap-2"><span className="w-7 text-xs text-[var(--zf-muted)]">Mem</span><LoadBar label={`${name} memory`} value={load.mem} /></div>
    </div>
  )
}
