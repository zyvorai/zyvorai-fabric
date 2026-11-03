// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import type { VmNetworkPolicy } from '../api/dataplane'
import { dryRunDataplane, explainDataplane } from '../api/dataplane'
import {
  TEMPLATES,
  applyControl,
  modeFromPolicy,
  type ControlAction,
  type EnforcementMode,
} from '../lib/policyControls'

interface Props {
  policy: VmNetworkPolicy
  disabled?: boolean
  /** When set, enables Explain / Dry-run Guard against the live API. */
  vmName?: string
  onChange: (next: VmNetworkPolicy) => void
  onApply?: (action: ControlAction, extra?: { cidr?: string; port?: string }) => Promise<void> | void
  onTemplate?: (id: string) => void
}

const MODES: { id: EnforcementMode; label: string; hint: string }[] = [
  { id: 'open', label: 'Open', hint: 'Default allow · not Cilium default-deny' },
  { id: 'audit', label: 'Audit', hint: 'Evaluate, do not drop (Cilium audit)' },
  { id: 'guard', label: 'Guard', hint: 'Enforce default-deny at the VM edge' },
]

export default function CiliumFlowControls({
  policy,
  disabled,
  vmName,
  onChange,
  onApply,
  onTemplate,
}: Props) {
  const mode = modeFromPolicy(policy)
  const [cidr, setCidr] = useState('')
  const [port, setPort] = useState('tcp/443')
  const [dest, setDest] = useState('1.1.1.1')
  const [dport, setDport] = useState('443')
  const [explainOut, setExplainOut] = useState('')
  const [dryOut, setDryOut] = useState('')
  const [busy, setBusy] = useState(false)

  const run = async (action: ControlAction, extra?: { cidr?: string; port?: string }) => {
    if (onApply) {
      setBusy(true)
      try {
        await onApply(action, extra)
      } finally {
        setBusy(false)
      }
      return
    }
    onChange(applyControl(policy, { action, ...extra }))
  }

  return (
    <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 space-y-3">
      <div>
        <p className="text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wide">
          Packet-flow control
        </p>
        <p className="text-xs text-[var(--zf-muted)]">
          Cilium-style Guard / Audit / Open on FluxVM edge policy. Does not write Cilium maps.
        </p>
      </div>
      <div className="flex flex-wrap gap-2">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            disabled={disabled || busy}
            title={m.hint}
            onClick={() => void run(m.id)}
            className={`px-3 py-1.5 text-sm rounded-lg border ${
              mode === m.id
                ? m.id === 'guard'
                  ? 'bg-red-50 text-red-800 border-red-200'
                  : m.id === 'audit'
                    ? 'bg-amber-50 text-amber-900 border-amber-200'
                    : 'bg-emerald-50 text-emerald-800 border-emerald-200'
                : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)] text-[var(--zf-ink)]'
            }`}
          >
            {m.label}
          </button>
        ))}
        <button
          type="button"
          disabled={disabled || busy}
          onClick={() => void run('invert')}
          className="px-3 py-1.5 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)]"
          title="Swap allow/deny CIDRs and flip default allow"
        >
          Invert
        </button>
      </div>
      <div className="flex flex-wrap gap-2 items-end">
        <label className="text-xs text-[var(--zf-muted)]">
          CIDR / host
          <input
            className="mt-1 block bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-2 py-1 text-sm"
            value={cidr}
            disabled={disabled}
            placeholder="1.1.1.1 or 10.0.0.0/8"
            onChange={(e) => setCidr(e.target.value)}
          />
        </label>
        <label className="text-xs text-[var(--zf-muted)]">
          Port
          <input
            className="mt-1 block bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-2 py-1 text-sm"
            value={port}
            disabled={disabled}
            placeholder="tcp/443"
            onChange={(e) => setPort(e.target.value)}
          />
        </label>
        <button
          type="button"
          disabled={disabled || busy || !cidr.trim()}
          onClick={() => void run('block', { cidr: cidr.trim() })}
          className="px-3 py-1.5 text-sm rounded-lg border border-red-200 bg-red-50 text-red-800"
        >
          Block
        </button>
        <button
          type="button"
          disabled={disabled || busy || !cidr.trim()}
          onClick={() => void run('allow', { cidr: cidr.trim(), port: port.trim() || undefined })}
          className="px-3 py-1.5 text-sm rounded-lg border border-emerald-200 bg-emerald-50 text-emerald-800"
        >
          Allow
        </button>
      </div>
      {onTemplate && (
        <div className="flex flex-wrap gap-2">
          {TEMPLATES.map((t) => (
            <button
              key={t.id}
              type="button"
              disabled={disabled}
              className="text-xs px-2 py-1 rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-canvas)]"
              onClick={() => onTemplate(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
      )}
      {vmName && (
        <>
          <div className="flex flex-wrap gap-2 items-end">
            <input
              className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-2 py-1 text-sm"
              value={dest}
              onChange={(e) => setDest(e.target.value)}
              placeholder="explain dest"
            />
            <input
              className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-2 py-1 text-sm w-20"
              value={dport}
              onChange={(e) => setDport(e.target.value)}
            />
            <button
              type="button"
              className="px-3 py-1.5 text-sm rounded-lg border border-[var(--zf-hairline)]"
              onClick={async () => {
                setBusy(true)
                try {
                  const r = await explainDataplane(vmName, dest, Number(dport) || 0, 'tcp')
                  setExplainOut(r.summary || `${r.verdict} ${r.reason}`)
                } catch (e) {
                  setExplainOut(String(e))
                } finally {
                  setBusy(false)
                }
              }}
            >
              Explain
            </button>
            <button
              type="button"
              className="px-3 py-1.5 text-sm rounded-lg border border-amber-200 bg-amber-50"
              onClick={async () => {
                setBusy(true)
                try {
                  const r = await dryRunDataplane(vmName)
                  setDryOut(`Guard dry-run: ${r.would_drop}/${r.examined} flows would drop`)
                } catch (e) {
                  setDryOut(String(e))
                } finally {
                  setBusy(false)
                }
              }}
            >
              Dry-run Guard
            </button>
          </div>
          {explainOut && <p className="text-xs font-mono text-[var(--zf-ink)]">{explainOut}</p>}
          {dryOut && <p className="text-xs font-mono text-[#92640a]">{dryOut}</p>}
        </>
      )}
      <p className="text-xs text-[var(--zf-muted)]">
        Mode: <strong>{mode}</strong>
        {policy.audit_mode ? ' · audit' : ''}
        {policy.default_allow ? ' · default-allow' : ' · default-deny'}
        {' · '}
        deny={policy.deny_cidrs?.length ?? 0} allow={policy.allow_cidrs.length}
      </p>
    </div>
  )
}
