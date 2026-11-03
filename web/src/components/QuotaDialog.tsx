// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { X, Shield } from 'lucide-react'
import { createQuota, updateQuota, CreateQuotaRequest, ResourceQuota } from '../api/quota'
import { useToastContext } from '../contexts/ToastContext'

type QuotaDialogProps =
  | { mode: 'create'; quota?: never; onClose: () => void; onSuccess: () => void }
  | { mode: 'edit'; quota: ResourceQuota; onClose: () => void; onSuccess: () => void }

export default function QuotaDialog({ mode, quota, onClose, onSuccess }: QuotaDialogProps) {
  const toast = useToastContext()
  const [submitting, setSubmitting] = useState(false)
  const [formData, setFormData] = useState<CreateQuotaRequest>({
    name: quota?.name ?? '',
    max_cpus: quota?.max_cpus ?? 16,
    max_memory: quota?.max_memory ?? 16384,
    max_disk: quota?.max_disk ?? 500,
    max_vms: quota?.max_vms ?? 10,
    tags: quota?.tags ?? [],
    enabled: quota?.enabled ?? true,
  })
  const [tagInput, setTagInput] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    if (!formData.name.trim()) {
      toast.error('Quota name is required')
      return
    }

    if (formData.max_cpus <= 0 || formData.max_memory <= 0 || formData.max_disk <= 0 || formData.max_vms <= 0) {
      toast.error('All limits must be greater than 0')
      return
    }

    setSubmitting(true)
    try {
      if (mode === 'create') {
        await createQuota(formData)
        toast.success('Quota created successfully')
      } else {
        const { enabled: _, ...editPayload } = formData
        await updateQuota(quota.id, editPayload)
        toast.success('Quota updated successfully')
      }
      onSuccess()
      onClose()
    } catch (_error) {
      toast.error(mode === 'create' ? 'Failed to create quota' : 'Failed to update quota')
    } finally {
      setSubmitting(false)
    }
  }

  const handleAddTag = () => {
    const trimmedTag = tagInput.trim()
    if (trimmedTag && !formData.tags?.includes(trimmedTag)) {
      setFormData({
        ...formData,
        tags: [...(formData.tags || []), trimmedTag],
      })
      setTagInput('')
    }
  }

  const handleRemoveTag = (tag: string) => {
    setFormData({
      ...formData,
      tags: formData.tags?.filter((t) => t !== tag) || [],
    })
  }

  const handleTagKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      handleAddTag()
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-[var(--zf-canvas)] rounded-lg shadow-2xl border border-[var(--zf-hairline)] w-full max-w-2xl max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-[var(--zf-hairline)] sticky top-0 bg-[var(--zf-surface)] z-10">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-blue-500" />
            <div>
              <h2 className="text-xl font-bold">
                {mode === 'create' ? 'Create Resource Quota' : 'Edit Resource Quota'}
              </h2>
              {mode === 'edit' && (
                <p className="text-sm text-[var(--zf-muted)]">ID: {quota.id}</p>
              )}
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition" aria-label="Close" title="Close">
            <X className="w-6 h-6" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="p-6 space-y-6">
          {/* Name */}
          <div>
            <label className="block text-sm font-medium mb-2">
              Quota Name <span className="text-red-500">*</span>
            </label>
            <input
              type="text"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              placeholder="e.g., Development Team Quota"
              className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-blue-500"
              required
            />
          </div>

          {/* Current Usage Info (edit mode only) */}
          {mode === 'edit' && (
            <div className="p-4 bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
              <h4 className="text-sm font-medium mb-3">Current Usage</h4>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                <div>
                  <p className="text-[var(--zf-muted)]">CPUs</p>
                  <p className="font-medium">{quota.used_cpus} used</p>
                </div>
                <div>
                  <p className="text-[var(--zf-muted)]">Memory</p>
                  <p className="font-medium">{quota.used_memory}MB used</p>
                </div>
                <div>
                  <p className="text-[var(--zf-muted)]">Disk</p>
                  <p className="font-medium">{quota.used_disk}GB used</p>
                </div>
                <div>
                  <p className="text-[var(--zf-muted)]">VMs</p>
                  <p className="font-medium">{quota.used_vms} active</p>
                </div>
              </div>
              <p className="text-xs text-amber-600 mt-3">
                Warning: Setting limits below current usage may prevent new VM creation
              </p>
            </div>
          )}

          {/* Resource Limits */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* Max CPUs */}
            <div>
              <label className="block text-sm font-medium mb-2">
                Max CPUs <span className="text-red-500">*</span>
              </label>
              <input
                type="number"
                value={formData.max_cpus}
                onChange={(e) => setFormData({ ...formData, max_cpus: parseInt(e.target.value) || 0 })}
                min="1"
                className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
                required
              />
              {mode === 'edit' && formData.max_cpus < quota.used_cpus ? (
                <p className="text-xs text-red-600 mt-1">
                  Below current usage ({quota.used_cpus})
                </p>
              ) : (
                <p className="text-xs text-[var(--zf-muted)] mt-1">Total CPU cores allowed</p>
              )}
            </div>

            {/* Max Memory */}
            <div>
              <label className="block text-sm font-medium mb-2">
                Max Memory (MB) <span className="text-red-500">*</span>
              </label>
              <input
                type="number"
                value={formData.max_memory}
                onChange={(e) => setFormData({ ...formData, max_memory: parseInt(e.target.value) || 0 })}
                min="1"
                step="1024"
                className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
                required
              />
              {mode === 'edit' && formData.max_memory < quota.used_memory ? (
                <p className="text-xs text-red-600 mt-1">
                  Below current usage ({quota.used_memory}MB)
                </p>
              ) : (
                <p className="text-xs text-[var(--zf-muted)] mt-1">
                  {(formData.max_memory / 1024).toFixed(1)}GB total
                </p>
              )}
            </div>

            {/* Max Disk */}
            <div>
              <label className="block text-sm font-medium mb-2">
                Max Disk (GB) <span className="text-red-500">*</span>
              </label>
              <input
                type="number"
                value={formData.max_disk}
                onChange={(e) => setFormData({ ...formData, max_disk: parseInt(e.target.value) || 0 })}
                min="1"
                step="10"
                className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
                required
              />
              {mode === 'edit' && formData.max_disk < quota.used_disk ? (
                <p className="text-xs text-red-600 mt-1">
                  Below current usage ({quota.used_disk}GB)
                </p>
              ) : (
                <p className="text-xs text-[var(--zf-muted)] mt-1">Total disk space allowed</p>
              )}
            </div>

            {/* Max VMs */}
            <div>
              <label className="block text-sm font-medium mb-2">
                Max VMs <span className="text-red-500">*</span>
              </label>
              <input
                type="number"
                value={formData.max_vms}
                onChange={(e) => setFormData({ ...formData, max_vms: parseInt(e.target.value) || 0 })}
                min="1"
                className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
                required
              />
              {mode === 'edit' && formData.max_vms < quota.used_vms ? (
                <p className="text-xs text-red-600 mt-1">
                  Below current usage ({quota.used_vms} VMs)
                </p>
              ) : (
                <p className="text-xs text-[var(--zf-muted)] mt-1">Maximum number of VMs</p>
              )}
            </div>
          </div>

          {/* Tags */}
          <div>
            <label className="block text-sm font-medium mb-2">
              Apply to VMs with Tags (Optional)
            </label>
            <p className="text-xs text-[var(--zf-muted)] mb-3">
              Leave empty to apply globally, or specify tags to limit quota to tagged VMs
            </p>

            {/* Current Tags */}
            {formData.tags && formData.tags.length > 0 && (
              <div className="flex flex-wrap gap-2 mb-3">
                {formData.tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-2 px-3 py-1.5 bg-[var(--zf-link-fill)] rounded-full text-sm font-medium"
                  >
                    {tag}
                    <button
                      type="button"
                      onClick={() => handleRemoveTag(tag)}
                      className="hover:bg-black/20 rounded-full p-0.5 transition" aria-label={`Remove tag ${tag}`} title={`Remove tag ${tag}`}>
                      <X className="w-3.5 h-3.5" />
                    </button>
                  </span>
                ))}
              </div>
            )}

            {/* Add Tag Input */}
            <div className="flex gap-2">
              <input
                type="text"
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                onKeyDown={handleTagKeyDown}
                placeholder="Enter tag name..."
                className="flex-1 bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-blue-500"
              />
              <button
                type="button"
                onClick={handleAddTag}
                disabled={!tagInput.trim()}
                className="px-4 py-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Add Tag
              </button>
            </div>
          </div>

          {/* Enabled (create mode only) */}
          {mode === 'create' && (
            <div className="flex items-center gap-3">
              <input
                type="checkbox"
                id="enabled"
                checked={formData.enabled}
                onChange={(e) => setFormData({ ...formData, enabled: e.target.checked })}
                className="w-4 h-4 bg-[var(--zf-canvas)] border-[var(--zf-hairline)] rounded focus:ring-blue-500"
              />
              <label htmlFor="enabled" className="text-sm font-medium">
                Enable quota immediately
              </label>
            </div>
          )}

          {/* Footer */}
          <div className="flex items-center justify-end gap-3 pt-4 border-t border-[var(--zf-hairline)]">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="px-4 py-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] rounded-lg transition disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="px-4 py-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg transition disabled:opacity-50"
            >
              {submitting
                ? (mode === 'create' ? 'Creating...' : 'Saving...')
                : (mode === 'create' ? 'Create Quota' : 'Save Changes')
              }
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
