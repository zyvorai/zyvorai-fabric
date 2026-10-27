// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PageLoading } from '../components/Skeleton'
import { useEffect, useState } from 'react'
import { Plus, Trash2, Copy, Layers, Pencil } from 'lucide-react'
import {
  listTemplates as fetchTemplates,
  deleteTemplate as removeTemplate,
  deployTemplate,
  createTemplate,
  updateTemplate,
  VMTemplate,
} from '../api/templates'
import { listVMs, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { useNavigate } from 'react-router'
import { PageHeader, EmptyState, Modal } from '../components/ui'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'

export default function Templates() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const navigate = useNavigate()
  const [templates, setTemplates] = useState<VMTemplate[]>([])
  const [vms, setVMs] = useState<VM[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreateDialog, setShowCreateDialog] = useState(false)
  const [showSaveTemplate, setShowSaveTemplate] = useState(false)
  const [selectedTemplate, setSelectedTemplate] = useState<string | null>(null)
  const [editingTemplate, setEditingTemplate] = useState<VMTemplate | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    loadTemplates()
  }, [])

  const loadTemplates = async () => {
    setLoadError(null)
    try {
      const [data, vmList] = await Promise.all([fetchTemplates(), listVMs()])
      setTemplates(data)
      setVMs(vmList)
    } catch (error) {
      const msg = formatUserError(error)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load templates', error)
    } finally {
      setLoading(false)
    }
  }

  const handleDelete = async (id: string, name: string) => {
    const ok = await confirm('Delete Template', `Delete template '${name}'? This cannot be undone.`, { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return

    try {
      await removeTemplate(id)
      toast.success(`Template '${name}' deleted successfully`)
      loadTemplates()
    } catch (_error) {
      toastFailure(toast, `Failed to delete template '${name}'`, _error)
    }
  }

  const handleInstantiate = (templateId: string) => {
    setSelectedTemplate(templateId)
    setShowCreateDialog(true)
  }

  if (loading) return <PageLoading />

  return (
    <div className="space-y-6">
      {loadError && (
        <ErrorBanner
          title="Could not load templates"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadTemplates}
        />
      )}
      <PageHeader
        title="VM Templates"
        actions={
          <button
            onClick={() => setShowSaveTemplate(true)}
            disabled={vms.length === 0}
            title={vms.length === 0 ? 'No VMs to create a template from' : undefined}
            className="zf-btn zf-btn-primary"
          >
            <Plus className="w-4 h-4" />
            Create from VM
          </button>
        }
      />

      <div className="bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg p-4 text-sm text-[var(--zf-secondary)]">
        <p>
          Templates allow you to quickly create new VMs from pre-configured images.
          Create a template from an existing VM to save its configuration.
        </p>
      </div>

      {templates.length === 0 ? (
        <div className="zf-panel">
          <EmptyState
            icon={<Layers className="w-6 h-6" />}
            title="No templates yet"
            description="Create a template from an existing VM to get started"
            action={
              <button
                onClick={() => vms.length === 0 ? navigate('/app/vms') : setShowSaveTemplate(true)}
                className="zf-btn zf-btn-primary"
              >
                {vms.length === 0 ? 'Go to VMs' : 'Create from VM'}
              </button>
            }
          />
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {templates.map((template) => (
            <TemplateCard
              key={template.id}
              template={template}
              onDelete={() => handleDelete(template.id, template.name)}
              onInstantiate={() => handleInstantiate(template.id)}
              onEdit={() => setEditingTemplate(template)}
            />
          ))}
        </div>
      )}

      {showCreateDialog && selectedTemplate && (
        <CreateVMFromTemplateDialog
          templateId={selectedTemplate}
          onClose={() => {
            setShowCreateDialog(false)
            setSelectedTemplate(null)
          }}
          onSuccess={() => {
            toast.success('VM created from template successfully')
            navigate('/app/vms')
          }}
        />
      )}

      {showSaveTemplate && (
        <SaveTemplateDialog
          vms={vms}
          onClose={() => setShowSaveTemplate(false)}
          onSuccess={() => {
            toast.success('Template saved')
            setShowSaveTemplate(false)
            loadTemplates()
          }}
        />
      )}

      {editingTemplate && (
        <EditTemplateDialog
          template={editingTemplate}
          onClose={() => setEditingTemplate(null)}
          onSuccess={() => {
            toast.success('Template updated')
            setEditingTemplate(null)
            loadTemplates()
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

function TemplateCard({
  template,
  onDelete,
  onInstantiate,
  onEdit,
}: {
  template: VMTemplate
  onDelete: () => void
  onInstantiate: () => void
  onEdit: () => void
}) {
  return (
    <div className="zf-panel p-6 transition">
      <div className="flex items-start justify-between mb-4">
        <div>
          <h3 className="text-xl font-bold mb-2 text-[var(--zf-ink)]">{template.name}</h3>
          <p className="text-sm text-[var(--zf-muted)]">{template.description || 'No description'}</p>
        </div>
      </div>

      <div className="space-y-2 mb-4 text-sm">
        <div className="flex items-center justify-between">
          <span className="text-[var(--zf-muted)]">CPUs</span>
          <span className="font-medium">{template.cpus}</span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-[var(--zf-muted)]">Memory</span>
          <span className="font-medium">{template.memory} MB</span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-[var(--zf-muted)]">Disk Size</span>
          <span className="font-medium">{template.disk} GB</span>
        </div>
        {template.tags.length > 0 && (
          <div className="flex items-center justify-between">
            <span className="text-[var(--zf-muted)]">Tags</span>
            <span className="font-medium text-xs">{template.tags.join(', ')}</span>
          </div>
        )}
        <div className="flex items-center justify-between">
          <span className="text-[var(--zf-muted)]">Created</span>
          <span className="font-medium text-xs">{new Date(template.created).toLocaleDateString()}</span>
        </div>
      </div>

      <div className="flex gap-2">
        <button
          onClick={onInstantiate}
          className="flex-1 zf-btn zf-btn-primary"
        >
          <Copy className="w-4 h-4" />
          Create VM
        </button>
        <button
          onClick={onEdit}
          title="Edit template"
          className="zf-btn zf-btn-ghost"
        >
          <Pencil className="w-4 h-4" />
        </button>
        <button
          onClick={onDelete}
          className="zf-btn zf-btn-danger"
        >
          <Trash2 className="w-4 h-4" />
        </button>
      </div>
    </div>
  )
}

function CreateVMFromTemplateDialog({
  templateId,
  onClose,
  onSuccess,
}: {
  templateId: string
  onClose: () => void
  onSuccess: () => void
}) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState('')
  const [isCreating, setIsCreating] = useState(false)

  const handleCreate = async () => {
    if (!vmName.trim()) {
      toast.error('Please enter a VM name')
      return
    }

    setIsCreating(true)
    try {
      await deployTemplate(templateId, vmName)
      onSuccess()
      onClose()
    } catch (error) {
      toastFailure(toast, 'Failed to create VM from template', error)
    } finally {
      setIsCreating(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold text-[var(--zf-ink)]">Create VM from Template</h2>
        <button onClick={onClose} className="p-2 hover:bg-black/[0.04] rounded transition">
          <span className="text-2xl text-[var(--zf-muted)]">&times;</span>
        </button>
      </div>

      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">VM Name</label>
          <input
            type="text"
            value={vmName}
            onChange={(e) => setVmName(e.target.value)}
            placeholder="Enter VM name"
            className="input-field"
            autoFocus
          />
        </div>
      </div>

      <div className="flex justify-end gap-2 mt-6">
        <button
          onClick={onClose}
          disabled={isCreating}
          className="zf-btn zf-btn-ghost"
        >
          Cancel
        </button>
        <button
          onClick={handleCreate}
          disabled={isCreating}
          className="zf-btn zf-btn-primary"
        >
          {isCreating ? 'Creating...' : 'Create VM'}
        </button>
      </div>
    </Modal>
  )
}

function SaveTemplateDialog({ vms, onClose, onSuccess }: { vms: VM[]; onClose: () => void; onSuccess: () => void }) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState(vms[0]?.name || '')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSubmitting(true)
    try {
      // cpus/memory/disk/image are required by the API shape but ignored by
      // the backend whenever from_vm is set -- it pulls the real config from
      // that VM instead.
      await createTemplate({
        name,
        description: description || undefined,
        from_vm: vmName,
        cpus: 1,
        memory: 0,
        disk: 0,
        image: '',
      })
      onSuccess()
    } catch (error) {
      toastFailure(toast, 'Failed to save template', error)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold text-[var(--zf-ink)]">Create Template from VM</h2>
        <button onClick={onClose} className="p-2 hover:bg-black/[0.04] rounded transition">
          <span className="text-2xl text-[var(--zf-muted)]">&times;</span>
        </button>
      </div>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Source VM</label>
          <select value={vmName} onChange={e => setVmName(e.target.value)}
            className="input-field">
            {vms.map(v => <option key={v.name} value={v.name}>{v.name}</option>)}
          </select>
          <p className="text-xs text-[var(--zf-muted)] mt-1">The template captures this VM's CPU, memory, disk size, image, and tags.</p>
        </div>
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Template Name</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)} placeholder="e.g. web-server-baseline"
            className="input-field" required autoFocus />
        </div>
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Description</label>
          <input type="text" value={description} onChange={e => setDescription(e.target.value)}
            className="input-field" />
        </div>
        <div className="flex justify-end gap-2 pt-2">
          <button type="button" onClick={onClose} disabled={submitting}
            className="zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" disabled={submitting}
            className="zf-btn zf-btn-primary">{submitting ? 'Saving...' : 'Save Template'}</button>
        </div>
      </form>
    </Modal>
  )
}

function EditTemplateDialog({ template, onClose, onSuccess }: { template: VMTemplate; onClose: () => void; onSuccess: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState(template.name)
  const [description, setDescription] = useState(template.description || '')
  const [cpus, setCpus] = useState(String(template.cpus))
  const [memory, setMemory] = useState(String(template.memory))
  const [disk, setDisk] = useState(String(template.disk))
  const [tags, setTags] = useState(template.tags.join(', '))
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const cpusNum = parseInt(cpus)
    const memoryNum = parseInt(memory)
    const diskNum = parseInt(disk)
    if (!Number.isInteger(cpusNum) || cpusNum < 1) { setError('CPUs must be a positive integer'); return }
    if (!Number.isInteger(memoryNum) || memoryNum < 1) { setError('Memory must be a positive integer (MB)'); return }
    if (!Number.isInteger(diskNum) || diskNum < 1) { setError('Disk must be a positive integer (GB)'); return }
    setError('')
    setSubmitting(true)
    try {
      await updateTemplate(template.id, {
        name,
        description: description || undefined,
        cpus: cpusNum,
        memory: memoryNum,
        disk: diskNum,
        tags: tags.split(',').map(t => t.trim()).filter(Boolean),
      })
      onSuccess()
    } catch (err) {
      const msg = formatUserError(err)
      setError(msg)
      toastFailure(toast, 'Failed to update template', err)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold text-[var(--zf-ink)]">Edit Template</h2>
        <button onClick={onClose} className="p-2 hover:bg-black/[0.04] rounded transition">
          <span className="text-2xl text-[var(--zf-muted)]">&times;</span>
        </button>
      </div>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Template Name</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)}
            className="input-field" required autoFocus />
        </div>
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Description</label>
          <input type="text" value={description} onChange={e => setDescription(e.target.value)}
            className="input-field" />
        </div>
        <div className="grid grid-cols-3 gap-3">
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">CPUs</label>
            <input type="number" min={1} value={cpus} onChange={e => setCpus(e.target.value)}
              className="input-field" required />
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Memory (MB)</label>
            <input type="number" min={1} value={memory} onChange={e => setMemory(e.target.value)}
              className="input-field" required />
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Disk (GB)</label>
            <input type="number" min={1} value={disk} onChange={e => setDisk(e.target.value)}
              className="input-field" required />
          </div>
        </div>
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Tags (comma-separated)</label>
          <input type="text" value={tags} onChange={e => setTags(e.target.value)} placeholder="prod, web"
            className="input-field" />
        </div>
        {error && <p className="text-[var(--zf-danger)] text-sm">{error}</p>}
        <div className="flex justify-end gap-2 pt-2">
          <button type="button" onClick={onClose} disabled={submitting}
            className="zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" disabled={submitting}
            className="zf-btn zf-btn-primary">{submitting ? 'Saving...' : 'Save Changes'}</button>
        </div>
      </form>
    </Modal>
  )
}
