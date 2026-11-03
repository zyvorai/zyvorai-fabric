// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { Download, Copy, Check, ChevronUp, ChevronDown } from 'lucide-react'
import { PageHeader } from '../components/ui'
import { AppleTerminalFrame } from '../components/AppleTerminalFrame'

interface ConfigField { key: string; label: string; type: 'text' | 'number' | 'select' | 'checkbox'; options?: string[]; placeholder?: string }
interface ConfigSection { name: string; key: string; expanded: boolean; fields: ConfigField[] }

const SECTIONS: ConfigSection[] = [
  { name: 'VM Configuration', key: 'vm', expanded: true, fields: [
    { key: 'name', label: 'VM Name', type: 'text', placeholder: 'my-vm' },
    { key: 'cpus', label: 'vCPUs', type: 'number', placeholder: '2' },
    { key: 'memory', label: 'Memory (MB)', type: 'number', placeholder: '2048' },
    { key: 'image', label: 'Disk Image Path', type: 'text', placeholder: '/var/lib/zyvor-fabricd/images/disk.qcow2' },
    { key: 'firmware', label: 'Firmware', type: 'select', options: ['uefi', 'bios'] },
  ]},
  { name: 'Network', key: 'network', expanded: false, fields: [
    { key: 'network_type', label: 'Network Type', type: 'select', options: ['user', 'bridge', 'tap'] },
    { key: 'bridge', label: 'Bridge Name', type: 'text', placeholder: 'br0' },
    { key: 'mac', label: 'MAC Address', type: 'text', placeholder: 'auto' },
  ]},
  { name: 'Storage', key: 'storage', expanded: false, fields: [
    { key: 'disk_format', label: 'Disk Format', type: 'select', options: ['qcow2', 'raw', 'vmdk'] },
    { key: 'disk_size', label: 'Disk Size (GB)', type: 'number', placeholder: '20' },
    { key: 'readonly', label: 'Read-only Root', type: 'checkbox' },
  ]},
  { name: 'Advanced', key: 'advanced', expanded: false, fields: [
    { key: 'tpm', label: 'Enable TPM', type: 'checkbox' },
    { key: 'secure_boot', label: 'Secure Boot', type: 'checkbox' },
    { key: 'console', label: 'Console', type: 'select', options: ['serial', 'virtio', 'none'] },
    { key: 'cpu_mode', label: 'CPU Mode', type: 'select', options: ['host', 'max', 'qemu64'] },
  ]},
]

export default function ManifestBuilder() {
  const [config, setConfig] = useState<Record<string, string | number | boolean>>({})
  const [expanded, setExpanded] = useState<Record<string, boolean>>({ vm: true })
  const [copied, setCopied] = useState(false)

  const setField = (key: string, value: string | number | boolean) => setConfig(prev => ({ ...prev, [key]: value }))
  const toggleSection = (key: string) => setExpanded(prev => ({ ...prev, [key]: !prev[key] }))

  const generateYAML = (): string => {
    const lines: string[] = ['# Zyvor Fabric VM manifest', `# Generated: ${new Date().toISOString()}`, '']
    for (const section of SECTIONS) {
      const sectionValues = section.fields.filter(f => config[f.key] !== undefined && config[f.key] !== '' && config[f.key] !== false)
      if (sectionValues.length === 0) continue
      lines.push(`# ${section.name}`)
      for (const field of sectionValues) {
        const val = config[field.key]
        if (typeof val === 'boolean') lines.push(`${field.key}: ${val}`)
        else if (typeof val === 'number') lines.push(`${field.key}: ${val}`)
        else lines.push(`${field.key}: "${val}"`)
      }
      lines.push('')
    }
    return lines.join('\n')
  }

  const yaml = generateYAML()

  const handleCopy = () => { navigator.clipboard.writeText(yaml); setCopied(true); setTimeout(() => setCopied(false), 2000) }
  const handleDownload = () => {
    const blob = new Blob([yaml], { type: 'text/yaml' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url; a.download = `${config.name || 'vm'}-manifest.yaml`; a.click()
    URL.revokeObjectURL(url)
  }

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <PageHeader title="Manifest Builder" description="Build VM configuration manifests with YAML export" />

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Form */}
        <div className="space-y-3">
          {SECTIONS.map(section => (
            <div key={section.key} className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
              <button onClick={() => toggleSection(section.key)} className="w-full flex items-center justify-between px-4 py-3 text-sm font-semibold text-[var(--zf-ink)] hover:bg-black/[0.04] transition-colors">
                {section.name}
                {expanded[section.key] ? <ChevronUp className="w-4 h-4 text-[var(--zf-muted)]" /> : <ChevronDown className="w-4 h-4 text-[var(--zf-muted)]" />}
              </button>
              {expanded[section.key] && (
                <div className="px-4 pb-4 space-y-3">
                  {section.fields.map(field => (
                    <div key={field.key}>
                      {field.type === 'checkbox' ? (
                        <label className="flex items-center gap-3 cursor-pointer">
                          <input type="checkbox" checked={!!config[field.key]} onChange={(e) => setField(field.key, e.target.checked)} className="rounded border-[var(--zf-hairline)]" />
                          <span className="text-sm text-[var(--zf-ink)]">{field.label}</span>
                        </label>
                      ) : (
                        <>
                          <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1">{field.label}</label>
                          {field.type === 'select' ? (
                            <select value={String(config[field.key] || '')} onChange={(e) => setField(field.key, e.target.value)} aria-label={field.label}
                              className="input-field text-sm">
                              <option value="">Select...</option>
                              {field.options?.map(opt => <option key={opt} value={opt}>{opt}</option>)}
                            </select>
                          ) : (
                            <input type={field.type} value={String(config[field.key] || '')} onChange={(e) => setField(field.key, field.type === 'number' ? Number(e.target.value) : e.target.value)}
                              placeholder={field.placeholder} aria-label={field.label}
                              className="input-field text-sm" />
                          )}
                        </>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>

        {/* YAML preview */}
        <div className="space-y-3">
          <AppleTerminalFrame
            title="manifest — YAML preview"
            bodyClassName="max-h-[500px] overflow-y-auto px-3 py-2 whitespace-pre"
            trailing={
              <div className="flex items-center gap-1 shrink-0">
                <button
                  type="button"
                  onClick={handleCopy}
                  title="Copy to clipboard"
                  className="inline-flex items-center gap-1 px-2 py-1 rounded text-xs text-white/60 hover:text-white hover:bg-white/10 transition-colors"
                >
                  {copied ? <Check className="w-3 h-3 text-[#28c840]" /> : <Copy className="w-3 h-3" />}
                  {copied ? 'Copied' : 'Copy'}
                </button>
                <button
                  type="button"
                  onClick={handleDownload}
                  title="Download YAML"
                  className="inline-flex items-center gap-1 px-2 py-1 rounded text-xs text-white/60 hover:text-white hover:bg-white/10 transition-colors"
                >
                  <Download className="w-3 h-3" /> Download
                </button>
              </div>
            }
          >
            {yaml}
          </AppleTerminalFrame>
        </div>
      </div>
    </div>
  )
}
