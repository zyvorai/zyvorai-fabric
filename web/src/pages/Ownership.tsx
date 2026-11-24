// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import { PageHeader } from '../components/ui'
import { PageLoading } from '../components/Skeleton'
import { OwnershipReportView } from '../components/HousekeepingViews'
import {
  clearOwnership,
  extendLease,
  getOwnership,
  getOwnershipReport,
  setOwnership,
  type OwnershipReport,
} from '../api/housekeeping'
import { formatUserError } from '../utils/apiError'

const EMPTY = { owner: '', contact: '', purpose: '', notes: '', expires: '' }

export default function Ownership() {
  const [report, setReport] = useState<OwnershipReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [vm, setVm] = useState<string | null>(null)
  const [form, setForm] = useState(EMPTY)

  const refresh = useCallback(async () => {
    try {
      setReport(await getOwnershipReport())
      setError(null)
    } catch (e) {
      setError(formatUserError(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const select = async (name: string) => {
    setVm(name)
    setError(null)
    try {
      const o = await getOwnership(name)
      setForm({
        owner: o.owner ?? '',
        contact: o.contact ?? '',
        purpose: o.purpose ?? '',
        notes: o.notes ?? '',
        expires: o.expires_at ? o.expires_at.slice(0, 10) : '',
      })
    } catch (e) {
      setError(formatUserError(e))
    }
  }

  const act = async (fn: () => Promise<unknown>) => {
    setError(null)
    try {
      await fn()
      await refresh()
    } catch (e) {
      setError(formatUserError(e))
    }
  }

  const save = () =>
    act(() =>
      setOwnership(vm as string, {
        owner: form.owner,
        contact: form.contact,
        purpose: form.purpose,
        notes: form.notes,
        // A date picker value is the end of that day in UTC.
        expires_at: form.expires ? `${form.expires}T23:59:59Z` : undefined,
      }),
    )

  if (loading) return <PageLoading />
  const field = (id: keyof typeof EMPTY, label: string, type = 'text') => (
    <div className="flex flex-col">
      <label htmlFor={`own-${id}`}>{label}</label>
      <input
        id={`own-${id}`}
        type={type}
        value={form[id]}
        onChange={(e) => setForm({ ...form, [id]: e.target.value })}
      />
    </div>
  )
  return (
    <div>
      <PageHeader
        title="Ownership and leases"
        description="Who owns each VM, why it exists and when its lease ends. Expired leases only show up in reports; nothing is stopped or deleted."
        onRefresh={() => void refresh()}
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      {report && <OwnershipReportView report={report} onSelect={(n) => void select(n)} />}
      {vm && (
        <form
          aria-label={`Ownership of ${vm}`}
          className="mt-6 grid max-w-xl gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            void save()
          }}
        >
          <h2 className="text-lg">{vm}</h2>
          {field('owner', 'Owner')}
          {field('contact', 'Contact')}
          {field('purpose', 'Purpose')}
          {field('notes', 'Notes')}
          {field('expires', 'Lease ends (UTC date)', 'date')}
          <div className="flex gap-3">
            <button type="submit" className="btn-primary">Save</button>
            <button type="button" className="btn-secondary" onClick={() => void act(() => extendLease(vm, 30))}>
              Extend 30 days
            </button>
            <button
              type="button"
              className="btn-secondary"
              onClick={() => void act(async () => { await clearOwnership(vm); setForm(EMPTY) })}
            >
              Clear
            </button>
          </div>
        </form>
      )}
    </div>
  )
}
