// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PageLoading } from '../components/Skeleton'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import { PackageCheck, Plus, Trash2, Zap, X, HardDrive } from 'lucide-react'
import { listWarmPools, createWarmPool, deleteWarmPool, claimWarmPool, WarmPool } from '../api/warmPools'
import { listImages, ImageInfo } from '../api/images'
import { PageHeader, EmptyState, Modal } from '../components/ui'
import ErrorBanner from '../components/ErrorBanner'
import ConfirmDialog from '../components/ConfirmDialog'
import { useConfirm } from '../hooks/useConfirm'
import { useToastContext } from '../contexts/ToastContext'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'

export default function WarmPools() {
  const navigate = useNavigate()
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [pools, setPools] = useState<WarmPool[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [showCreate, setShowCreate] = useState(false)
  const [claimTarget, setClaimTarget] = useState<WarmPool | null>(null)

  const load = async () => {
    setLoadError(null)
    try {
      setPools(await listWarmPools())
    } catch (err) {
      setLoadError(formatUserError(err))
      toastFailure(toast, 'Failed to load warm pools', err)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { load() }, [])

  const handleDelete = async (pool: WarmPool) => {
    const ok = await confirm('Delete Warm Pool', `Delete pool '${pool.name}' and its ${pool.ready_members} pre-booted member(s)? This cannot be undone.`, { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try {
      await deleteWarmPool(pool.name)
      // Tearing down every member VM happens in the background -- the pool
      // itself is already gone from the list, members finish shutting down
      // shortly after.
      toast.success(`Pool '${pool.name}' deleted`)
    } catch (err) {
      toastFailure(toast, `Failed to delete pool '${pool.name}'`, err)
    } finally {
      load()
    }
  }

  if (loading) return <PageLoading />

  return (
    <div className="space-y-6">
      {loadError && (
        <ErrorBanner title="Could not load warm pools" headline={loadError} hints={hintsForError(loadError)} onRetry={load} />
      )}
      <PageHeader
        title="Warm Pools"
        description="Pre-booted, paused VMs ready to claim instantly instead of cold-creating"
        icon={PackageCheck}
        primaryAction={
          <button
            onClick={() => setShowCreate(true)}
            className="zf-btn zf-btn-primary"
          >
            <Plus className="w-4 h-4" />
            Create Pool
          </button>
        }
      />

      {pools.length === 0 ? (
        <div className="zf-panel">
          <EmptyState
            icon={<PackageCheck className="w-6 h-6" />}
            title="No warm pools yet"
            description="Create a pool to keep N VMs pre-booted and paused, ready to hand out instantly on claim"
            action={
              <button onClick={() => setShowCreate(true)} className="zf-btn zf-btn-primary">
                Create Pool
              </button>
            }
          />
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {pools.map((pool) => (
            <div key={pool.name} className="zf-panel p-5">
              <div className="flex items-start justify-between gap-3 mb-3">
                <div className="flex items-center gap-2.5 min-w-0">
                  <div className="icon-tile icon-tile-sm icon-tile-green shrink-0">
                    <PackageCheck className="w-4 h-4" />
                  </div>
                  <h3 className="font-semibold text-[var(--zf-ink)] truncate">{pool.name}</h3>
                </div>
                <button
                  onClick={() => handleDelete(pool)}
                  className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-[var(--zf-danger)] hover:bg-red-50 transition-colors shrink-0"
                  title="Delete pool"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>

              <div className="flex items-center gap-1.5 text-xs text-[var(--zf-muted)] mb-3 font-mono truncate">
                <HardDrive className="w-3.5 h-3.5 shrink-0" />
                {pool.image}
              </div>

              <div className="flex items-center justify-between text-sm mb-1.5">
                <span className="text-[var(--zf-muted)]">Ready to claim</span>
                <span className={pool.ready_members > 0 ? 'text-emerald-700 font-medium' : 'text-amber-800 font-medium'}>
                  {pool.ready_members} / {pool.size}
                </span>
              </div>
              <div className="h-1.5 rounded-full bg-[var(--zf-canvas)] overflow-hidden mb-4">
                <div
                  className="h-full bg-[var(--zf-success)] transition-all duration-500"
                  style={{ width: `${pool.size > 0 ? (pool.ready_members / pool.size) * 100 : 0}%` }}
                />
              </div>

              <div className="flex items-center gap-3 text-xs text-[var(--zf-muted)] mb-4">
                <span>{pool.cpus} vCPUs</span>
                <span>·</span>
                <span>{pool.memory} MB</span>
              </div>

              <button
                onClick={() => setClaimTarget(pool)}
                disabled={pool.ready_members === 0}
                className="w-full zf-btn zf-btn-primary"
                title={pool.ready_members === 0 ? 'No ready members right now' : undefined}
              >
                <Zap className="w-3.5 h-3.5" />
                Claim Instantly
              </button>
            </div>
          ))}
        </div>
      )}

      {showCreate && (
        <CreatePoolModal
          onClose={() => setShowCreate(false)}
          onCreated={() => { setShowCreate(false); load() }}
        />
      )}

      {claimTarget && (
        <ClaimPoolModal
          pool={claimTarget}
          onClose={() => setClaimTarget(null)}
          onClaimed={(vmName) => {
            setClaimTarget(null)
            toast.success(`Claimed '${vmName}' from pool '${claimTarget.name}'`)
            navigate(`/app/vms/${vmName}`)
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

function CreatePoolModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [images, setImages] = useState<ImageInfo[]>([])
  const [name, setName] = useState('')
  const [size, setSize] = useState(3)
  const [image, setImage] = useState('')
  const [cpus, setCpus] = useState(2)
  const [memory, setMemory] = useState(2048)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    listImages().then((data) => {
      setImages(data)
      setImage((prev) => prev || data[0]?.path || '')
    }).catch(() => {})
  }, [])

  const handleSubmit = async () => {
    setError(null)
    if (!image.trim()) {
      setError('An image is required')
      return
    }
    setSubmitting(true)
    try {
      await createWarmPool({ name, size, image, cpus, memory })
      toast.success(`Pool '${name}' is booting ${size} member(s)`)
      onCreated()
    } catch (err) {
      toastFailure(toast, 'Failed to create pool', err)
      setError(formatUserError(err))
      setSubmitting(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="icon-tile icon-tile-md icon-tile-green">
            <PackageCheck className="w-5 h-5" />
          </div>
          <h2 className="text-lg font-bold text-[var(--zf-ink)]">Create Warm Pool</h2>
        </div>
        {!submitting && (
          <button onClick={onClose} className="p-2 hover:bg-black/[0.04] rounded transition text-[var(--zf-muted)] hover:text-[var(--zf-ink)]">
            <X className="w-4 h-4" />
          </button>
        )}
      </div>
      <div className="space-y-4">
        {error && (
          <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{error}</div>
        )}
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Pool Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. web-workers"
            disabled={submitting}
            className="input-field font-mono text-sm"
            required
            autoFocus
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Disk Image</label>
          <select
            value={image}
            onChange={(e) => setImage(e.target.value)}
            disabled={submitting}
            className="input-field text-sm"
          >
            {images.length === 0 && <option value="">No catalog images found</option>}
            {images.map((img) => (
              <option key={img.path} value={img.path}>{img.name}</option>
            ))}
          </select>
        </div>
        <div className="grid grid-cols-3 gap-3">
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Size</label>
            <input
              type="number" min={1} max={64} value={size}
              onChange={(e) => setSize(Math.max(1, Math.min(64, parseInt(e.target.value) || 1)))}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">vCPUs</label>
            <input
              type="number" min={1} max={32} value={cpus}
              onChange={(e) => setCpus(Math.max(1, Math.min(32, parseInt(e.target.value) || 1)))}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Memory (MB)</label>
            <input
              type="number" min={256} step={256} value={memory}
              onChange={(e) => setMemory(Math.max(256, parseInt(e.target.value) || 256))}
              disabled={submitting}
              className="input-field text-sm"
            />
          </div>
        </div>
        <p className="text-xs text-[var(--zf-muted)]">
          Each member boots fully, then pauses -- claiming resumes one instantly instead of a cold create. The pool backfills automatically after every claim.
        </p>
        <div className="flex justify-end gap-2 pt-2">
          <button type="button" onClick={onClose} disabled={submitting} className="zf-btn zf-btn-ghost">
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={submitting || !name || !image}
            className="zf-btn zf-btn-primary"
          >
            {submitting && <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />}
            {submitting ? 'Creating…' : 'Create Pool'}
          </button>
        </div>
      </div>
    </Modal>
  )
}

function ClaimPoolModal({ pool, onClose, onClaimed }: { pool: WarmPool; onClose: () => void; onClaimed: (vmName: string) => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleSubmit = async () => {
    setError(null)
    setSubmitting(true)
    try {
      const vm = await claimWarmPool(pool.name, name)
      onClaimed(vm.name)
    } catch (err) {
      toastFailure(toast, 'Failed to claim from pool', err)
      setError(formatUserError(err))
      setSubmitting(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-sm">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="icon-tile icon-tile-md icon-tile-green">
            <Zap className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-lg font-bold text-[var(--zf-ink)]">Claim from '{pool.name}'</h2>
            <p className="text-xs text-[var(--zf-muted)]">Instantly resumes an already-booted member</p>
          </div>
        </div>
        {!submitting && (
          <button onClick={onClose} className="p-2 hover:bg-black/[0.04] rounded transition text-[var(--zf-muted)] hover:text-[var(--zf-ink)]">
            <X className="w-4 h-4" />
          </button>
        )}
      </div>
      <div className="space-y-4">
        {error && (
          <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">{error}</div>
        )}
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">New VM Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && name && !submitting) handleSubmit() }}
            placeholder="e.g. web-worker-7"
            disabled={submitting}
            className="input-field font-mono text-sm"
            required
            autoFocus
          />
        </div>
        <div className="flex justify-end gap-2 pt-2">
          <button type="button" onClick={onClose} disabled={submitting} className="zf-btn zf-btn-ghost">
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={submitting || !name}
            className="zf-btn zf-btn-primary"
          >
            {submitting && <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />}
            {submitting ? 'Claiming…' : 'Claim'}
          </button>
        </div>
      </div>
    </Modal>
  )
}
