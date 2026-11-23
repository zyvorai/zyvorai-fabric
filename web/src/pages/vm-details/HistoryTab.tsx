// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import type { VM } from '../../api/vm'
import {
  diffVmRevisions,
  getSinceGood,
  getVmHistory,
  markRevisionGood,
  type RevisionSummary,
  type SinceGood,
  type VmDiff,
  type VmHistory,
} from '../../api/configHistory'
import { DiffView, RevisionTable, SinceGoodView } from '../../components/ConfigHistoryView'
import { PanelLoading } from '../../components/Skeleton'
import { useToastContext } from '../../contexts/ToastContext'
import { usePermissions } from '../../hooks/usePermissions'
import { formatUserError } from '../../utils/apiError'
import { toastFailure } from '../../utils/toastError'

export default function HistoryTab({ vm }: { vm: VM }) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [history, setHistory] = useState<VmHistory | null>(null)
  const [sinceGood, setSinceGood] = useState<SinceGood | null>(null)
  const [diff, setDiff] = useState<VmDiff | null>(null)
  const [selected, setSelected] = useState<number[]>([])
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    setError(null)
    try {
      const [h, s] = await Promise.all([getVmHistory(vm.name), getSinceGood(vm.name)])
      setHistory(h)
      setSinceGood(s)
    } catch (e) {
      setError(formatUserError(e))
    }
  }, [vm.name])

  useEffect(() => {
    void load()
  }, [load])

  const toggle = (rev: number) =>
    setSelected((cur) => (cur.includes(rev) ? cur.filter((r) => r !== rev) : [...cur, rev].slice(-2)))

  const compare = async () => {
    const [a, b] = [...selected].sort((x, y) => x - y)
    try {
      setDiff(await diffVmRevisions(vm.name, a, b))
    } catch (e) {
      toastFailure(toast, 'Could not compare revisions', e)
    }
  }

  const markGood = async (r: RevisionSummary) => {
    try {
      await markRevisionGood(vm.name, r.rev)
      toast.success(`Revision ${r.rev} marked good`)
      await load()
    } catch (e) {
      toastFailure(toast, 'Could not mark the revision good', e)
    }
  }

  if (error) return <p role="alert" className="text-red-700">{error}</p>
  if (!history || !sinceGood) return <PanelLoading />

  return (
    <div className="space-y-6">
      <p className="text-sm text-[var(--zf-muted)]">
        Every configuration change is recorded with who made it, when, and a link to the audit log entry. Secrets,
        cloud-init commands, SSH keys and kernel arguments are never stored, only hashed markers. This tab is read-only
        for the VM: nothing here changes or reverts it.
      </p>
      {history.chain.status === 'broken' && (
        <p role="alert" className="text-red-700">
          The revision chain does not verify (revision {history.chain.first_break?.rev}: {history.chain.first_break?.reason}).
        </p>
      )}
      <section aria-label="What changed since last known good">
        <h2 className="mb-2 text-lg font-semibold">What changed since last known good</h2>
        <SinceGoodView data={sinceGood} />
        <p className="mt-2 text-xs text-[var(--zf-muted)]">{history.good_signal}</p>
      </section>
      <section aria-label="Revision history">
        <h2 className="mb-2 text-lg font-semibold">Revisions</h2>
        <p className="mb-2 text-sm">
          {history.retention.kept} kept (limit {history.retention.max_revisions} per VM). Pick two revisions to compare.
        </p>
        <RevisionTable history={history} selected={selected} onToggle={toggle} onMarkGood={(r) => void markGood(r)} canWrite={canWrite} />
        <div className="mt-3">
          <button type="button" className="btn-primary" disabled={selected.length !== 2} onClick={() => void compare()}>
            Compare selected revisions
          </button>
        </div>
      </section>
      {diff && <DiffView diff={diff} />}
    </div>
  )
}
