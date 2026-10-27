// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { Camera, Plus, Trash2, RotateCcw } from 'lucide-react'
import {
  listSnapshots,
  createSnapshotWithRetry,
  deleteSnapshot,
  revertSnapshot,
  type VMSnapshot,
} from '../api/snapshots'
import { useConfirm } from '../hooks/useConfirm'
import { useToastContext } from '../contexts/ToastContext'
import ConfirmDialog from '../components/ConfirmDialog'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import RelativeTime from '../components/RelativeTime'
import { PageHeader } from '../components/ui'

export default function Snapshots() {
  const { confirmState, confirm, cancel } = useConfirm()
  const toast = useToastContext()
  const [vmName, setVmName] = useState('')
  const [snapshots, setSnapshots] = useState<VMSnapshot[]>([])
  const [loading, setLoading] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [showCreateDialog, setShowCreateDialog] = useState(false)

  const loadSnapshots = async () => {
    if (!vmName) return
    setLoading(true)
    setLoadError(null)
    try {
      const data = await listSnapshots(vmName)
      setSnapshots(data)
    } catch (error) {
      const msg = formatUserError(error)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load snapshots', error)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (vmName) loadSnapshots()
  }, [vmName])

  const handleDelete = async (id: string) => {
    if (!await confirm('Delete Snapshot', 'Are you sure you want to delete this snapshot?')) return
    try {
      await deleteSnapshot(vmName, id)
      await loadSnapshots()
    } catch (error) {
      toastFailure(toast, 'Failed to delete snapshot', error)
    }
  }

  const handleRevert = async (id: string) => {
    if (!await confirm('Revert Snapshot', 'Revert VM to this snapshot? The VM must be stopped.', { confirmLabel: 'Revert', variant: 'warning' })) return
    try {
      await revertSnapshot(vmName, id)
      toast.success('Successfully reverted to snapshot')
    } catch (error) {
      toastFailure(toast, 'Failed to revert snapshot', error)
    }
  }

  return (
    <div className="p-8">
      {loadError && vmName && (
        <ErrorBanner
          title="Could not load snapshots"
          headline={loadError}
          onRetry={loadSnapshots}
        />
      )}
      <PageHeader title="VM Snapshots" description="Create and manage VM snapshots" />

      {/* VM selector */}
      <div className="zf-panel p-6 mb-8">
        <div className="flex items-center gap-4">
          <div className="flex-1">
            <label className="block text-sm font-medium mb-2">VM Name</label>
            <input
              type="text"
              value={vmName}
              onChange={(e) => setVmName(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-4 py-2"
              placeholder="Enter VM name"
            />
          </div>
          <button
            onClick={loadSnapshots}
            disabled={!vmName}
            className="zf-btn zf-btn-ghost mt-7"
          >
            Load Snapshots
          </button>
          <button
            onClick={() => setShowCreateDialog(true)}
            disabled={!vmName}
            className="zf-btn zf-btn-primary mt-7"
          >
            <Plus className="w-4 h-4" />
            Create
          </button>
        </div>
      </div>

      {/* Snapshots list */}
      {vmName && (
        <div className="zf-panel overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="text-left text-[var(--zf-muted)] text-sm">
                <th className="p-4">Name</th>
                <th className="p-4">Type</th>
                <th className="p-4">Description</th>
                <th className="p-4">Created</th>
                <th className="p-4">Actions</th>
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr>
                  <td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">
                    Loading snapshots...
                  </td>
                </tr>
              ) : snapshots.length === 0 ? (
                <tr>
                  <td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">
                    No snapshots found for this VM.
                  </td>
                </tr>
              ) : (
                snapshots.map((snap) => (
                  <tr key={snap.id} className="border-t border-[var(--zf-hairline)] hover:bg-[var(--zf-surface)]">
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        <Camera className="w-4 h-4 text-[var(--zf-link)]" />
                        <span className="font-medium">{snap.name}</span>
                      </div>
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{snap.snapshot_type}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{snap.description || '-'}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">
                      <RelativeTime date={snap.created} />
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        <button
                          onClick={() => handleRevert(snap.id)}
                          className="p-2 hover:bg-black/[0.04] rounded transition"
                          title="Revert to snapshot"
                        >
                          <RotateCcw className="w-4 h-4 text-amber-600" />
                        </button>
                        <button
                          onClick={() => handleDelete(snap.id)}
                          className="p-2 hover:bg-black/[0.04] rounded transition"
                          title="Delete snapshot"
                        >
                          <Trash2 className="w-4 h-4 text-red-600" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      )}

      {showCreateDialog && (
        <CreateSnapshotDialog
          vmName={vmName}
          onClose={() => setShowCreateDialog(false)}
          onCreated={loadSnapshots}
        />
      )}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel ?? 'Delete'}
          variant={confirmState.variant ?? 'danger'}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}

function CreateSnapshotDialog({
  vmName,
  onClose,
  onCreated,
}: {
  vmName: string
  onClose: () => void
  onCreated: () => void
}) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [snapshotType, setSnapshotType] = useState<'Disk' | 'Full'>('Disk')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSubmitting(true)
    try {
      await createSnapshotWithRetry(vmName, {
        name,
        description: description || undefined,
        snapshot_type: snapshotType,
      })
      toast.success(
        snapshotType === 'Full'
          ? `Full snapshot '${name}' created`
          : `Snapshot '${name}' created`,
      )
      onCreated()
      onClose()
    } catch (error) {
      toastFailure(toast, 'Failed to create snapshot', error)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="modal-backdrop">
      <div className="modal-card w-full max-w-md">
        <h2 className="text-2xl font-bold mb-6">Create Snapshot</h2>
        <form onSubmit={handleSubmit}>
          <div className="mb-4">
            <label className="block text-sm font-medium mb-2">Snapshot Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-4 py-2"
              placeholder="my-snapshot"
              required
              disabled={submitting}
            />
          </div>
          <div className="mb-4">
            <label className="block text-sm font-medium mb-2">Description</label>
            <input
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-4 py-2"
              placeholder="Optional description"
              disabled={submitting}
            />
          </div>
          <div className="mb-6">
            <label className="block text-sm font-medium mb-2">Type</label>
            <select
              value={snapshotType}
              onChange={(e) => setSnapshotType(e.target.value as 'Disk' | 'Full')}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-4 py-2"
              disabled={submitting}
            >
              <option value="Disk">Disk Only</option>
              <option value="Full">Full (disk + memory — slower)</option>
            </select>
            {snapshotType === 'Full' && (
              <p className="text-xs text-[var(--zf-muted)] mt-2">
                Full snapshots can take several minutes under host load. Prefer Disk Only for routine checkpoints.
              </p>
            )}
            {submitting && (
              <p className="text-xs text-[var(--zf-muted)] mt-2">
                {snapshotType === 'Full'
                  ? 'Creating full snapshot — this may take a few minutes…'
                  : 'Creating snapshot…'}
              </p>
            )}
          </div>
          <div className="flex gap-4">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="zf-btn zf-btn-ghost flex-1"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting || !name.trim()}
              className="zf-btn zf-btn-primary flex-1"
            >
              {submitting ? 'Creating…' : 'Create'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
