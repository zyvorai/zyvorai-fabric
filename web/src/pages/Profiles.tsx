// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useCallback } from 'react'
import { Cpu, MemoryStick, HardDrive, Plus, Trash2 } from 'lucide-react'
import { listProfiles, createProfile, deleteProfile, VMProfile } from '../api/profiles'
import { useToastContext } from '../contexts/ToastContext'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'

export default function Profiles() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [profiles, setProfiles] = useState<VMProfile[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load profiles')
  const [showCreateDialog, setShowCreateDialog] = useState(false)

  const loadProfiles = useCallback(() => {
    return run(async () => {
      const data = await listProfiles()
      setProfiles(data)
    })
  }, [run])

  useEffect(() => {
    void loadProfiles()
  }, [loadProfiles])

  const handleDelete = async (name: string) => {
    if (!await confirm('Delete Profile', `Delete profile '${name}'?`, { variant: 'danger', confirmLabel: 'Delete' })) return
    try {
      await deleteProfile(name)
      toast.success(`Profile '${name}' deleted`)
      loadProfiles()
    } catch (err) {
      toastFailure(toast, `Failed to delete profile '${name}'`, err)
    }
  }

  const categoryColors: Record<string, string> = {
    general: 'bg-blue-50 text-[var(--zf-link)] border-blue-100',
    compute: 'bg-amber-50 text-amber-800 border-amber-200',
    memory: 'bg-violet-50 text-violet-700 border-violet-200',
    storage: 'bg-emerald-50 text-emerald-700 border-emerald-200',
    gpu: 'bg-red-50 text-red-700 border-red-200',
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Instance Types"
        onRefresh={() => void loadProfiles()}
        refreshing={loading}
        actions={
          <button onClick={() => setShowCreateDialog(true)} className="zf-btn zf-btn-primary"><Plus className="w-4 h-4" />Create Profile</button>
        }
      />
      <PageLoadBanner title="Could not load profiles" headline={loadError} onRetry={() => void loadProfiles()} />
      {loading && !loadError && (
        <div className="flex items-center justify-center h-32"><div className="animate-spin rounded-full h-12 w-12 border-b-2 border-[var(--zf-link)]" /></div>
      )}
      {!loadError && (

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {profiles.map(p => (
          <div key={p.name} className="zf-panel-muted p-6 transition">
            <div className="flex items-start justify-between mb-3">
              <div>
                <h3 className="text-lg font-bold">{p.name}</h3>
                <span className={`inline-block mt-1 px-2 py-0.5 rounded text-xs border ${categoryColors[p.category] || categoryColors.general}`}>{p.category}</span>
              </div>
              {!p.builtin && <button onClick={() => handleDelete(p.name)} className="p-1 rounded text-[var(--zf-muted)] hover:bg-red-50 hover:text-red-600 transition-colors" aria-label="Delete" title="Delete"><Trash2 className="w-4 h-4" /></button>}
            </div>
            <p className="text-sm text-[var(--zf-muted)] mb-4">{p.description}</p>
            <div className="space-y-2 text-sm">
              <div className="flex justify-between"><span className="text-[var(--zf-muted)] flex items-center gap-1"><Cpu className="w-3.5 h-3.5" />CPUs</span><span className="font-medium">{p.cpus}</span></div>
              <div className="flex justify-between"><span className="text-[var(--zf-muted)] flex items-center gap-1"><MemoryStick className="w-3.5 h-3.5" />Memory</span><span className="font-medium">{p.memory >= 1024 ? `${p.memory / 1024} GB` : `${p.memory} MB`}</span></div>
              <div className="flex justify-between"><span className="text-[var(--zf-muted)] flex items-center gap-1"><HardDrive className="w-3.5 h-3.5" />Disk</span><span className="font-medium">{p.disk} GB</span></div>
              {p.network_bandwidth && <div className="flex justify-between"><span className="text-[var(--zf-muted)]">Network</span><span className="font-medium">{p.network_bandwidth}</span></div>}
            </div>
            {p.builtin && <div className="mt-3 text-xs text-[var(--zf-muted)]">Built-in</div>}
          </div>
        ))}
      </div>
      )}

      {showCreateDialog && <CreateProfileDialog onClose={() => setShowCreateDialog(false)} onSuccess={() => { toast.success('Profile created'); setShowCreateDialog(false); void loadProfiles() }} />}

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

function CreateProfileDialog({ onClose, onSuccess }: { onClose: () => void; onSuccess: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [cpus, setCpus] = useState(2)
  const [memory, setMemory] = useState(2048)
  const [disk, setDisk] = useState(20)
  const [category, setCategory] = useState('general')

  const handleCreate = async () => {
    if (!name.trim()) { toast.error('Enter a profile name'); return }
    try {
      await createProfile({ name, description: `Custom ${name} profile`, cpus, memory, disk, category: category as VMProfile['category'], network_bandwidth: undefined })
      onSuccess()
    } catch (e) {
      toastFailure(toast, 'Failed to create profile', e)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)] w-full max-w-md">
        <div className="flex items-center justify-between p-6 border-b border-[var(--zf-hairline)]"><h2 className="text-xl font-bold">Create Profile</h2><button onClick={onClose} className="p-2 hover:bg-white/[0.03] rounded"><span className="text-2xl">&times;</span></button></div>
        <div className="p-6 space-y-4">
          <div><label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Name</label><input aria-label="Name" value={name} onChange={e => setName(e.target.value)} placeholder="my-profile" className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]" autoFocus /></div>
          <div><label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Category</label><select aria-label="Category" value={category} onChange={e => setCategory(e.target.value)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)]"><option value="general">General</option><option value="compute">Compute</option><option value="memory">Memory</option><option value="storage">Storage</option></select></div>
          <div className="grid grid-cols-3 gap-3">
            <div><label className="block text-sm text-[var(--zf-ink)] mb-1">CPUs</label><input aria-label="CPUs" type="number" value={cpus} onChange={e => setCpus(+e.target.value)} min={1} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-3 text-[var(--zf-ink)]" /></div>
            <div><label className="block text-sm text-[var(--zf-ink)] mb-1">Memory (MB)</label><input aria-label="Memory (MB)" type="number" value={memory} onChange={e => setMemory(+e.target.value)} min={256} step={256} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-3 text-[var(--zf-ink)]" /></div>
            <div><label className="block text-sm text-[var(--zf-ink)] mb-1">Disk (GB)</label><input aria-label="Disk (GB)" type="number" value={disk} onChange={e => setDisk(+e.target.value)} min={1} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-3 text-[var(--zf-ink)]" /></div>
          </div>
        </div>
        <div className="flex justify-end gap-2 p-6 border-t border-[var(--zf-hairline)]">
          <button onClick={onClose} className="zf-btn zf-btn-ghost">Cancel</button>
          <button onClick={handleCreate} className="zf-btn zf-btn-primary">Create</button>
        </div>
      </div>
    </div>
  )
}
