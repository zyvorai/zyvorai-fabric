// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react'
import { Plus, Trash2, TrendingUp, Clock } from 'lucide-react'
import {
  listPolicies,
  listScaleEvents,
  createPolicy,
  deletePolicy,
  type ScalingPolicy,
  type ScaleEvent,
} from '../api/autoscale'
import { listVMs, type VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader, EmptyState } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'
import { usePermissions } from '../hooks/usePermissions'
import ReadOnlyNotice from '../components/ReadOnlyNotice'

export default function Autoscale() {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const { confirmState, confirm, cancel } = useConfirm()
  const [policies, setPolicies] = useState<ScalingPolicy[]>([])
  const [events, setEvents] = useState<ScaleEvent[]>([])
  const [vms, setVms] = useState<VM[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load autoscale policies')
  const [showCreate, setShowCreate] = useState(false)

  const loadData = useCallback(() => {
    return run(async () => {
      const [p, e, v] = await Promise.all([listPolicies(), listScaleEvents(), listVMs()])
      setPolicies(p)
      setEvents(e)
      setVms(v)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDelete = async (vmName: string) => {
    const ok = await confirm('Delete Policy', `Remove autoscale policy for '${vmName}'?`, {
      variant: 'danger',
      confirmLabel: 'Delete',
    })
    if (!ok) return
    try {
      await deletePolicy(vmName)
      toast.success('Policy deleted')
      void loadData()
    } catch (e) {
      toastFailure(toast, 'Failed to delete policy', e)
    }
  }

  return (
    <div className="space-y-6">
      {!canWrite && <ReadOnlyNotice />}

      <PageHeader
        title="Autoscale"
        description="CPU and memory scaling policies per VM"
        onRefresh={() => void loadData()}
        refreshing={loading}
        actions={
          canWrite ? (
            <button
              onClick={() => setShowCreate(true)}
              className="zf-btn zf-btn-primary"
            >
              <Plus className="w-4 h-4" />
              Create Policy
            </button>
          ) : undefined
        }
      />

      <PageLoadBanner title="Could not load autoscale policies" headline={loadError} onRetry={() => void loadData()} domain="vm" />

      {!loadError && policies.length === 0 && !loading && (
        <EmptyState
          icon={<TrendingUp className="w-12 h-12" />}
          title="No autoscale policies"
          description="Create a policy to scale VM resources based on CPU or memory thresholds."
          action={
            canWrite ? (
              <button
                onClick={() => setShowCreate(true)}
                className="zf-btn zf-btn-primary zf-btn-sm"
              >
                Create Policy
              </button>
            ) : undefined
          }
        />
      )}

      {policies.length > 0 && (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-[var(--zf-surface)] text-[var(--zf-muted)] text-left">
              <tr>
                <th className="p-4">VM</th>
                <th className="p-4">CPU range</th>
                <th className="p-4">Memory range</th>
                <th className="p-4">Thresholds</th>
                <th className="p-4">Cooldown</th>
                <th className="p-4" />
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {policies.map((p) => (
                <tr key={p.vm_name} className="hover:bg-[var(--zf-surface)]">
                  <td className="p-4 font-medium text-[var(--zf-ink)]">{p.vm_name}</td>
                  <td className="p-4 text-[var(--zf-muted)]">{p.min_cpus}–{p.max_cpus} vCPU</td>
                  <td className="p-4 text-[var(--zf-muted)]">{p.min_memory_mb}–{p.max_memory_mb} MB</td>
                  <td className="p-4 text-[var(--zf-muted)] text-xs">
                    CPU ↑{p.cpu_scale_up_threshold ?? '—'}% ↓{p.cpu_scale_down_threshold ?? '—'}%
                  </td>
                  <td className="p-4 text-[var(--zf-muted)]">{p.cooldown_secs}s</td>
                  <td className="p-4 text-right">
                    {canWrite && (
                      <button
                        onClick={() => void handleDelete(p.vm_name)}
                        className="p-2 text-red-600 hover:bg-red-50 rounded-lg"
                        title="Delete policy"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {events.length > 0 && (
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5">
          <h3 className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-4">
            <Clock className="w-4 h-4 text-[var(--zf-muted)]" />
            Recent scale events
          </h3>
          <div className="space-y-2 max-h-64 overflow-y-auto">
            {events.slice(0, 20).map((ev) => (
              <div key={ev.id} className="flex justify-between gap-4 text-xs border-b border-[var(--zf-hairline)]/60 pb-2">
                <span className="text-[var(--zf-ink)]">{ev.vm_name} · {ev.action} {ev.resource}</span>
                <span className="text-[var(--zf-muted)] shrink-0">{new Date(ev.timestamp).toLocaleString()}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {showCreate && canWrite && (
        <CreatePolicyDialog
          vms={vms}
          existing={policies.map((p) => p.vm_name)}
          onClose={() => setShowCreate(false)}
          onCreated={() => {
            setShowCreate(false)
            void loadData()
          }}
        />
      )}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel}
          variant={confirmState.variant}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}

function CreatePolicyDialog({
  vms,
  existing,
  onClose,
  onCreated,
}: {
  vms: VM[]
  existing: string[]
  onClose: () => void
  onCreated: () => void
}) {
  const toast = useToastContext()
  const available = vms.filter((v) => !existing.includes(v.name))
  const [vmName, setVmName] = useState(available[0]?.name ?? '')
  const [minCpus, setMinCpus] = useState(1)
  const [maxCpus, setMaxCpus] = useState(4)
  const [minMem, setMinMem] = useState(512)
  const [maxMem, setMaxMem] = useState(4096)
  const [cpuUp, setCpuUp] = useState(80)
  const [cpuDown, setCpuDown] = useState(20)
  const [cooldown, setCooldown] = useState(300)
  const [saving, setSaving] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!vmName) return
    setSaving(true)
    try {
      await createPolicy({
        vm_name: vmName,
        min_cpus: minCpus,
        max_cpus: maxCpus,
        min_memory_mb: minMem,
        max_memory_mb: maxMem,
        cpu_scale_up_threshold: cpuUp,
        cpu_scale_down_threshold: cpuDown,
        cooldown_secs: cooldown,
      })
      toast.success(`Autoscale policy created for '${vmName}'`)
      onCreated()
    } catch (err) {
      toastFailure(toast, 'Failed to create policy', err)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-lg">
        <h2 className="text-xl font-bold mb-4">Create autoscale policy</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-xs text-[var(--zf-muted)] mb-1">VM</label>
            <select
              value={vmName}
              onChange={(e) => setVmName(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)]"
              required
            >
              {available.length === 0 && <option value="">No VMs available</option>}
              {available.map((v) => (
                <option key={v.name} value={v.name}>{v.name}</option>
              ))}
            </select>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Min vCPUs" value={minCpus} onChange={setMinCpus} />
            <Field label="Max vCPUs" value={maxCpus} onChange={setMaxCpus} />
            <Field label="Min memory (MB)" value={minMem} onChange={setMinMem} />
            <Field label="Max memory (MB)" value={maxMem} onChange={setMaxMem} />
            <Field label="CPU scale-up %" value={cpuUp} onChange={setCpuUp} />
            <Field label="CPU scale-down %" value={cpuDown} onChange={setCpuDown} />
          </div>
          <Field label="Cooldown (seconds)" value={cooldown} onChange={setCooldown} />
          <div className="flex justify-end gap-2 pt-2">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost">Cancel</button>
            <button
              type="submit"
              disabled={saving || !vmName}
              className="zf-btn zf-btn-primary"
            >
              Create
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function Field({
  label,
  value,
  onChange,
}: {
  label: string
  value: number
  onChange: (n: number) => void
}) {
  return (
    <div>
      <label className="block text-xs text-[var(--zf-muted)] mb-1">{label}</label>
      <input
        type="number"
        value={value}
        onChange={(e) => onChange(parseInt(e.target.value) || 0)}
        className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)]"
      />
    </div>
  )
}
