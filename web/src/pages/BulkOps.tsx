// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useState } from 'react'
import { PageHeader } from '../components/ui'
import { BulkPlanView, BulkResultView } from '../components/BulkPlanView'
import {
  applyBulk,
  planBulk,
  type BulkAction,
  type BulkApplied,
  type BulkDryRun,
} from '../api/bulkOps'
import { formatUserError } from '../utils/apiError'

const ACTIONS: BulkAction[] = ['start', 'stop', 'snapshot', 'tag-add', 'tag-remove']

function split(s: string): string[] {
  return s.split(',').map((x) => x.trim()).filter(Boolean)
}

export default function BulkOps() {
  const [action, setAction] = useState<BulkAction>('stop')
  const [tags, setTags] = useState('')
  const [prefix, setPrefix] = useState('')
  const [tag, setTag] = useState('')
  const [plan, setPlan] = useState<BulkDryRun | null>(null)
  const [result, setResult] = useState<BulkApplied | null>(null)
  const [error, setError] = useState<string | null>(null)

  const request = () => ({
    selector: { tags: split(tags), name_prefix: prefix || undefined },
    action,
    tag: tag || undefined,
  })

  const preview = async () => {
    setError(null)
    setResult(null)
    try {
      setPlan(await planBulk(request()))
    } catch (e) {
      setPlan(null)
      setError(formatUserError(e))
    }
  }

  const apply = async () => {
    if (!plan) return
    setError(null)
    try {
      setResult(await applyBulk({ ...request(), plan_hash: plan.plan.plan_hash }))
      setPlan(null)
    } catch (e) {
      setError(formatUserError(e))
    }
  }

  return (
    <div>
      <PageHeader
        title="Bulk VM operations"
        description="Select VMs by tag or name prefix, review the plan, then apply it with its plan hash. The default is a dry run."
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <label htmlFor="bulk-action">Action</label>
        <select id="bulk-action" value={action} onChange={(e) => setAction(e.target.value as BulkAction)}>
          {ACTIONS.map((a) => (
            <option key={a} value={a}>{a}</option>
          ))}
        </select>
        <label htmlFor="bulk-tags">Tags (comma separated)</label>
        <input id="bulk-tags" value={tags} onChange={(e) => setTags(e.target.value)} />
        <label htmlFor="bulk-prefix">Name prefix</label>
        <input id="bulk-prefix" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
        {(action === 'tag-add' || action === 'tag-remove') && (
          <>
            <label htmlFor="bulk-tag">Tag to {action === 'tag-add' ? 'add' : 'remove'}</label>
            <input id="bulk-tag" value={tag} onChange={(e) => setTag(e.target.value)} />
          </>
        )}
        <button type="button" className="btn-primary" disabled={!tags && !prefix} onClick={() => void preview()}>
          Preview plan
        </button>
      </div>
      {plan && (
        <>
          <BulkPlanView plan={plan.plan} />
          <button
            type="button"
            className="btn-primary mt-4"
            disabled={!plan.can_apply}
            onClick={() => void apply()}
          >
            Apply this plan
          </button>
        </>
      )}
      {result && <BulkResultView result={result} />}
    </div>
  )
}
