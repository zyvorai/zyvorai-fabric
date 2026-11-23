// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useEffect, useState } from 'react'
import { PageHeader } from '../components/ui'
import { PageLoading } from '../components/Skeleton'
import { DrainPlanView } from '../components/DrainPlanView'
import { listHosts, type HostInfo } from '../api/datacenter'
import { planHostDrain, type DrainPlan as Plan } from '../api/drainPlan'
import { formatUserError } from '../utils/apiError'

export default function DrainPlan() {
  const [hosts, setHosts] = useState<HostInfo[]>([])
  const [hostId, setHostId] = useState('')
  const [plan, setPlan] = useState<Plan | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    listHosts()
      .then((h) => {
        setHosts(h)
        if (h.length > 0) setHostId(h[0].id)
      })
      .catch((e) => setError(formatUserError(e)))
      .finally(() => setLoading(false))
  }, [])

  const run = async () => {
    setError(null)
    try {
      setPlan(await planHostDrain(hostId))
    } catch (e) {
      setPlan(null)
      setError(formatUserError(e))
    }
  }

  if (loading) return <PageLoading />
  return (
    <div>
      <PageHeader
        title="Host drain plan"
        description="Preview where a host's VMs would go before putting it into maintenance. Nothing is moved or stopped."
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      <div className="mb-4 flex items-center gap-3">
        <label htmlFor="drain-host">Host</label>
        <select id="drain-host" value={hostId} onChange={(e) => setHostId(e.target.value)}>
          {hosts.map((h) => (
            <option key={h.id} value={h.id}>{h.hostname}</option>
          ))}
        </select>
        <button type="button" className="btn-primary" disabled={!hostId} onClick={() => void run()}>
          Plan drain
        </button>
      </div>
      {plan && <DrainPlanView plan={plan} />}
    </div>
  )
}
