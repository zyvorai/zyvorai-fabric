// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../../components/Skeleton'
import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  AlertCircle,
  Loader2,
  Plus,
  RefreshCw,
  Shield,
  Trash2,
  X,
} from 'lucide-react'
import {
  DataplaneStats,
  DataplaneStatus,
  FlowRecord,
  DropReasonRecord,
  VmNetworkPolicy,
  emptyPolicy,
  getDataplaneEffective,
  getDataplaneFlows,
  getDataplaneDropReasons,
  getDataplanePolicy,
  getDataplaneStats,
  getDataplaneStatus,
  applyDataplaneControl,
  setDataplanePolicy,
} from '../../api/dataplane'
import { useToastContext } from '../../contexts/ToastContext'
import { toastFailure } from '../../utils/toastError'
import { formatUserError } from '../../utils/apiError'
import { usePermissions } from '../../hooks/usePermissions'
import { StatusBadge } from '../../components/ui'
import SubsystemBanner from '../../components/SubsystemBanner'
import { TerminalTextarea } from '../../components/AppleTerminalFrame'
import PacketFlowPanel from '../../components/PacketFlowPanel'
import CiliumFlowControls from '../../components/CiliumFlowControls'
import { fromRawFlow } from '../../lib/packetflow'
import { templatePolicy, type ControlAction } from '../../lib/policyControls'

type PanelTab = 'status' | 'policy' | 'effective' | 'stats' | 'flows' | 'drops'

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MiB`
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GiB`
}

const inputCls =
  'w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50'
const labelCls = 'block text-xs text-[var(--zf-muted)] mb-1'

export default function DataplanePanel({ vmName }: { vmName: string }) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [tab, setTab] = useState<PanelTab>('status')
  const [status, setStatus] = useState<DataplaneStatus | null>(null)
  const [stats, setStats] = useState<DataplaneStats | null>(null)
  const [flows, setFlows] = useState<FlowRecord[]>([])
  const [drops, setDrops] = useState<DropReasonRecord[]>([])
  const [policy, setPolicy] = useState<VmNetworkPolicy>(emptyPolicy())
  const [dirty, setDirty] = useState(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [flowLimit, setFlowLimit] = useState(100)
  const [autoRefresh, setAutoRefresh] = useState(false)
  const [cidrDraft, setCidrDraft] = useState('')
  const [denyDraft, setDenyDraft] = useState('')
  const [portDraft, setPortDraft] = useState('')
  const [groupDraft, setGroupDraft] = useState('')
  const [labelDraft, setLabelDraft] = useState('')
  const [fqdnDraft, setFqdnDraft] = useState('')
  const [entityDraft, setEntityDraft] = useState('')
  const [showJson, setShowJson] = useState(false)
  const [jsonText, setJsonText] = useState('')
  const [effective, setEffective] = useState<Record<string, unknown> | null>(null)

  const load = useCallback(async () => {
    setError(null)
    try {
      const [st, pol, stStats, fl, dr, eff] = await Promise.all([
        getDataplaneStatus(vmName),
        getDataplanePolicy(vmName).catch(() => null),
        getDataplaneStats(vmName).catch(() => null),
        getDataplaneFlows(vmName, flowLimit).catch(() => ({ items: [] as FlowRecord[] })),
        getDataplaneDropReasons(vmName, flowLimit).catch(() => ({
          items: [] as DropReasonRecord[],
        })),
        getDataplaneEffective(vmName).catch(() => null),
      ])
      setStatus(st)
      setStats(stStats)
      setFlows(fl.items)
      setDrops(dr.items)
      setEffective(eff)
      if (!dirty) {
        const p = { ...emptyPolicy(), ...(pol ?? st.policy ?? {}) }
        setPolicy(p)
        setJsonText(JSON.stringify(p, null, 2))
      }
    } catch (err) {
      setError(formatUserError(err))
      setStatus(null)
    } finally {
      setLoading(false)
    }
  }, [vmName, flowLimit, dirty])

  useEffect(() => {
    setLoading(true)
    void load()
  }, [vmName]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (tab === 'flows') void load()
  }, [flowLimit]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!autoRefresh) return
    const id = setInterval(() => {
      void load()
    }, 5000)
    return () => clearInterval(id)
  }, [autoRefresh, load])

  const updatePolicy = (patch: Partial<VmNetworkPolicy>) => {
    setPolicy((prev) => {
      const next = { ...prev, ...patch }
      setJsonText(JSON.stringify(next, null, 2))
      return next
    })
    setDirty(true)
  }

  const addCidr = () => {
    const v = cidrDraft.trim()
    if (!v) return
    if (policy.allow_cidrs.includes(v)) {
      toast.error('CIDR already listed')
      return
    }
    updatePolicy({ allow_cidrs: [...policy.allow_cidrs, v] })
    setCidrDraft('')
  }

  const removeCidr = (cidr: string) => {
    updatePolicy({ allow_cidrs: policy.allow_cidrs.filter((c) => c !== cidr) })
  }

  const addPort = () => {
    const v = portDraft.trim()
    if (!v) return
    if (!/^(tcp|udp)\/\d{1,5}(-\d{1,5})?$/i.test(v)) {
      toast.error('Ports must look like tcp/443 or udp/53')
      return
    }
    if (policy.allow_ports.includes(v)) {
      toast.error('Port already listed')
      return
    }
    updatePolicy({ allow_ports: [...policy.allow_ports, v] })
    setPortDraft('')
  }

  const removePort = (port: string) => {
    updatePolicy({ allow_ports: policy.allow_ports.filter((p) => p !== port) })
  }

  const addTagged = (
    field: 'deny_cidrs' | 'groups' | 'labels' | 'allow_fqdns' | 'entities',
    draft: string,
    setDraft: (v: string) => void,
  ) => {
    const v = draft.trim()
    if (!v) return
    const cur = policy[field] ?? []
    if (cur.includes(v)) {
      toast.error('Already listed')
      return
    }
    updatePolicy({ [field]: [...cur, v] })
    setDraft('')
  }

  const removeTagged = (
    field: 'deny_cidrs' | 'groups' | 'labels' | 'allow_fqdns' | 'entities',
    value: string,
  ) => {
    updatePolicy({ [field]: (policy[field] ?? []).filter((x) => x !== value) })
  }

  const applyPreset = (kind: 'open' | 'deny' | 'web') => {
    if (kind === 'open') {
      updatePolicy({
        default_allow: true,
        allow_cidrs: [],
        allow_ports: [],
        max_egress_mbps: null,
        max_egress_pps: null,
        sample_rate: policy.sample_rate,
      })
    } else if (kind === 'deny') {
      updatePolicy({
        default_allow: false,
        allow_cidrs: [],
        allow_ports: [],
        max_egress_mbps: null,
        max_egress_pps: null,
        sample_rate: policy.sample_rate || 1,
      })
    } else {
      updatePolicy({
        default_allow: false,
        allow_cidrs: ['0.0.0.0/0', '::/0'],
        allow_ports: ['tcp/80', 'tcp/443', 'udp/53'],
        max_egress_mbps: 100,
        max_egress_pps: 10000,
        sample_rate: Math.max(policy.sample_rate, 1),
      })
    }
  }

  const resetPolicy = () => {
    const p = status?.policy ?? emptyPolicy()
    setPolicy(p)
    setJsonText(JSON.stringify(p, null, 2))
    setDirty(false)
  }

  const savePolicy = async () => {
    let toSave = policy
    if (showJson) {
      try {
        toSave = JSON.parse(jsonText) as VmNetworkPolicy
      } catch {
        toast.error('Policy JSON is invalid')
        return
      }
    }
    setSaving(true)
    try {
      const saved = await setDataplanePolicy(vmName, toSave)
      setPolicy(saved)
      setJsonText(JSON.stringify(saved, null, 2))
      setDirty(false)
      toast.success('Dataplane policy saved')
      await load()
    } catch (err) {
      toastFailure(toast, 'Failed to save dataplane policy', err)
    } finally {
      setSaving(false)
    }
  }

  const applyFlowControl = async (
    action: ControlAction,
    extra?: { cidr?: string; port?: string },
  ) => {
    setSaving(true)
    try {
      const saved = await applyDataplaneControl(vmName, { action, ...extra })
      setPolicy(saved)
      setJsonText(JSON.stringify(saved, null, 2))
      setDirty(false)
      toast.success(`Dataplane ${action} applied`)
      await load()
    } catch (err) {
      toastFailure(toast, `Failed to apply ${action}`, err)
    } finally {
      setSaving(false)
    }
  }

  const tabs: { id: PanelTab; label: string }[] = [
    { id: 'status', label: 'Status' },
    { id: 'policy', label: 'Policy' },
    { id: 'effective', label: 'Effective' },
    { id: 'stats', label: 'Stats' },
    { id: 'flows', label: 'Flows' },
    { id: 'drops', label: 'Drops' },
  ]

  const dropRate = useMemo(() => {
    if (!stats) return null
    const total = stats.allowed_packets + stats.dropped_packets
    if (total === 0) return '0%'
    return `${((stats.dropped_packets / total) * 100).toFixed(1)}%`
  }, [stats])

  if (loading) return <PanelLoading label="Loading VM edge dataplane…" />

  if (error) {
    return (
      <div className="space-y-3">
        <SubsystemBanner subsystem="vm_dataplane" title="VM edge dataplane" />
        <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-6 space-y-3">
        <div className="flex items-center gap-2 text-red-600">
          <AlertCircle className="w-4 h-4" />
          <span className="text-sm font-medium">VM edge dataplane (FluxVM)</span>
        </div>
        <p className="text-sm text-[var(--zf-muted)]">{error}</p>
        <p className="text-xs text-[var(--zf-muted)]">
          Requires FluxVM with <code className="font-mono">[sandbox.dataplane] mode = &quot;ebpf&quot;</code>.
          Separate from Security → Network Policies (Fabric SDN).
        </p>
        <button type="button" onClick={() => { setLoading(true); void load() }} className="zf-btn zf-btn-ghost zf-btn-sm">
          <RefreshCw className="w-3.5 h-3.5" /> Retry
        </button>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <SubsystemBanner subsystem="vm_dataplane" title="VM edge dataplane" />
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Shield className="w-4 h-4 text-[var(--zf-link)]" />
          <div>
            <h3 className="text-sm font-semibold text-[var(--zf-ink)]">VM edge dataplane (FluxVM)</h3>
            <p className="text-xs text-[var(--zf-muted)]">
              Per-VM TC/eBPF policy. Schema version is whatever FluxVM reports (11 on a current attach) — not Fabric SDN.
              Service Fabric is not attached to bridge-less direct taps.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-1.5 text-xs text-[var(--zf-muted)]">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
            />
            Auto-refresh
          </label>
          <button
            type="button"
            onClick={() => void load()}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)] text-[var(--zf-ink)] hover:bg-[var(--zf-canvas)]"
          >
            <RefreshCw className="w-3.5 h-3.5" /> Refresh
          </button>
        </div>
      </div>

      <div className="flex gap-1 border-b border-[var(--zf-hairline)]">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => setTab(t.id)}
            className={`px-3 py-2 text-sm font-medium relative ${
              tab === t.id ? 'text-[var(--zf-link)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
            }`}
          >
            {t.label}
            {t.id === 'policy' && dirty && (
              <span className="ml-1 inline-block w-1.5 h-1.5 rounded-full bg-amber-500" />
            )}
            {tab === t.id && (
              <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-[var(--zf-link-fill)] rounded-full" />
            )}
          </button>
        ))}
      </div>

      {tab === 'status' && status && (
        <div className="space-y-4">
          <div className="flex flex-wrap gap-2">
            <StatusBadge status={status.attached ? 'running' : 'stopped'} />
            <span className="text-xs px-2 py-1 rounded-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-ink)]">
              mode: {status.mode}
            </span>
            {status.policy_synced ? (
              <span className="text-xs px-2 py-1 rounded-full bg-emerald-50 text-emerald-700 border border-emerald-200">
                policy synced
              </span>
            ) : (
              <span className="text-xs px-2 py-1 rounded-full bg-amber-50 text-amber-800 border border-amber-200">
                policy not synced
              </span>
            )}
            {!status.schema_compatible && (
              <span className="text-xs px-2 py-1 rounded-full bg-red-50 text-red-700 border border-red-200">
                schema incompatible
              </span>
            )}
            {status.pod_ingress_required && (
              <span
                className={`text-xs px-2 py-1 rounded-full border ${
                  status.pod_ingress_attached
                    ? 'bg-emerald-50 text-emerald-700 border-emerald-200'
                    : 'bg-amber-50 text-amber-800 border-amber-200'
                }`}
              >
                pod ingress {status.pod_ingress_attached ? 'attached' : 'required'}
              </span>
            )}
          </div>
          <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 grid grid-cols-2 sm:grid-cols-4 gap-4 text-sm">
            <Stat label="Attached" value={status.attached ? 'yes' : 'no'} />
            <Stat label="Mode" value={status.mode} />
            <Stat label="Schema version" value={status.schema_version?.toString() ?? '—'} />
            <Stat label="Schema compatible" value={status.schema_compatible ? 'yes' : 'no'} />
            <Stat label="Policy synced" value={status.policy_synced ? 'yes' : 'no'} />
            <Stat label="Required" value={status.required ? 'yes' : 'no'} />
            <Stat label="Interface" value={status.interface ?? '—'} />
            <Stat label="Identity" value={String(status.identity)} />
            <Stat
              label="Pod ingress required"
              value={status.pod_ingress_required ? 'yes' : 'no'}
            />
            <Stat
              label="Pod ingress attached"
              value={status.pod_ingress_attached ? 'yes' : 'no'}
            />
            <Stat label="Pin dir" value={status.pin_dir ?? '—'} mono />
          </div>
          {!status.attached && status.mode === 'legacy' && (
            <p className="text-sm text-amber-800 bg-amber-50 border border-amber-200 rounded-lg px-3 py-2">
              Dataplane is in legacy (nftables) mode. Set FluxVM{' '}
              <code className="font-mono text-xs">[sandbox.dataplane] mode = &quot;ebpf&quot;</code> and
              restart FluxVM to attach the TC program.
            </p>
          )}
          {!status.attached && status.mode !== 'legacy' && (
            <p className="text-sm text-amber-800 bg-amber-50 border border-amber-200 rounded-lg px-3 py-2">
              eBPF mode configured but not attached yet — start a bridged (netns) VM or check BPF object
              path / memlock / bpffs mounts.
            </p>
          )}
          <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 space-y-2">
            <p className="text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wide">
              Active policy snapshot
            </p>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-sm">
              <Stat
                label="Default allow"
                value={(status.policy ?? policy).default_allow ? 'yes' : 'no'}
              />
              <Stat
                label="Allow CIDRs"
                value={String((status.policy ?? policy).allow_cidrs.length)}
              />
              <Stat
                label="Allow ports"
                value={String((status.policy ?? policy).allow_ports.length)}
              />
              <Stat
                label="Sample rate"
                value={String((status.policy ?? policy).sample_rate)}
              />
              <Stat
                label="Max Mbps"
                value={(status.policy ?? policy).max_egress_mbps?.toString() ?? 'unlimited'}
              />
              <Stat
                label="Max PPS"
                value={(status.policy ?? policy).max_egress_pps?.toString() ?? 'unlimited'}
              />
              <Stat
                label="Allow ICMP"
                value={(status.policy ?? policy).allow_icmp ? 'yes' : 'no'}
              />
              <Stat
                label="Groups"
                value={String((status.policy ?? policy).groups?.length ?? 0)}
              />
              <Stat
                label="Deny CIDRs"
                value={String((status.policy ?? policy).deny_cidrs?.length ?? 0)}
              />
            </div>
            {((status.policy ?? policy).allow_ports.length > 0 ||
              (status.policy ?? policy).allow_cidrs.length > 0) && (
              <p className="text-xs text-[var(--zf-muted)] font-mono break-all">
                {[...(status.policy ?? policy).allow_cidrs, ...(status.policy ?? policy).allow_ports]
                  .slice(0, 12)
                  .join(' · ')}
                {(status.policy ?? policy).allow_cidrs.length +
                  (status.policy ?? policy).allow_ports.length >
                12
                  ? ' · …'
                  : ''}
              </p>
            )}
          </div>
        </div>
      )}

      {tab === 'policy' && (
        <div className="space-y-4">
          {!canWrite && (
            <p className="text-sm text-amber-800 bg-amber-50 border border-amber-200 rounded-lg px-3 py-2">
              Viewer accounts can inspect policy but cannot save changes.
            </p>
          )}

          <CiliumFlowControls
            vmName={vmName}
            policy={policy}
            disabled={!canWrite || saving}
            onChange={(next) => {
              setPolicy(next)
              setJsonText(JSON.stringify(next, null, 2))
              setDirty(true)
            }}
            onApply={applyFlowControl}
            onTemplate={(id) => {
              const tpl = templatePolicy(id)
              if (!tpl) return
              setPolicy(tpl)
              setJsonText(JSON.stringify(tpl, null, 2))
              setDirty(true)
              void setDataplanePolicy(vmName, tpl)
                .then((saved) => {
                  setPolicy(saved)
                  setJsonText(JSON.stringify(saved, null, 2))
                  setDirty(false)
                  toast.success(`Template ${id} applied`)
                })
                .catch((err) => toastFailure(toast, `Failed to apply template ${id}`, err))
            }}
          />

          <div className="flex flex-wrap gap-2">
            <PresetBtn disabled={!canWrite} onClick={() => applyPreset('open')} label="Allow all" />
            <PresetBtn disabled={!canWrite} onClick={() => applyPreset('deny')} label="Deny all" />
            <PresetBtn disabled={!canWrite} onClick={() => applyPreset('web')} label="Web egress" />
            <button
              type="button"
              onClick={() => setShowJson((v) => !v)}
              className="text-xs px-2.5 py-1.5 rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]"
            >
              {showJson ? 'Form editor' : 'Advanced JSON'}
            </button>
          </div>

          {showJson ? (
            <TerminalTextarea
              title="dataplane — policy JSON"
              className="h-56"
              value={jsonText}
              onChange={(e) => {
                setJsonText(e.target.value)
                setDirty(true)
              }}
              readOnly={!canWrite}
            />
          ) : (
            <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 space-y-4">
              <label className="flex items-center gap-2 text-sm text-[var(--zf-ink)]">
                <input
                  type="checkbox"
                  checked={policy.default_allow}
                  disabled={!canWrite}
                  onChange={(e) => updatePolicy({ default_allow: e.target.checked })}
                />
                Default allow (when unset, unmatched traffic is denied)
              </label>

              <TagList
                label="Allow CIDRs"
                hint="e.g. 10.0.0.0/8 or ::/0"
                items={policy.allow_cidrs}
                draft={cidrDraft}
                setDraft={setCidrDraft}
                onAdd={addCidr}
                onRemove={removeCidr}
                onClear={() => updatePolicy({ allow_cidrs: [] })}
                disabled={!canWrite}
                placeholder="Add CIDR"
              />

              <TagList
                label="Allow ports"
                hint="Must be tcp/PORT or udp/PORT — e.g. tcp/443 or udp/53"
                items={policy.allow_ports}
                draft={portDraft}
                setDraft={setPortDraft}
                onAdd={addPort}
                onRemove={removePort}
                onClear={() => updatePolicy({ allow_ports: [] })}
                disabled={!canWrite}
                placeholder="tcp/443"
              />

              <TagList
                label="Deny CIDRs"
                hint="Dropped even if an allow CIDR matches"
                items={policy.deny_cidrs ?? []}
                draft={denyDraft}
                setDraft={setDenyDraft}
                onAdd={() => addTagged('deny_cidrs', denyDraft, setDenyDraft)}
                onRemove={(v) => removeTagged('deny_cidrs', v)}
                onClear={() => updatePolicy({ deny_cidrs: [] })}
                disabled={!canWrite}
                placeholder="10.66.0.0/16"
              />

              <TagList
                label="Groups"
                hint="Explicit security-group names"
                items={policy.groups ?? []}
                draft={groupDraft}
                setDraft={setGroupDraft}
                onAdd={() => addTagged('groups', groupDraft, setGroupDraft)}
                onRemove={(v) => removeTagged('groups', v)}
                onClear={() => updatePolicy({ groups: [] })}
                disabled={!canWrite}
                placeholder="web"
              />

              <TagList
                label="Labels"
                hint="key=value — matches groups with those labels"
                items={policy.labels ?? []}
                draft={labelDraft}
                setDraft={setLabelDraft}
                onAdd={() => addTagged('labels', labelDraft, setLabelDraft)}
                onRemove={(v) => removeTagged('labels', v)}
                onClear={() => updatePolicy({ labels: [] })}
                disabled={!canWrite}
                placeholder="app=web"
              />

              <TagList
                label="Allow FQDNs"
                hint="Resolved to CIDRs at apply / refresh-dns"
                items={policy.allow_fqdns ?? []}
                draft={fqdnDraft}
                setDraft={setFqdnDraft}
                onAdd={() => addTagged('allow_fqdns', fqdnDraft, setFqdnDraft)}
                onRemove={(v) => removeTagged('allow_fqdns', v)}
                onClear={() => updatePolicy({ allow_fqdns: [] })}
                disabled={!canWrite}
                placeholder="api.example.com"
              />

              <TagList
                label="Entities"
                hint="world, host, cluster, …"
                items={policy.entities ?? []}
                draft={entityDraft}
                setDraft={setEntityDraft}
                onAdd={() => addTagged('entities', entityDraft, setEntityDraft)}
                onRemove={(v) => removeTagged('entities', v)}
                onClear={() => updatePolicy({ entities: [] })}
                disabled={!canWrite}
                placeholder="world"
              />

              <div className="flex flex-wrap gap-4">
                <label className="flex items-center gap-2 text-sm text-[var(--zf-ink)]">
                  <input
                    type="checkbox"
                    checked={!!policy.allow_icmp}
                    disabled={!canWrite}
                    onChange={(e) => updatePolicy({ allow_icmp: e.target.checked })}
                  />
                  Allow ICMP / ICMPv6
                </label>
                <label className="flex items-center gap-2 text-sm text-[var(--zf-ink)]">
                  <input
                    type="checkbox"
                    checked={!!policy.audit_mode}
                    disabled={!canWrite}
                    onChange={(e) => updatePolicy({ audit_mode: e.target.checked })}
                  />
                  Audit mode (log-and-allow)
                </label>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                <div>
                  <label className={labelCls}>Max egress Mbps</label>
                  <input
                    type="number"
                    min={0}
                    className={inputCls}
                    disabled={!canWrite}
                    value={policy.max_egress_mbps ?? ''}
                    placeholder="unlimited"
                    onChange={(e) => {
                      const v = e.target.value
                      updatePolicy({
                        max_egress_mbps: v === '' ? null : Math.max(0, parseInt(v, 10) || 0),
                      })
                    }}
                  />
                </div>
                <div>
                  <label className={labelCls}>Max egress PPS</label>
                  <input
                    type="number"
                    min={0}
                    className={inputCls}
                    disabled={!canWrite}
                    value={policy.max_egress_pps ?? ''}
                    placeholder="unlimited"
                    onChange={(e) => {
                      const v = e.target.value
                      updatePolicy({
                        max_egress_pps: v === '' ? null : Math.max(0, parseInt(v, 10) || 0),
                      })
                    }}
                  />
                </div>
                <div>
                  <label className={labelCls}>Sample rate</label>
                  <input
                    type="number"
                    min={0}
                    className={inputCls}
                    disabled={!canWrite}
                    value={policy.sample_rate}
                    onChange={(e) =>
                      updatePolicy({ sample_rate: Math.max(0, parseInt(e.target.value, 10) || 0) })
                    }
                  />
                  <p className="text-xs text-[var(--zf-muted)] mt-1">0 = off; ≥1 enables flow sampling</p>
                </div>
              </div>
            </div>
          )}

          <div className="flex flex-wrap gap-2 justify-end">
            <button
              type="button"
              disabled={!dirty || saving}
              onClick={resetPolicy}
              className="px-3 py-1.5 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)] text-[var(--zf-ink)] disabled:opacity-40"
            >
              Discard
            </button>
            {canWrite && (
              <button
                type="button"
                disabled={saving || (!dirty && !showJson)}
                onClick={() => void savePolicy()}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg bg-[var(--zf-link-fill)] text-white disabled:opacity-40"
              >
                {saving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : null}
                Save policy
              </button>
            )}
          </div>
        </div>
      )}

      {tab === 'effective' && (
        <div className="space-y-3">
          <p className="text-sm text-[var(--zf-muted)]">
            Declared policy merged with matching security groups (FluxVM edge).
          </p>
          {!effective ? (
            <p className="text-sm text-[var(--zf-muted)]">Effective policy unavailable.</p>
          ) : (
            <TerminalTextarea
              title="dataplane — effective"
              className="h-80"
              value={JSON.stringify(effective, null, 2)}
              readOnly
            />
          )}
        </div>
      )}

      {tab === 'stats' && (
        <div className="space-y-4">
          {!stats ? (
            <p className="text-sm text-[var(--zf-muted)]">No stats available (program may not be attached).</p>
          ) : (
            <>
              <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 grid grid-cols-2 sm:grid-cols-3 gap-4 text-sm">
                <Stat label="Allowed packets" value={stats.allowed_packets.toLocaleString()} />
                <Stat label="Allowed bytes" value={formatBytes(stats.allowed_bytes)} />
                <Stat label="Dropped packets" value={stats.dropped_packets.toLocaleString()} />
                <Stat label="Dropped bytes" value={formatBytes(stats.dropped_bytes)} />
                <Stat label="Drop rate" value={dropRate ?? '—'} />
              </div>
              {stats.pod_policy && (
                <div className="space-y-2">
                  <p className="text-sm text-[var(--zf-muted)]">
                    Pod policy counters
                    {stats.pod_policy.directional
                      ? ' (per-direction from Set 17 rule telemetry)'
                      : ' (shared Set 14/15 map mirrored to both directions)'}
                  </p>
                  <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 grid grid-cols-2 sm:grid-cols-3 gap-4 text-sm">
                    <Stat
                      label="Egress allowed"
                      value={stats.pod_policy.egress.allowed.toLocaleString()}
                    />
                    <Stat
                      label="Egress dropped"
                      value={stats.pod_policy.egress.dropped.toLocaleString()}
                    />
                    <Stat
                      label="Egress audited"
                      value={stats.pod_policy.egress.audited.toLocaleString()}
                    />
                    <Stat
                      label="Ingress allowed"
                      value={stats.pod_policy.ingress.allowed.toLocaleString()}
                    />
                    <Stat
                      label="Ingress dropped"
                      value={stats.pod_policy.ingress.dropped.toLocaleString()}
                    />
                    <Stat
                      label="Ingress audited"
                      value={stats.pod_policy.ingress.audited.toLocaleString()}
                    />
                  </div>
                </div>
              )}
              <button
                type="button"
                onClick={() => void load()}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)]"
              >
                <RefreshCw className="w-3.5 h-3.5" /> Refresh counters
              </button>
            </>
          )}
        </div>
      )}

      {tab === 'flows' && (
        <div className="space-y-3">
          <div className="flex flex-wrap items-end gap-3">
            <div>
              <label className={labelCls}>Limit</label>
              <select
                className={inputCls}
                value={flowLimit}
                onChange={(e) => setFlowLimit(parseInt(e.target.value, 10))}
              >
                {[20, 50, 100, 250, 500, 1000].map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </div>
            <button
              type="button"
              onClick={() => void load()}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)]"
            >
              <RefreshCw className="w-3.5 h-3.5" /> Reload flows
            </button>
            <p className="text-xs text-[var(--zf-muted)] pb-2">
              {flows.length} flow{flows.length === 1 ? '' : 's'} · sample_rate={policy.sample_rate}
            </p>
          </div>

          {flows.length === 0 ? (
            <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-6 text-center text-sm text-[var(--zf-muted)]">
              No flows sampled yet. Set sample rate ≥ 1 on the Policy tab and generate traffic.
            </div>
          ) : (
            <PacketFlowPanel
              views={flows.map((f) =>
                fromRawFlow(f, {
                  vmName,
                  dataplaneMode: status?.mode,
                  labels: policy.labels,
                }),
              )}
              emptyHint="No flows match the current filter."
              onReload={() => void load()}
              onBlockDest={
                canWrite
                  ? (ip, port, proto) =>
                      void applyFlowControl('block', {
                        cidr: ip,
                        port: proto && proto !== 'any' ? `${proto}/${port}` : undefined,
                      })
                  : undefined
              }
            />
          )}
        </div>
      )}

      {tab === 'drops' && (
        <div className="space-y-3">
          <div className="flex flex-wrap items-end gap-3">
            <button
              type="button"
              onClick={() => void load()}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)]"
            >
              <RefreshCw className="w-3.5 h-3.5" /> Reload drops
            </button>
            <p className="text-xs text-[var(--zf-muted)] pb-2">
              {drops.length} drop reason{drops.length === 1 ? '' : 's'}
            </p>
          </div>
          {drops.length === 0 ? (
            <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-6 text-center text-sm text-[var(--zf-muted)]">
              No drop-reason samples yet.
            </div>
          ) : (
            <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="bg-[var(--zf-canvas)] text-left text-xs text-[var(--zf-muted)]">
                  <tr>
                    <th className="px-3 py-2">Reason</th>
                    <th className="px-3 py-2">Src</th>
                    <th className="px-3 py-2">Dst</th>
                    <th className="px-3 py-2">Pkts</th>
                    <th className="px-3 py-2">Bytes</th>
                  </tr>
                </thead>
                <tbody>
                  {drops.map((d, i) => (
                    <tr key={i} className="border-t border-[var(--zf-canvas-alt)]">
                      <td className="px-3 py-2 font-mono text-xs">{d.reason}</td>
                      <td className="px-3 py-2 font-mono text-xs">
                        {d.source}:{d.source_port}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs">
                        {d.destination}:{d.destination_port}
                      </td>
                      <td className="px-3 py-2">{d.packets.toLocaleString()}</td>
                      <td className="px-3 py-2">{d.bytes.toLocaleString()}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function Stat({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0">
      <div className="text-xs text-[var(--zf-muted)] uppercase tracking-wide">{label}</div>
      <div
        className={`font-medium text-[var(--zf-ink)] mt-0.5 truncate ${mono ? 'font-mono text-xs' : ''}`}
        title={value}
      >
        {value}
      </div>
    </div>
  )
}

function PresetBtn({
  label,
  onClick,
  disabled,
}: {
  label: string
  onClick: () => void
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className="text-xs px-2.5 py-1.5 rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)] text-[var(--zf-ink)] hover:bg-[var(--zf-canvas)] disabled:opacity-40"
    >
      {label}
    </button>
  )
}

function TagList({
  label,
  hint,
  items,
  draft,
  setDraft,
  onAdd,
  onRemove,
  onClear,
  disabled,
  placeholder,
}: {
  label: string
  hint: string
  items: string[]
  draft: string
  setDraft: (v: string) => void
  onAdd: () => void
  onRemove: (v: string) => void
  onClear: () => void
  disabled?: boolean
  placeholder: string
}) {
  return (
    <div>
      <label className={labelCls}>
        {label} <span className="font-normal normal-case">({hint})</span>
      </label>
      <div className="flex flex-wrap gap-1.5 mb-2 min-h-[1.5rem]">
        {items.length === 0 && (
          <span className="text-xs text-[var(--zf-muted)]">None</span>
        )}
        {items.map((item) => (
          <span
            key={item}
            className="inline-flex items-center gap-1 text-xs font-mono px-2 py-1 rounded-md bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] text-[var(--zf-ink)]"
          >
            {item}
            {!disabled && (
              <button
                type="button"
                onClick={() => onRemove(item)}
                className="text-[var(--zf-muted)] hover:text-red-600"
                aria-label={`Remove ${item}`}
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </span>
        ))}
      </div>
      {!disabled && (
        <div className="flex gap-2">
          <input
            type="text"
            className={inputCls}
            value={draft}
            placeholder={placeholder}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                onAdd()
              }
            }}
          />
          <button
            type="button"
            onClick={onAdd}
            className="inline-flex items-center gap-1 px-3 py-2 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)] shrink-0"
          >
            <Plus className="w-3.5 h-3.5" /> Add
          </button>
          {items.length > 0 && (
            <button
              type="button"
              onClick={onClear}
              className="inline-flex items-center gap-1 px-3 py-2 text-sm rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)] text-[var(--zf-muted)] shrink-0"
              title="Clear all"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      )}
    </div>
  )
}
