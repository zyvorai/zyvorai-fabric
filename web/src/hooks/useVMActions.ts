// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback } from 'react'
import { startVM, stopVM, restartVM, pauseVM, resumeVM, deleteVM } from '../api/vm'
import { createBackup } from '../api/backup'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'
import { usePermissions } from './usePermissions'

export function useVMActions(vmName: string, onSuccess?: () => void) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()

  const performAction = useCallback(async (
    action: (name: string) => Promise<unknown>,
    actionName: string,
  ) => {
    if (!canWrite) {
      toast.error('You do not have permission to perform this action')
      return
    }
    try {
      await action(vmName)
      toast.success(`VM '${vmName}' ${actionName} successfully`)
      onSuccess?.()
    } catch (error) {
      toastFailure(toast, `Failed to ${actionName} VM '${vmName}'`, error)
    }
  }, [vmName, onSuccess, toast, canWrite])

  const handleBackup = useCallback(async () => {
    if (!canWrite) {
      toast.error('You do not have permission to perform this action')
      return
    }
    try {
      await createBackup({ vm_name: vmName, backup_type: 'full' })
      toast.success(`Backup started for VM '${vmName}'`)
      onSuccess?.()
    } catch (error) {
      toastFailure(toast, `Failed to backup VM '${vmName}'`, error)
    }
  }, [vmName, onSuccess, toast, canWrite])

  return {
    handleStart: useCallback(() => performAction(startVM, 'started'), [performAction]),
    handleStop: useCallback(() => performAction(stopVM, 'stopped'), [performAction]),
    handleRestart: useCallback(() => performAction(restartVM, 'restarted'), [performAction]),
    handlePause: useCallback(() => performAction(pauseVM, 'paused'), [performAction]),
    handleResume: useCallback(() => performAction(resumeVM, 'resumed'), [performAction]),
    handleDelete: useCallback(() => performAction(deleteVM, 'deleted'), [performAction]),
    handleBackup,
  }
}
