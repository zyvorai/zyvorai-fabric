// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { X, Copy } from 'lucide-react'
import { cloneVM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'

interface CloneVMDialogProps {
  vmName: string
  onClose: () => void
  onSuccess: () => void
}

export default function CloneVMDialog({ vmName, onClose, onSuccess }: CloneVMDialogProps) {
  const toast = useToastContext()
  const [targetName, setTargetName] = useState(`${vmName}-clone`)
  const [includeSnapshots, setIncludeSnapshots] = useState(false)
  const [linkedClone, setLinkedClone] = useState(false)
  const [isCloning, setIsCloning] = useState(false)

  const handleClone = async () => {
    if (!targetName.trim()) {
      toast.error('Please enter a name for the cloned VM')
      return
    }

    setIsCloning(true)
    try {
      await cloneVM(vmName, targetName, {
        includeSnapshots,
        linkedClone,
      })
      toast.success(`VM '${vmName}' cloned to '${targetName}' successfully`)
      onSuccess()
      onClose()
    } catch (error) {
      toastFailure(toast, 'Failed to clone VM', error)
    } finally {
      setIsCloning(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-[var(--zf-canvas)] rounded-lg shadow-2xl border border-[var(--zf-hairline)] w-full max-w-md">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-[var(--zf-hairline)]">
          <div className="flex items-center gap-3">
            <Copy className="w-6 h-6 text-[var(--zf-link)]" />
            <h2 className="text-xl font-bold">Clone VM</h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 hover:bg-white/[0.03] rounded transition"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-4">
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">
              Source VM
            </label>
            <input
              type="text"
              value={vmName}
              disabled
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-muted)]"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">
              New VM Name
            </label>
            <input
              type="text"
              value={targetName}
              onChange={(e) => setTargetName(e.target.value)}
              placeholder="Enter name for cloned VM"
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
            />
          </div>

          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="includeSnapshots"
                checked={includeSnapshots}
                onChange={(e) => setIncludeSnapshots(e.target.checked)}
                className="w-4 h-4 text-blue-600 bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-blue-500"
              />
              <label htmlFor="includeSnapshots" className="text-sm text-[var(--zf-ink)]">
                Include snapshots
              </label>
            </div>

            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="linkedClone"
                checked={linkedClone}
                onChange={(e) => setLinkedClone(e.target.checked)}
                className="w-4 h-4 text-blue-600 bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-blue-500"
              />
              <label htmlFor="linkedClone" className="text-sm text-[var(--zf-ink)]">
                Linked clone (faster, uses less space)
              </label>
            </div>
          </div>

          <div className="bg-blue-500/10 border border-blue-500/20 rounded-lg p-3 text-sm text-[var(--zf-link)]">
            {linkedClone ? (
              <p>Linked clone creates a VM that shares disk with the source. Changes won't affect the source.</p>
            ) : (
              <p>Full clone creates an independent copy of the VM. This may take some time.</p>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 p-6 border-t border-[var(--zf-hairline)]">
          <button
            onClick={onClose}
            disabled={isCloning}
            className="px-4 py-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] rounded-lg transition disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            onClick={handleClone}
            disabled={isCloning}
            className="px-4 py-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white rounded-lg transition disabled:opacity-50 flex items-center gap-2"
          >
            {isCloning ? (
              <>
                <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-white"></div>
                Cloning...
              </>
            ) : (
              <>
                <Copy className="w-4 h-4" />
                Clone VM
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  )
}
