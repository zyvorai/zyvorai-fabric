// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import type { FlowTheme, PacketFlowView } from '../lib/packetflow'
import { filterViews, verdictClass } from '../lib/packetflow'

interface Props {
  views: PacketFlowView[]
  loading?: boolean
  emptyHint?: string
  onReload?: () => void
  onBlockDest?: (destIp: string, destPort: number, proto: string) => void
}

export default function PacketFlowPanel({ views, loading, emptyHint, onReload, onBlockDest }: Props) {
  const [theme, setTheme] = useState<FlowTheme>('color')
  const [detail, setDetail] = useState<'path' | 'table'>('path')
  const [verdict, setVerdict] = useState('all')
  const [protocol, setProtocol] = useState('all')
  const filtered = useMemo(
    () => filterViews(views, verdict, protocol),
    [views, verdict, protocol],
  )
  const dark = theme === 'color'

  return (
    <div
      className={`rounded-xl border overflow-hidden ${
        dark ? 'bg-[#0b1020] border-[#243056] text-[#e8eefc]' : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)] text-[var(--zf-ink)]'
      }`}
    >
      <div className={`flex flex-wrap gap-2 items-center p-3 border-b ${dark ? 'border-[#243056]' : 'border-[var(--zf-hairline)]'}`}>
        <label className="text-xs">
          Theme
          <select
            className={`ml-1 rounded border px-2 py-1 text-sm ${dark ? 'bg-[#121a30] border-[#243056]' : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)]'}`}
            value={theme}
            onChange={(e) => setTheme(e.target.value as FlowTheme)}
          >
            <option value="color">Colorful</option>
            <option value="normal">Normal</option>
          </select>
        </label>
        <label className="text-xs">
          Verdict
          <select
            className={`ml-1 rounded border px-2 py-1 text-sm ${dark ? 'bg-[#121a30] border-[#243056]' : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)]'}`}
            value={verdict}
            onChange={(e) => setVerdict(e.target.value)}
          >
            <option value="all">all</option>
            <option value="FORWARDED">FORWARDED</option>
            <option value="DROPPED">DROPPED</option>
            <option value="AUDIT">AUDIT</option>
          </select>
        </label>
        <label className="text-xs">
          Proto
          <select
            className={`ml-1 rounded border px-2 py-1 text-sm ${dark ? 'bg-[#121a30] border-[#243056]' : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)]'}`}
            value={protocol}
            onChange={(e) => setProtocol(e.target.value)}
          >
            <option value="all">all</option>
            <option value="tcp">tcp</option>
            <option value="udp">udp</option>
            <option value="icmp">icmp</option>
          </select>
        </label>
        <label className="text-xs">
          Detail
          <select
            className={`ml-1 rounded border px-2 py-1 text-sm ${dark ? 'bg-[#121a30] border-[#243056]' : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)]'}`}
            value={detail}
            onChange={(e) => setDetail(e.target.value as 'path' | 'table')}
          >
            <option value="path">packet path</option>
            <option value="table">table only</option>
          </select>
        </label>
        {onReload && (
          <button
            type="button"
            onClick={onReload}
            className={`ml-auto text-sm px-3 py-1 rounded border ${dark ? 'border-[#243056]' : 'border-[var(--zf-hairline)]'}`}
          >
            Reload
          </button>
        )}
      </div>
      {loading ? (
        <p className="p-6 text-sm opacity-70">Loading flows…</p>
      ) : filtered.length === 0 ? (
        <p className="p-6 text-sm opacity-70">{emptyHint ?? '(no flows)'}</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className={`text-left text-xs uppercase ${dark ? 'text-[#8b9bb8]' : 'text-[var(--zf-muted)]'}`}>
                <th className="py-2 px-3">Verdict</th>
                <th className="py-2 px-3">Dir</th>
                <th className="py-2 px-3">Tuple / path</th>
                <th className="py-2 px-3">IDs</th>
                <th className="py-2 px-3">Counters</th>
                <th className="py-2 px-3">VM</th>
                {onBlockDest && <th className="py-2 px-3">Control</th>}
              </tr>
            </thead>
            <tbody>
              {filtered.map((f, i) => (
                <tr key={`${f.summary}-${i}`} className={dark ? 'border-t border-[#243056]' : 'border-t border-[var(--zf-hairline)]'}>
                  <td className={`py-2 px-3 ${verdictClass(f.verdict, theme)}`}>{f.verdict}</td>
                  <td className="py-2 px-3">{f.direction}</td>
                  <td className="py-2 px-3 font-mono text-xs">
                    {f.protocol}/{f.destinationPort} {f.sourceIp}:{f.sourcePort} → {f.destinationIp}:
                    {f.destinationPort}
                    {f.dropReason && (
                      <div className={verdictClass('DROPPED', theme)}>drop_reason={f.dropReason}</div>
                    )}
                    {detail === 'path' && (
                      <div className={`mt-2 whitespace-pre-wrap ${dark ? 'text-[#60a5fa]' : 'text-[#155eef]'}`}>
                        {f.hops.map((h, idx) => {
                          const last = idx === f.hops.length - 1
                          return `${last ? '└─' : '├─'} [${h.index}] ${h.name} (${h.role})  ${h.detail}${last ? '' : '\n│'}`
                        }).join('\n')}
                      </div>
                    )}
                  </td>
                  <td className="py-2 px-3 font-mono text-xs">
                    {f.source.identity}→{f.destination.identity}
                  </td>
                  <td className="py-2 px-3">
                    {f.packets}p / {f.bytes}B
                  </td>
                  <td className="py-2 px-3">
                    {f.vmName}
                    <div className="text-xs opacity-60">{f.source.labels.join(' ')}</div>
                  </td>
                  {onBlockDest && (
                    <td className="py-2 px-3">
                      <button
                        type="button"
                        className="text-xs px-2 py-1 rounded border border-red-200 bg-red-50 text-red-800"
                        onClick={() => onBlockDest(f.destinationIp, f.destinationPort, f.protocol)}
                      >
                        Block
                      </button>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
