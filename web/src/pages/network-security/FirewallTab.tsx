// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../../components/Skeleton'
import { useState, useEffect } from 'react'
import { Plus, Trash2, RefreshCw, Pencil } from 'lucide-react'
import * as api from '../../api/network-security'
import type { FirewallProfile, CreateFirewallProfileRequest, FirewallRule, FirewallAction, FirewallZone, CreateFirewallZoneRequest, VMFirewallAssignment, CreateVMFirewallAssignmentRequest } from '../../api/network-security'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from '../network/ModalShared'
import { StatusBadge } from './ModalShared'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface FirewallTabProps {
  profiles: FirewallProfile[]
  zones: FirewallZone[]
  assignments: VMFirewallAssignment[]
  onDeleteProfile: (id: string) => void
  onDeleteZone: (id: string) => void
  onDeleteAssignment: (vmName: string) => void
  onAdoptProfile?: (id: string) => void
  onAdoptZone?: (id: string) => void
  onEditProfile?: (id: string) => void
  onCreate: () => void
  onCreateZone: () => void
  onCreateAssignment: () => void
  onSync: () => void
}

function FirewallTabContent({ profiles, zones, assignments, onDeleteProfile, onDeleteZone, onDeleteAssignment, onAdoptProfile, onAdoptZone, onEditProfile, onCreate, onCreateZone, onCreateAssignment, onSync }: FirewallTabProps) {
  const readOnly = useReadOnly()
  const [view, setView] = useState<'profiles' | 'zones' | 'assignments'>('profiles')
  const [status, setStatus] = useState<{ active_profiles: number; assigned_vms: number } | null>(null)

  const refreshStatus = () => { api.firewallStatus().then(setStatus).catch(() => {}) }
  useEffect(() => { refreshStatus() }, [])
  const handleSyncClick = async () => { await onSync(); refreshStatus() }

  const handleCreateClick = () => {
    if (view === 'profiles') onCreate()
    else if (view === 'zones') onCreateZone()
    else onCreateAssignment()
  }

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h2 className="text-xl font-semibold">VM Firewall</h2>
          <div className="flex bg-[var(--zf-surface)] rounded-lg p-0.5">
            {(['profiles', 'zones', 'assignments'] as const).map(v => (
              <button key={v} onClick={() => setView(v)} className={`px-3 py-1 rounded text-sm transition ${view === v ? 'bg-[var(--zf-canvas-alt)] text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}>
                {v.charAt(0).toUpperCase() + v.slice(1)}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {status && (
            <span className="text-xs text-[var(--zf-muted)] bg-[var(--zf-surface)] rounded-lg px-3 py-1.5 border border-[var(--zf-hairline)]">
              {status.active_profiles} active &middot; {status.assigned_vms} assigned VMs
            </span>
          )}
          {!readOnly && <button onClick={handleSyncClick} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync
          </button>}
          {!readOnly && <button onClick={handleCreateClick} className="flex items-center gap-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Add {view === 'profiles' ? 'Profile' : view === 'zones' ? 'Zone' : 'Assignment'}
          </button>}
        </div>
      </div>

      {view === 'profiles' && (
        profiles.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No firewall profiles. Create one to define firewall rules for VMs.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Default Action</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Rules</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {profiles.map(p => (
                  <tr key={p.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4">
                      <div className="font-medium">{p.name}{isHostManaged(p) && <HostBadge />}</div>
                      {p.description && <div className="text-xs text-[var(--zf-muted)] mt-1 max-w-md truncate">{p.description}</div>}
                    </td>
                    <td className="p-4">
                      <StatusBadge status={p.default_action} color={p.default_action === 'accept' ? 'green' : 'red'} />
                    </td>
                    <td className="p-4 font-mono text-sm">{p.rules.length}</td>
                    <td className="p-4 text-xs text-[var(--zf-muted)]">
                      {isHostManaged(p) ? 'host nft' : 'managed'}
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        {!readOnly && !isHostManaged(p) && onEditProfile && (
                          <button onClick={() => onEditProfile(p.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="Edit">
                            <Pencil className="w-4 h-4" />
                          </button>
                        )}
                        <HostManagedActions readOnly={readOnly}
                          item={{ id: p.id, managed: p.managed }}
                          onDelete={() => onDeleteProfile(p.id)}
                          onAdopt={onAdoptProfile ? () => onAdoptProfile(p.id) : undefined}
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

      {view === 'zones' && (
        zones.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No firewall zones configured.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Profile ID</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {zones.map(z => (
                  <tr key={z.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-medium">
                      {z.name}{isHostManaged(z) && <HostBadge />}
                      {z.description && <div className="text-xs text-[var(--zf-muted)] mt-1 max-w-md truncate">{z.description}</div>}
                    </td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{z.default_profile_id ?? '-'}</td>
                    <td className="p-4">
                      <HostManagedActions readOnly={readOnly}
                        item={{ id: z.id, managed: z.managed }}
                        onDelete={() => onDeleteZone(z.id)}
                        onAdopt={onAdoptZone ? () => onAdoptZone(z.id) : undefined}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {view === 'assignments' && (
        assignments.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No VM firewall assignments.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VM Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Profile ID</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Zone ID</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {assignments.map(a => (
                  <tr key={a.vm_name} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-medium">{a.vm_name}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{a.profile_id}</td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{a.zone_id ?? '-'}</td>
                    <td className="p-4">
                      {!readOnly && (
                      <button onClick={() => onDeleteAssignment(a.vm_name)} className="p-2 hover:bg-red-600 rounded transition" aria-label={`Delete ${a.vm_name}`} title={`Delete ${a.vm_name}`}>
                        <Trash2 className="w-4 h-4" />
                      </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}
    </div>
  )
}

export function CreateFirewallProfileModal({ onClose, onCreated }: { onClose: () => void; onCreated: (p: FirewallProfile) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [defaultAction, setDefaultAction] = useState<FirewallAction>('drop')
  const [rules, setRules] = useState<FirewallRule[]>([])
  const [ruleProto, setRuleProto] = useState('')
  const [rulePort, setRulePort] = useState('')
  const [ruleSrc, setRuleSrc] = useState('')
  const [ruleAction, setRuleAction] = useState<FirewallAction>('accept')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const addRule = () => {
    setRules(prev => [...prev, {
      priority: (prev.length + 1) * 10,
      protocol: ruleProto || undefined,
      dest_port: rulePort ? parseInt(rulePort) : undefined,
      source_cidr: ruleSrc || undefined,
      action: ruleAction,
    }])
    setRuleProto('')
    setRulePort('')
    setRuleSrc('')
  }

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateFirewallProfileRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        default_action: defaultAction,
        rules: rules.map((r, i) => ({ ...r, priority: (i + 1) * 10 })),
      }
      const p = await api.createFirewallProfile(req)
      onCreated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Firewall Profile" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="web-profile" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Allow web traffic" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Default Action</label>
          <select aria-label="Default Action" value={defaultAction} onChange={e => setDefaultAction(e.target.value as FirewallAction)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="accept">Accept</option>
            <option value="drop">Drop</option>
            <option value="reject">Reject</option>
            <option value="log">Log</option>
          </select>
        </div>
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Rule</div>
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Protocol" value={ruleProto} onChange={setRuleProto} placeholder="tcp" />
            <InputField label="Dest Port" value={rulePort} onChange={setRulePort} placeholder="80" type="number" />
          </div>
          <InputField label="Source CIDR" value={ruleSrc} onChange={setRuleSrc} placeholder="0.0.0.0/0" />
          <div>
            <label className="block text-xs text-[var(--zf-muted)] mb-1">Action</label>
            <select aria-label="Action" value={ruleAction} onChange={e => setRuleAction(e.target.value as FirewallAction)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
              <option value="accept">Accept</option>
              <option value="drop">Drop</option>
              <option value="reject">Reject</option>
              <option value="log">Log</option>
            </select>
          </div>
          <button type="button" onClick={addRule} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Rule
          </button>
          {rules.length > 0 && (
            <div className="space-y-1 mt-2">
              {rules.map((r, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={r.action} color={r.action === 'accept' ? 'green' : 'red'} />
                  {r.protocol && <span className="text-[var(--zf-muted)]">{r.protocol}</span>}
                  {r.dest_port && <span className="text-[var(--zf-muted)]">:{r.dest_port}</span>}
                  {r.source_cidr && <span className="text-[var(--zf-muted)]">src:{r.source_cidr}</span>}
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
          {submitting ? 'Creating...' : 'Create Profile'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditFirewallProfileModal({ id, onClose, onUpdated }: { id: string; onClose: () => void; onUpdated: (p: FirewallProfile) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [defaultAction, setDefaultAction] = useState<FirewallAction>('drop')
  const [rules, setRules] = useState<FirewallRule[]>([])
  const [ruleProto, setRuleProto] = useState('')
  const [rulePort, setRulePort] = useState('')
  const [ruleSrc, setRuleSrc] = useState('')
  const [ruleAction, setRuleAction] = useState<FirewallAction>('accept')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getFirewallProfile(id).then(p => {
      if (cancelled) return
      setName(p.name)
      setDescription(p.description ?? '')
      setDefaultAction(p.default_action)
      setRules(p.rules)
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
      priority: (prev.length + 1) * 10,
      protocol: ruleProto || undefined,
      dest_port: rulePort ? parseInt(rulePort) : undefined,
      source_cidr: ruleSrc || undefined,
      action: ruleAction,
    }])
    setRuleProto('')
    setRulePort('')
    setRuleSrc('')
  }

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateFirewallProfileRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        default_action: defaultAction,
        rules: rules.map((r, i) => ({ ...r, priority: (i + 1) * 10 })),
      }
      const p = await api.updateFirewallProfile(id, req)
      onUpdated(p)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit Firewall Profile" onClose={onClose}>
        <PanelLoading label="Loading..." />
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit Firewall Profile" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit Firewall Profile" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="web-profile" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Allow web traffic" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Default Action</label>
          <select aria-label="Default Action" value={defaultAction} onChange={e => setDefaultAction(e.target.value as FirewallAction)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="accept">Accept</option>
            <option value="drop">Drop</option>
            <option value="reject">Reject</option>
            <option value="log">Log</option>
          </select>
        </div>
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Rule</div>
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Protocol" value={ruleProto} onChange={setRuleProto} placeholder="tcp" />
            <InputField label="Dest Port" value={rulePort} onChange={setRulePort} placeholder="80" type="number" />
          </div>
          <InputField label="Source CIDR" value={ruleSrc} onChange={setRuleSrc} placeholder="0.0.0.0/0" />
          <div>
            <label className="block text-xs text-[var(--zf-muted)] mb-1">Action</label>
            <select aria-label="Action" value={ruleAction} onChange={e => setRuleAction(e.target.value as FirewallAction)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] text-sm focus:outline-none focus:border-blue-500">
              <option value="accept">Accept</option>
              <option value="drop">Drop</option>
              <option value="reject">Reject</option>
              <option value="log">Log</option>
            </select>
          </div>
          <button type="button" onClick={addRule} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Rule
          </button>
          {rules.length > 0 && (
            <div className="space-y-1 mt-2">
              {rules.map((r, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <StatusBadge status={r.action} color={r.action === 'accept' ? 'green' : 'red'} />
                  {r.protocol && <span className="text-[var(--zf-muted)]">{r.protocol}</span>}
                  {r.dest_port && <span className="text-[var(--zf-muted)]">:{r.dest_port}</span>}
                  {r.source_cidr && <span className="text-[var(--zf-muted)]">src:{r.source_cidr}</span>}
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

export function CreateFirewallZoneModal({ profiles, onClose, onCreated }: { profiles: FirewallProfile[]; onClose: () => void; onCreated: (z: FirewallZone) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [defaultProfileId, setDefaultProfileId] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim()) { setErr('Name is required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateFirewallZoneRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        default_profile_id: defaultProfileId || undefined,
      }
      const z = await api.createFirewallZone(req)
      onCreated(z)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create Firewall Zone" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="dmz" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="DMZ zone" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Default Profile</label>
          <select aria-label="Default Profile" value={defaultProfileId} onChange={e => setDefaultProfileId(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="">None</option>
            {profiles.map(p => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Zone'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function CreateVMFirewallAssignmentModal({ profiles, zones, onClose, onCreated }: { profiles: FirewallProfile[]; zones: FirewallZone[]; onClose: () => void; onCreated: (a: VMFirewallAssignment) => void }) {
  const [vmName, setVmName] = useState('')
  const [profileId, setProfileId] = useState(profiles[0]?.id ?? '')
  const [zoneId, setZoneId] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!vmName.trim() || !profileId) { setErr('VM name and profile are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateVMFirewallAssignmentRequest = {
        vm_name: vmName.trim(),
        profile_id: profileId,
        zone_id: zoneId || undefined,
      }
      const a = await api.createVMFirewallAssignment(req)
      onCreated(a)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Assign VM Firewall Profile" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="VM Name" value={vmName} onChange={setVmName} placeholder="web-server-01" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Profile</label>
          <select aria-label="Profile" value={profileId} onChange={e => setProfileId(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            {profiles.length === 0 ? <option value="">No profiles — create one first</option> : profiles.map(p => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Zone (optional)</label>
          <select aria-label="Zone (optional)" value={zoneId} onChange={e => setZoneId(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="">None</option>
            {zones.map(z => (
              <option key={z.id} value={z.id}>{z.name}</option>
            ))}
          </select>
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting || !profileId} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Assigning...' : 'Assign Profile'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default FirewallTabContent
