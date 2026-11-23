// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useEffect, useState } from 'react'
import { PageHeader } from '../components/ui'
import { PageLoading } from '../components/Skeleton'
import { CpuCompatView, CpuInventoryView } from '../components/CpuCompatView'
import { getCpuCompat, getCpuInventory, type CpuCompat as Compat, type CpuInventory } from '../api/cpuCompat'
import { formatUserError } from '../utils/apiError'

export default function CpuCompat() {
  const [inv, setInv] = useState<CpuInventory | null>(null)
  const [vm, setVm] = useState('')
  const [compat, setCompat] = useState<Compat | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    getCpuInventory().then(setInv).catch((e) => setError(formatUserError(e)))
  }, [])

  const check = async () => {
    setError(null)
    try {
      setCompat(await getCpuCompat(vm))
    } catch (e) {
      setCompat(null)
      setError(formatUserError(e))
    }
  }

  if (!inv && !error) return <PageLoading />
  const vms = (inv?.vms ?? []).filter((v) => v.configured)
  return (
    <div>
      <PageHeader
        title="CPU compatibility"
        description="Host CPU flags, VM CPU models and whether a VM can run on each host. Hosts report nothing by themselves: record them with fabricctl cpu-report."
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      {inv && <CpuInventoryView inv={inv} />}
      <div className="my-4 flex items-center gap-3">
        <label htmlFor="cpu-vm">VM</label>
        <select id="cpu-vm" value={vm} onChange={(e) => setVm(e.target.value)}>
          <option value="">Select a VM</option>
          {vms.map((v) => (
            <option key={v.name} value={v.name}>{v.name}</option>
          ))}
        </select>
        <button type="button" className="btn-primary" disabled={!vm} onClick={() => void check()}>
          Check hosts
        </button>
      </div>
      {compat && <CpuCompatView compat={compat} />}
    </div>
  )
}
