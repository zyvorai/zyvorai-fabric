// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Zap, Plus, Trash2, PowerOff } from 'lucide-react'
import {
  listZones,
  createZone,
  deleteZone,
  listSpotInstances,
  createSpotInstance,
  deleteSpotInstance,
  evictSpotInstance,
  type AvailabilityZone,
  type SpotInstance,
} from '../api/zones'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'
import { PageHeader } from '../components/ui'

const statusColor: Record<AvailabilityZone['status'], string> = {
  available: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  degraded: 'text-amber-800 bg-amber-50 border-amber-200',
  unavailable: 'text-red-700 bg-red-50 border-red-200',
}

const spotStatusColor: Record<SpotInstance['status'], string> = {
  running: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  evicted: 'text-amber-800 bg-amber-50 border-amber-200',
  terminated: 'bg-[var(--zf-canvas)] text-[var(--zf-muted)] border-[var(--zf-hairline)]',
}

export default function Zones() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [zones, setZones] = useState<AvailabilityZone[]>([])
  const [spots, setSpots] = useState<SpotInstance[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load availability zones')
  const [tab, setTab] = useState<'zones' | 'spot'>('zones')
  const [showCreateZone, setShowCreateZone] = useState(false)
  const [showCreateSpot, setShowCreateSpot] = useState(false)

  const loadData = useCallback(() => {
    return run(async () => {
      const [z, s] = await Promise.all([listZones(), listSpotInstances()])
      setZones(z)
      setSpots(s)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDeleteZone = async (id: string, name: string) => {
    if (!await confirm('Delete Availability Zone', `Delete zone '${name}'?`, { variant: 'danger', confirmLabel: 'Delete' })) return
    try {
      await deleteZone(id)
      toast.success('Zone deleted')
      loadData()
    } catch (err) {
      toastFailure(toast, 'Failed to delete zone', err)
    }
  }

  const handleEvict = async (spot: SpotInstance) => {
    if (!await confirm('Evict Spot Instance', `Evict '${spot.vm_name}'? Its eviction policy (${spot.eviction_policy}) will be applied to the VM immediately.`, { variant: 'danger', confirmLabel: 'Evict' })) return
    try {
      await evictSpotInstance(spot.id)
      toast.success(`'${spot.vm_name}' evicted`)
      loadData()
    } catch (err) {
      toastFailure(toast, 'Failed to evict spot instance', err)
    }
  }

  const handleDeleteSpot = async (spot: SpotInstance) => {
    if (!await confirm('Delete Spot Instance Record', `Delete the spot instance record for '${spot.vm_name}'?`, { variant: 'danger', confirmLabel: 'Delete' })) return
    try {
      await deleteSpotInstance(spot.id)
      toast.success('Spot instance record deleted')
      loadData()
    } catch (err) {
      toastFailure(toast, 'Failed to delete spot instance', err)
    }
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Availability Zones"
        onRefresh={() => void loadData()}
        refreshing={loading}
      />

      <PageLoadBanner title="Could not load availability zones" headline={loadError} onRetry={() => void loadData()} />

      <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
        <div className="bg-[var(--zf-canvas)] rounded-lg px-4 py-3 border border-[var(--zf-hairline)]">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Zones</div>
          <div className="text-2xl font-bold">{zones.length}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] rounded-lg px-4 py-3 border border-[var(--zf-hairline)]">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Running Spot Instances</div>
          <div className="text-2xl font-bold text-emerald-700">{spots.filter(s => s.status === 'running').length}</div>
        </div>
        <div className="bg-[var(--zf-canvas)] rounded-lg px-4 py-3 border border-[var(--zf-hairline)]">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Evicted</div>
          <div className="text-2xl font-bold text-amber-800">{spots.filter(s => s.status === 'evicted').length}</div>
        </div>
      </div>

      <div className="flex gap-2 border-b border-[var(--zf-hairline)]">
        <button onClick={() => setTab('zones')} className={`px-4 py-2 text-sm font-medium border-b-2 transition ${tab === 'zones' ? 'border-[var(--zf-ink)] text-[var(--zf-ink)]' : 'border-transparent text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}>Zones</button>
        <button onClick={() => setTab('spot')} className={`px-4 py-2 text-sm font-medium border-b-2 transition ${tab === 'spot' ? 'border-[var(--zf-ink)] text-[var(--zf-ink)]' : 'border-transparent text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}>Spot Instances</button>
      </div>

      {tab === 'zones' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateZone(true)} className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Zone
            </button>
          </div>
          <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Region</th>
                  <th className="p-4">Status</th>
                  <th className="p-4">Hosts</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {loading ? (
                  <tr><td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">Loading…</td></tr>
                ) : zones.length === 0 ? (
                  <tr><td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">No availability zones.</td></tr>
                ) : zones.map(z => (
                  <tr key={z.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="p-4">
                      <div className="font-medium">{z.name}</div>
                      {z.description && <div className="text-xs text-[var(--zf-muted)]">{z.description}</div>}
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{z.region}</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium border ${statusColor[z.status]}`}>{z.status}</span>
                    </td>
                    <td className="p-4 text-sm">{z.hosts.length}</td>
                    <td className="p-4">
                      <button onClick={() => handleDeleteZone(z.id, z.name)} className="text-red-600 hover:text-red-800" aria-label="Delete" title="Delete">
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {tab === 'spot' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateSpot(true)} className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Request Spot Instance
            </button>
          </div>
          <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">VM</th>
                  <th className="p-4">Max Price/hr</th>
                  <th className="p-4">Priority</th>
                  <th className="p-4">Status</th>
                  <th className="p-4">Eviction Policy</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {loading ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">Loading…</td></tr>
                ) : spots.length === 0 ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No spot instances.</td></tr>
                ) : spots.map(s => (
                  <tr key={s.id} className="hover:bg-[var(--zf-surface)]">
                    <td className="p-4 font-medium">{s.vm_name}</td>
                    <td className="p-4 text-sm font-mono">${s.max_price_per_hour.toFixed(2)}</td>
                    <td className="p-4 text-sm capitalize">{s.priority}</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium border ${spotStatusColor[s.status]}`}>{s.status}</span>
                    </td>
                    <td className="p-4 text-sm capitalize">{s.eviction_policy}</td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        {s.status === 'running' && (
                          <button onClick={() => void handleEvict(s)} className="text-amber-700 hover:text-amber-800" title="Evict">
                            <PowerOff className="w-4 h-4" />
                          </button>
                        )}
                        <button onClick={() => void handleDeleteSpot(s)} className="text-red-600 hover:text-red-800" title="Delete record">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {showCreateZone && (
        <CreateZoneModal onClose={() => setShowCreateZone(false)} onCreated={() => { setShowCreateZone(false); loadData() }} />
      )}
      {showCreateSpot && (
        <CreateSpotModal onClose={() => setShowCreateSpot(false)} onCreated={() => { setShowCreateSpot(false); loadData() }} zones={zones} />
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

function CreateZoneModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [region, setRegion] = useState('')
  const [creating, setCreating] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { toast.error('Name is required'); return }
    setCreating(true)
    try {
      await createZone({ name: name.trim(), description: description || undefined, region: region || undefined })
      toast.success(`Zone '${name}' created`)
      onCreated()
    } catch (err) {
      toastFailure(toast, 'Failed to create zone', err)
    } finally {
      setCreating(false)
    }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-4">Create Availability Zone</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">Name</label>
            <input aria-label="Name" value={name} onChange={e => setName(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" placeholder="us-east-1a" />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Description</label>
            <input aria-label="Description" value={description} onChange={e => setDescription(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Region</label>
            <input aria-label="Region" value={region} onChange={e => setRegion(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" placeholder="default" />
          </div>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
            <button type="submit" disabled={creating} className="zf-btn zf-btn-primary flex-1">{creating ? 'Creating…' : 'Create'}</button>
          </div>
        </form>
      </div>
    </div>
  )
}

function CreateSpotModal({ onClose, onCreated, zones }: { onClose: () => void; onCreated: () => void; zones: AvailabilityZone[] }) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState('')
  const [maxPrice, setMaxPrice] = useState(0.1)
  const [priority, setPriority] = useState<'low' | 'regular'>('low')
  const [zoneId, setZoneId] = useState('')
  const [evictionPolicy, setEvictionPolicy] = useState<'stop' | 'delete' | 'deallocate'>('stop')
  const [creating, setCreating] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!vmName.trim()) { toast.error('VM name is required'); return }
    if (maxPrice <= 0) { toast.error('Max price must be greater than 0'); return }
    setCreating(true)
    try {
      await createSpotInstance({
        vm_name: vmName.trim(),
        max_price_per_hour: maxPrice,
        priority,
        zone_id: zoneId || undefined,
        eviction_policy: evictionPolicy,
      })
      toast.success(`Spot instance requested for '${vmName.trim()}'`)
      onCreated()
    } catch (err) {
      toastFailure(toast, 'Failed to request spot instance', err)
    } finally {
      setCreating(false)
    }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-xl font-bold mb-1 flex items-center gap-2"><Zap className="w-5 h-5 text-[var(--zf-muted)]" /> Request Spot Instance</h2>
        <p className="text-xs text-[var(--zf-muted)] mb-4">The VM must already exist. Eviction (manual or automatic) applies the chosen policy to it immediately.</p>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">VM Name</label>
            <input aria-label="VM Name" value={vmName} onChange={e => setVmName(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" placeholder="my-vm" />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-sm font-medium mb-1">Max Price/hr ($)</label>
              <input aria-label="Max Price/hr ($)" type="number" min={0.01} step={0.01} value={maxPrice} onChange={e => setMaxPrice(Number(e.target.value))} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2" />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">Priority</label>
              <select aria-label="Priority" value={priority} onChange={e => setPriority(e.target.value as 'low' | 'regular')} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
                <option value="low">Low</option>
                <option value="regular">Regular</option>
              </select>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-sm font-medium mb-1">Zone (optional)</label>
              <select aria-label="Zone (optional)" value={zoneId} onChange={e => setZoneId(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
                <option value="">Any</option>
                {zones.map(z => <option key={z.id} value={z.id}>{z.name}</option>)}
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">Eviction Policy</label>
              <select aria-label="Eviction Policy" value={evictionPolicy} onChange={e => setEvictionPolicy(e.target.value as 'stop' | 'delete' | 'deallocate')} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2">
                <option value="stop">Stop</option>
                <option value="deallocate">Deallocate</option>
                <option value="delete">Delete</option>
              </select>
            </div>
          </div>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
            <button type="submit" disabled={creating} className="zf-btn zf-btn-primary flex-1">{creating ? 'Requesting…' : 'Request'}</button>
          </div>
        </form>
      </div>
    </div>
  )
}
