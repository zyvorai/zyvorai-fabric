// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import {
  deleteSnapshotPolicy,
  getSnapshotPolicy,
  previewSnapshotPolicy,
  putSnapshotPolicy,
  type SnapshotPolicy,
  type SnapshotPolicyRunReport,
} from '../api/snapshotPolicy'
import { DEFAULT_FORM, formToInput, policyToForm, validateForm, type PolicyForm } from '../lib/snapshotPolicy'
import { formatUserError } from '../utils/apiError'
import { SnapshotPolicyView } from './SnapshotPolicyView'

/** Loads, edits and dry-runs one VM's snapshot policy. */
export default function SnapshotPolicyPanel({ vmName, readOnly }: { vmName: string; readOnly: boolean }) {
  const [policy, setPolicy] = useState<SnapshotPolicy | null>(null)
  const [form, setForm] = useState<PolicyForm>(DEFAULT_FORM)
  const [preview, setPreview] = useState<SnapshotPolicyRunReport | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      const p = await getSnapshotPolicy(vmName)
      setPolicy(p)
      setForm(policyToForm(p))
      setError(null)
    } catch (err) {
      setError(formatUserError(err))
    }
  }, [vmName])

  useEffect(() => {
    void load()
  }, [load])

  const run = async (fn: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await fn()
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <SnapshotPolicyView
      policy={policy}
      form={form}
      onFormChange={(f) => {
        setForm(f)
        setPreview(null)
      }}
      preview={preview}
      busy={busy}
      error={error}
      readOnly={readOnly}
      onSave={() =>
        void run(async () => {
          if (validateForm(form).length > 0) return
          const saved = await putSnapshotPolicy(vmName, formToInput(form))
          setPolicy(saved)
          setForm(policyToForm(saved))
          setPreview(null)
        })
      }
      onPreview={() => void run(async () => setPreview(await previewSnapshotPolicy(vmName)))}
      onDelete={() =>
        void run(async () => {
          await deleteSnapshotPolicy(vmName)
          setPolicy(null)
          setForm(policyToForm(null))
          setPreview(null)
        })
      }
    />
  )
}
