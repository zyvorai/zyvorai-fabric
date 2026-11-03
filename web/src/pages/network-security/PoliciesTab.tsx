// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../../components/Skeleton'
import { useState, useEffect } from 'react'
import { Plus, Trash2, RefreshCw, Pencil, Eye } from 'lucide-react'
import * as api from '../../api/network-security'
import type { NetworkPolicy, CreateNetworkPolicyRequest, PolicyRule, PolicyDirection, PolicyIngressRule, PolicyEgressRule, SecurityIdentity } from '../../api/network-security'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from '../network/ModalShared'
import { LabelSelectorInput, LabelTags, StatusBadge } from './ModalShared'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

/** One PolicyRule (UI's flat cidr+port shape) -> one backend rule with at
 * most one peer selector and one port rule. The backend supports multiple
 * peers/ports per rule and label-selector peers, which this simple form
 * doesn't author -- good enough for rules created through this UI; a
 * hand-crafted or discovered policy with richer rules just shows a
 * simplified view when edited here. */
function rulesToBackend(rules: PolicyRule[]): { ingress: PolicyIngressRule[]; egress: PolicyEgressRule[] } {
  const toRule = (r: PolicyRule) => ({
    peers: r.cidr ? [{ cidr: r.cidr }] : [],
    ports: r.port ? [{ protocol: (r.protocol as 'tcp' | 'udp' | 'any') || 'tcp', port: r.port }] : [],
  })
  const ingress = rules.filter(r => r.direction === 'ingress').map(r => {
    const { peers, ports } = toRule(r)
    return { from: peers, to_ports: ports }
  })
  const egress = rules.filter(r => r.direction === 'egress').map(r => {
    const { peers, ports } = toRule(r)
    return { to: peers, to_ports: ports }
  })
  return { ingress, egress }
}

function rulesFromBackend(ingress: PolicyIngressRule[], egress: PolicyEgressRule[]): PolicyRule[] {
  const fromPeers = (peers: PolicyIngressRule['from']) =>
    (peers ?? []).find((p): p is { cidr: string } => 'cidr' in p)?.cidr
  const rules: PolicyRule[] = []
  for (const r of ingress) {
    rules.push({ direction: 'ingress', cidr: fromPeers(r.from), protocol: r.to_ports?.[0]?.protocol, port: r.to_ports?.[0]?.port })
  }
  for (const r of egress) {
    rules.push({ direction: 'egress', cidr: fromPeers(r.to), protocol: r.to_ports?.[0]?.protocol, port: r.to_ports?.[0]?.port })
  }
  return rules
}

interface PoliciesTabProps {
  policies: NetworkPolicy[]
  identities: SecurityIdentity[]
  onDelete: (id: string) => void
  onAdopt?: (id: string) => void
  onAdoptIdentity?: (id: string) => void
  onEdit?: (id: string) => void
  onCreate: () => void
  onSync: () => void
}

function PoliciesTabContent({ policies, identities, onDelete, onAdopt, onAdoptIdentity, onEdit, onCreate, onSync }: PoliciesTabProps) {
  const readOnly = useReadOnly()
  const [view, setView] = useState<'policies' | 'identities'>('policies')
  const [status, setStatus] = useState<{ enforced: number; pending: number } | null>(null)
  const [viewingIdentityId, setViewingIdentityId] = useState<number | null>(null)

  const refreshStatus = () => { api.networkPolicyStatus().then(setStatus).catch(() => {}) }
  useEffect(() => { refreshStatus() }, [])
  const handleSyncClick = async () => { await onSync(); refreshStatus() }

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h2 className="text-xl font-semibold">Network Policies</h2>
          <div className="flex bg-[var(--zf-surface)] rounded-lg p-0.5">
            {(['policies', 'identities'] as const).map(v => (
              <button key={v} onClick={() => setView(v)} className={`px-3 py-1 rounded text-sm transition ${view === v ? 'bg-[var(--zf-canvas-alt)] text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}>
                {v.charAt(0).toUpperCase() + v.slice(1)}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {status && (
            <span className="text-xs text-[var(--zf-muted)] bg-[var(--zf-surface)] rounded-lg px-3 py-1.5 border border-[var(--zf-hairline)]">
              {status.enforced} enforced &middot; {status.pending} pending
            </span>
          )}
          {!readOnly && <button onClick={handleSyncClick} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync
          </button>}
          {view === 'policies' && !readOnly && (
            <button onClick={onCreate} className="flex items-center gap-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
              <Plus className="w-4 h-4" /> Add Policy
            </button>
          )}
        </div>
      </div>

      {view === 'policies' && (
      policies.length === 0 ? (
        <div className="p-12 text-center text-[var(--zf-muted)]">No network policies configured. Create one to define ingress/egress rules with label selectors.</div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-[var(--zf-surface)]">
              <tr>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Labels</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Ingress</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Egress</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VMs</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {policies.map(p => {
                const ingressCount = p.ingress?.length ?? 0
                const egressCount = p.egress?.length ?? 0
                const labels = p.endpoint_selector?.match_labels
                return (
                <tr key={p.id} className="hover:bg-white/[0.03] transition">
                  <td className="p-4">
                    <div className="font-medium">{p.name}{isHostManaged(p) && <HostBadge />}</div>
                    {p.description && <div className="text-xs text-[var(--zf-muted)] mt-1">{p.description}</div>}
                  </td>
                  <td className="p-4"><LabelTags labels={labels} /></td>
                  <td className="p-4">
                    <StatusBadge status={`${ingressCount} rules`} color="green" />
                  </td>
                  <td className="p-4">
                    <StatusBadge status={`${egressCount} rules`} color="yellow" />
                  </td>
                  <td className="p-4 text-[var(--zf-link)] font-medium">{p.matched_vms ?? (isHostManaged(p) ? 'host' : '—')}</td>
                  <td className="p-4">
                    <StatusBadge status={p.enabled ? 'active' : 'disabled'} color={p.enabled ? 'green' : 'gray'} />
                  </td>
                  <td className="p-4">
                    <div className="flex items-center gap-1">
                      {!readOnly && !isHostManaged(p) && onEdit && (
                        <button onClick={() => onEdit(p.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="Edit">
                          <Pencil className="w-4 h-4" />
                        </button>
                      )}
                      <HostManagedActions readOnly={readOnly}
                        item={{ id: p.id, managed: p.managed }}
                        onDelete={() => onDelete(p.id)}
                        onAdopt={onAdopt ? () => onAdopt(p.id) : undefined}
                      />
                    </div>
                  </td>
                </tr>
              )})}
            </tbody>
          </table>
        </div>
      )
      )}

      {view === 'identities' && (
        identities.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No security identities yet. Identities are created from VM labels or discovered from firewalld zones and nftables sets on the host.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">ID</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Labels</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Endpoints</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {identities.map(i => (
                  <tr key={i.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-mono text-sm">
                      {i.id}
                      {isHostManaged({ managed: i.managed, id: String(i.id) }) && <HostBadge />}
                      {i.description && <div className="text-xs text-[var(--zf-muted)] font-normal mt-0.5 max-w-xs">{i.description}</div>}
                    </td>
                    <td className="p-4"><LabelTags labels={i.labels} /></td>
                    <td className="p-4 font-mono text-xs text-[var(--zf-muted)] max-w-md truncate" title={i.endpoints.join(', ')}>
                      {i.endpoints.length > 0 ? i.endpoints.join(', ') : '—'}
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        <button onClick={() => setViewingIdentityId(i.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="View details">
                          <Eye className="w-4 h-4" />
                        </button>
                        <HostManagedActions readOnly={readOnly}
                          item={{ id: String(i.id), managed: i.managed }}
                          onDelete={() => {}}
                          onAdopt={onAdoptIdentity ? () => onAdoptIdentity(String(i.id)) : undefined}
                          adoptLabel="Adopt"
                        />
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}
      {viewingIdentityId !== null && (
        <IdentityDetailModal id={viewingIdentityId} onClose={() => setViewingIdentityId(null)} />
      )}
    </div>
  )
}

function IdentityDetailModal({ id, onClose }: { id: number; onClose: () => void }) {
  const [identity, setIdentity] = useState<SecurityIdentity | null>(null)
  const [loading, setLoading] = useState(true)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getIdentity(id).then(i => {
      if (cancelled) return
      setIdentity(i)
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [id])

  return (
    <ModalWrapper title={`Identity ${id}`} onClose={onClose}>
      {loading ? (
        <PanelLoading label="Loading..." />
      ) : err ? (
        <p className="text-red-600 text-sm">{err}</p>
      ) : identity ? (
        <div className="space-y-4">
          <div>
            <div className="text-xs text-[var(--zf-muted)] mb-1">ID</div>
            <div className="font-mono text-sm">{identity.id}{isHostManaged({ managed: identity.managed, id: String(identity.id) }) && <HostBadge />}</div>
          </div>
          {identity.description && (
            <div>
              <div className="text-xs text-[var(--zf-muted)] mb-1">Description</div>
              <div className="text-sm text-[var(--zf-ink)]">{identity.description}</div>
            </div>
          )}
          <div>
            <div className="text-xs text-[var(--zf-muted)] mb-1">Labels</div>
            <LabelTags labels={identity.labels} />
          </div>
          <div>
            <div className="text-xs text-[var(--zf-muted)] mb-1">Endpoints</div>
            {identity.endpoints.length > 0 ? (
              <div className="space-y-1">
                {identity.endpoints.map((ep, idx) => (
                  <div key={idx} className="font-mono text-xs text-[var(--zf-ink)] bg-[var(--zf-surface)] rounded px-2 py-1">{ep}</div>
                ))}
              </div>
            ) : (
              <span className="text-[var(--zf-muted)] text-sm">none</span>
            )}
          </div>
          <div className="grid grid-cols-2 gap-4 text-xs text-[var(--zf-muted)]">
            <div>
              <div className="mb-1">Created</div>
              <div>{new Date(identity.created).toLocaleString()}</div>
            </div>
            <div>
              <div className="mb-1">Updated</div>
              <div>{new Date(identity.updated).toLocaleString()}</div>
            </div>
          </div>
        </div>
      ) : null}
    </ModalWrapper>
  )
}

export function CreatePolicyModal({ onClose, onCreated }: { onClose: () => void; onCreated: (p: NetworkPolicy) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [rules, setRules] = useState<PolicyRule[]>([])
  const [ruleDir, setRuleDir] = useState<PolicyDirection>('ingress')
  const [ruleProto, setRuleProto] = useState('')
  const [rulePort, setRulePort] = useState('')
  const [ruleCidr, setRuleCidr] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const addRule = () => {
    setRules(prev => [...prev, {
      direction: ruleDir,
      protocol: ruleProto || undefined,
      port: rulePort ? parseInt(rulePort) : undefined,
      cidr: ruleCidr || undefined,
    }])
    setRuleProto('')
    setRulePort('')
    setRuleCidr('')
  }

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const { ingress, egress } = rulesToBackend(rules)
      const req: CreateNetworkPolicyRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        endpoint_selector: { match_labels: labels },
        ingress,
        egress,
      }
      const p = await api.createNetworkPolicy(req)
      onCreated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Network Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="allow-web-traffic" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Allow HTTP/HTTPS ingress" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Rule (allow)</div>
          <div>
            <label className="block text-xs text-[var(--zf-muted)] mb-1">Direction</label>
            <select aria-label="Direction" value={ruleDir} onChange={e => setRuleDir(e.target.value as PolicyDirection)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
              <option value="ingress">Ingress</option>
              <option value="egress">Egress</option>
            </select>
          </div>
          <div className="grid grid-cols-3 gap-2">
            <InputField label="Protocol" value={ruleProto} onChange={setRuleProto} placeholder="tcp" />
            <InputField label="Port" value={rulePort} onChange={setRulePort} placeholder="443" type="number" />
            <InputField label="CIDR" value={ruleCidr} onChange={setRuleCidr} placeholder="10.0.0.0/8" />
          </div>
          <button type="button" onClick={addRule} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Rule
          </button>
          {rules.length > 0 && (
            <div className="space-y-1 mt-2">
              {rules.map((r, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={r.direction} color={r.direction === 'ingress' ? 'green' : 'yellow'} />
                  {r.protocol && <span className="text-[var(--zf-muted)]">{r.protocol}</span>}
                  {r.port && <span className="text-[var(--zf-muted)]">:{r.port}</span>}
                  {r.cidr && <span className="text-[var(--zf-muted)]">{r.cidr}</span>}
                  <button onClick={() => setRules(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Policy'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditPolicyModal({ id, onClose, onUpdated }: { id: string; onClose: () => void; onUpdated: (p: NetworkPolicy) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [rules, setRules] = useState<PolicyRule[]>([])
  const [ruleDir, setRuleDir] = useState<PolicyDirection>('ingress')
  const [ruleProto, setRuleProto] = useState('')
  const [rulePort, setRulePort] = useState('')
  const [ruleCidr, setRuleCidr] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getNetworkPolicy(id).then(p => {
      if (cancelled) return
      setName(p.name)
      setDescription(p.description ?? '')
      setLabels(p.endpoint_selector?.match_labels ?? {})
      setRules(rulesFromBackend(p.ingress ?? [], p.egress ?? []))
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setLoadErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [id])

  const addRule = () => {
    setRules(prev => [...prev, {
      direction: ruleDir,
      protocol: ruleProto || undefined,
      port: rulePort ? parseInt(rulePort) : undefined,
      cidr: ruleCidr || undefined,
    }])
    setRuleProto('')
    setRulePort('')
    setRuleCidr('')
  }

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const { ingress, egress } = rulesToBackend(rules)
      const req: CreateNetworkPolicyRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        endpoint_selector: { match_labels: labels },
        ingress,
        egress,
      }
      const p = await api.updateNetworkPolicy(id, req)
      onUpdated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit Network Policy" onClose={onClose}>
        <PanelLoading label="Loading..." />
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit Network Policy" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit Network Policy" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="allow-web-traffic" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Allow HTTP/HTTPS ingress" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Rule (allow)</div>
          <div>
            <label className="block text-xs text-[var(--zf-muted)] mb-1">Direction</label>
            <select aria-label="Direction" value={ruleDir} onChange={e => setRuleDir(e.target.value as PolicyDirection)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
              <option value="ingress">Ingress</option>
              <option value="egress">Egress</option>
            </select>
          </div>
          <div className="grid grid-cols-3 gap-2">
            <InputField label="Protocol" value={ruleProto} onChange={setRuleProto} placeholder="tcp" />
            <InputField label="Port" value={rulePort} onChange={setRulePort} placeholder="443" type="number" />
            <InputField label="CIDR" value={ruleCidr} onChange={setRuleCidr} placeholder="10.0.0.0/8" />
          </div>
          <button type="button" onClick={addRule} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Rule
          </button>
          {rules.length > 0 && (
            <div className="space-y-1 mt-2">
              {rules.map((r, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={r.direction} color={r.direction === 'ingress' ? 'green' : 'yellow'} />
                  {r.protocol && <span className="text-[var(--zf-muted)]">{r.protocol}</span>}
                  {r.port && <span className="text-[var(--zf-muted)]">:{r.port}</span>}
                  {r.cidr && <span className="text-[var(--zf-muted)]">{r.cidr}</span>}
                  <button onClick={() => setRules(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default PoliciesTabContent
