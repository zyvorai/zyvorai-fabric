// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { Plus, Trash2, Download, Copy, Check, ChevronDown, ChevronUp, Layers } from 'lucide-react'
import { PageHeader } from '../components/ui'
import { AppleTerminalFrame } from '../components/AppleTerminalFrame'

interface MigrationEntry { id: number; name: string; source: string; target_format: string; cpus: number; memory: number; expanded: boolean }

let entryCounter = 0

export default function BatchMigrationBuilder() {
  const [entries, setEntries] = useState<MigrationEntry[]>([])
  const [copied, setCopied] = useState(false)

  const addEntry = () => setEntries(prev => [...prev, { id: ++entryCounter, name: '', source: '', target_format: 'qcow2', cpus: 2, memory: 2048, expanded: true }])
  const removeEntry = (id: number) => setEntries(prev => prev.filter(e => e.id !== id))
  const updateEntry = (id: number, field: string, value: string | number) => setEntries(prev => prev.map(e => e.id === id ? { ...e, [field]: value } : e))
  const toggleExpand = (id: number) => setEntries(prev => prev.map(e => e.id === id ? { ...e, expanded: !e.expanded } : e))

  const generateJSON = (): string => JSON.stringify({
    migrations: entries.map(e => ({ vm_name: e.name, source_path: e.source, target_format: e.target_format, cpus: e.cpus, memory_mb: e.memory }))
  }, null, 2)

  const json = generateJSON()
  const handleCopy = () => { navigator.clipboard.writeText(json); setCopied(true); setTimeout(() => setCopied(false), 2000) }
  const handleDownload = () => { const blob = new Blob([json], { type: 'application/json' }); const url = URL.createObjectURL(blob); const a = document.createElement('a'); a.href = url; a.download = 'batch-migration.json'; a.click(); URL.revokeObjectURL(url) }

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <PageHeader
        title="Batch Migration Builder"
        description="Configure multiple VM migrations with JSON export"
        actions={
          <button onClick={addEntry} title="Add VM" className="zf-btn zf-btn-primary">
            <Plus className="w-4 h-4" /> Add VM
          </button>
        }
      />

      {entries.length === 0 ? (
        <div className="bg-[var(--zf-canvas)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)]"><Layers className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">No VMs added. Click "Add VM" to start.</p></div>
      ) : (
        <div className="space-y-3">
          {entries.map((entry, idx) => (
            <div key={entry.id} className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
              <div className="flex items-center justify-between px-4 py-3 cursor-pointer hover:bg-black/[0.04] transition-colors" role="button" tabIndex={0} onClick={() => toggleExpand(entry.id)} onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleExpand(entry.id) } }}>
                <div className="flex items-center gap-3">
                  <span className="text-xs font-bold text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] w-6 h-6 rounded-full flex items-center justify-center">{idx + 1}</span>
                  <span className="text-sm font-medium text-[var(--zf-ink)]">{entry.name || `VM ${idx + 1}`}</span>
                  {entry.source && <span className="text-xs text-[var(--zf-muted)] font-mono truncate max-w-[200px]">{entry.source}</span>}
                </div>
                <div className="flex items-center gap-2">
                  <button onClick={(e) => { e.stopPropagation(); removeEntry(entry.id) }} className="p-1.5 text-[var(--zf-muted)] hover:text-red-600 hover:bg-red-50 rounded-lg transition-colors" title="Remove"><Trash2 className="w-4 h-4" /></button>
                  {entry.expanded ? <ChevronUp className="w-4 h-4 text-[var(--zf-muted)]" /> : <ChevronDown className="w-4 h-4 text-[var(--zf-muted)]" />}
                </div>
              </div>
              {entry.expanded && (
                <div className="px-4 pb-4 grid grid-cols-2 gap-3">
                  <div><label className="block text-xs text-[var(--zf-muted)] mb-1">VM Name</label><input type="text" value={entry.name} onChange={e => updateEntry(entry.id, 'name', e.target.value)} placeholder="my-vm" className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-[var(--zf-ink)]" /></div>
                  <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Source Path</label><input type="text" value={entry.source} onChange={e => updateEntry(entry.id, 'source', e.target.value)} placeholder="/path/to/disk.vmdk" className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-[var(--zf-ink)]" /></div>
                  <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Target Format</label><select value={entry.target_format} onChange={e => updateEntry(entry.id, 'target_format', e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"><option value="qcow2">QCOW2</option><option value="raw">RAW</option><option value="vmdk">VMDK</option></select></div>
                  <div className="grid grid-cols-2 gap-3">
                    <div><label className="block text-xs text-[var(--zf-muted)] mb-1">vCPUs</label><input type="number" value={entry.cpus} onChange={e => updateEntry(entry.id, 'cpus', Number(e.target.value))} min={1} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]" /></div>
                    <div><label className="block text-xs text-[var(--zf-muted)] mb-1">Memory MB</label><input type="number" value={entry.memory} onChange={e => updateEntry(entry.id, 'memory', Number(e.target.value))} min={256} step={256} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]" /></div>
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {entries.length > 0 && (
        <AppleTerminalFrame
          title={`batch-migration — JSON preview (${entries.length} VMs)`}
          bodyClassName="max-h-64 overflow-y-auto px-3 py-2 whitespace-pre"
          trailing={
            <div className="flex items-center gap-1 shrink-0">
              <button
                type="button"
                onClick={handleCopy}
                title="Copy JSON"
                className="inline-flex items-center gap-1 px-2 py-1 rounded text-xs text-white/60 hover:text-white hover:bg-white/10 transition-colors"
              >
                {copied ? <Check className="w-3 h-3 text-[#28c840]" /> : <Copy className="w-3 h-3" />}
                {copied ? 'Copied' : 'Copy'}
              </button>
              <button
                type="button"
                onClick={handleDownload}
                title="Download JSON"
                className="inline-flex items-center gap-1 px-2 py-1 rounded text-xs text-white/60 hover:text-white hover:bg-white/10 transition-colors"
              >
                <Download className="w-3 h-3" /> Download
              </button>
            </div>
          }
        >
          {json}
        </AppleTerminalFrame>
      )}
    </div>
  )
}
