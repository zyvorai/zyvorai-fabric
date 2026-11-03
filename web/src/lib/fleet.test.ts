// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { fabricNodes, fleetCounts, fleetSegments, fleetSummary, loadPercent, loadTone, stateTone, vmLoadFrom } from './fleet'

const vm = (name: string, state: string) => ({ name, state })

describe('stateTone', () => {
  it('draws only a failure as red; stopped is a normal state', () => {
    expect(stateTone('running')).toBe('proof')
    expect(stateTone('paused')).toBe('warn')
    expect(stateTone('failed')).toBe('danger')
    expect(stateTone('stopped')).toBe('neutral')
    expect(stateTone('who-knows')).toBe('neutral')
  })
})

describe('fleetCounts and fleetSummary', () => {
  const vms = [vm('a', 'running'), vm('b', 'running'), vm('c', 'paused'), vm('d', 'stopped'), vm('e', 'failed'), vm('f', 'starting')]
  it('counts every VM exactly once', () => {
    const c = fleetCounts(vms)
    expect(c).toEqual({ total: 6, running: 2, paused: 1, stopped: 1, failed: 1, other: 1 })
    expect(c.running + c.paused + c.stopped + c.failed + c.other).toBe(c.total)
  })
  it('says the same thing in words, and says so for an empty fleet', () => {
    expect(fleetSummary(fleetCounts(vms))).toBe('6 virtual machines: 2 running, 1 paused, 1 stopped, 1 failed, 1 in transition or unknown')
    expect(fleetSummary(fleetCounts([vm('a', 'running')]))).toBe('1 virtual machine: 1 running')
    expect(fleetSummary(fleetCounts([]))).toBe('No virtual machines yet')
  })
})

describe('fabricNodes', () => {
  const opts = { cx: 100, cy: 90, r: 60, max: 4 }
  it('puts the first node at the top and the rest evenly round the ring', () => {
    const { nodes } = fabricNodes([vm('a', 'running'), vm('b', 'running'), vm('c', 'running'), vm('d', 'running')], opts)
    expect(nodes[0].x).toBeCloseTo(100)
    expect(nodes[0].y).toBeCloseTo(30)
    expect(nodes[1].x).toBeCloseTo(160)
    expect(nodes[1].y).toBeCloseTo(90)
    for (const n of nodes) expect(Math.hypot(n.x - 100, n.y - 90)).toBeCloseTo(60)
  })
  it('keeps the running VMs when there are too many, and counts the rest', () => {
    const many = [vm('z-stopped', 'stopped'), vm('a-run', 'running'), vm('b-run', 'running'), vm('c-fail', 'failed'), vm('d-stop', 'stopped'), vm('e-pause', 'paused')]
    const { nodes, overflow } = fabricNodes(many, opts)
    expect(nodes.map((n) => n.name)).toEqual(['a-run', 'b-run', 'c-fail', 'e-pause'])
    expect(overflow).toBe(2)
  })
  it('handles an empty fleet with no nodes and no NaN', () => {
    expect(fabricNodes([], opts)).toEqual({ nodes: [], overflow: 0 })
  })
})

describe('loadPercent and loadTone', () => {
  it('clamps, rounds, and never turns a missing reading into zero', () => {
    expect(loadPercent(41.6)).toBe(42)
    expect(loadPercent(140)).toBe(100)
    expect(loadPercent(-3)).toBe(0)
    expect(loadPercent(undefined)).toBeNull()
    expect(loadPercent(Number.NaN)).toBeNull()
  })
  it('turns amber at 50 and red at 80', () => {
    expect([10, 49, 50, 79, 80, 100].map(loadTone)).toEqual(['proof', 'proof', 'warn', 'warn', 'danger', 'danger'])
  })
})

describe('vmLoadFrom', () => {
  it('sets memory use in bytes against the memory the VM was given, in MiB', () => {
    const l = vmLoadFrom({ memory: 4096 }, { cpu_usage: 37, memory_usage: 1024 * 1024 * 1024 })
    expect(l.cpu).toBe(37)
    expect(l.mem).toBeCloseTo(25)
  })
  it('has no memory share for a VM with no allocation, never zero or infinity', () => {
    expect(vmLoadFrom({ memory: 0 }, { cpu_usage: 5, memory_usage: 1000 }).mem).toBeNull()
  })
})

describe('fleetSegments', () => {
  it('lists only the states that have VMs, in a fixed order, with shares that sum to 1', () => {
    const segs = fleetSegments(fleetCounts([vm('a', 'running'), vm('b', 'running'), vm('c', 'failed'), vm('d', 'stopped')]))
    expect(segs.map((s) => s.key)).toEqual(['running', 'stopped', 'failed'])
    expect(segs.map((s) => s.count)).toEqual([2, 1, 1])
    expect(segs.reduce((a, s) => a + s.share, 0)).toBeCloseTo(1)
  })
  it('has no segments for an empty fleet', () => {
    expect(fleetSegments(fleetCounts([]))).toEqual([])
  })
  it('keeps a failed VM visible: the old summary line left failed VMs out entirely', () => {
    expect(fleetSegments(fleetCounts([vm('a', 'failed')])).map((s) => s.key)).toEqual(['failed'])
  })
})
