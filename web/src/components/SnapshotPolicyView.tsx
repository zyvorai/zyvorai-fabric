// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import type { SnapshotPolicy, SnapshotPolicyRunReport } from '../api/snapshotPolicy'
import { describeNextRun, validateForm, type PolicyForm } from '../lib/snapshotPolicy'

export interface SnapshotPolicyViewProps {
  /** The stored policy, or null when none exists. */
  policy: SnapshotPolicy | null
  form: PolicyForm
  onFormChange: (next: PolicyForm) => void
  onSave: () => void
  onPreview: () => void
  onDelete: () => void
  preview: SnapshotPolicyRunReport | null
  busy: boolean
  error: string | null
  /** Viewers see the policy but cannot change it. */
  readOnly: boolean
}

const input = 'w-full px-2 py-1 text-sm rounded border border-[var(--zf-hairline)] bg-white disabled:opacity-60'

/** Presentational snapshot-policy editor: pure of data fetching so it renders statically. */
export function SnapshotPolicyView(p: SnapshotPolicyViewProps) {
  const { form, onFormChange, readOnly } = p
  const set = <K extends keyof PolicyForm>(k: K, v: PolicyForm[K]) => onFormChange({ ...form, [k]: v })
  const errors = validateForm(form)
  const deletes = p.preview?.decisions.filter((d) => d.action === 'delete') ?? []

  return (
    <section className="zf-panel p-4 space-y-3" aria-label="Snapshot policy">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-[var(--zf-ink)]">Snapshot policy</h3>
          <p className="text-xs text-[var(--zf-muted)]">
            Takes snapshots on a schedule and prunes only those named <code>{form.namePrefix || 'auto'}-…</code>. The newest one
            and any tagged <code>keep</code> are never pruned.
          </p>
        </div>
        <div className="text-right text-xs text-[var(--zf-muted)]" data-testid="next-run">
          {p.policy ? (
            <>
              <div>Next run: {describeNextRun(p.policy.next_run, p.policy.enabled)}</div>
              <div>Last run: {p.policy.last_run ? new Date(p.policy.last_run).toLocaleString() : 'never'}</div>
              {p.policy.last_result && <div className="max-w-xs truncate" title={p.policy.last_result}>{p.policy.last_result}</div>}
            </>
          ) : (
            <div>No policy set</div>
          )}
        </div>
      </div>

      {p.error && <div role="alert" className="text-sm text-[var(--zf-danger-text)]">{p.error}</div>}

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
        <label className="flex items-center gap-2 sm:col-span-2">
          <input type="checkbox" checked={form.enabled} disabled={readOnly} onChange={(e) => set('enabled', e.target.checked)} />
          Enabled (the daemon scheduler must also be on: <code>[snapshots] enabled</code>)
        </label>
        <label>
          <span className="block text-xs text-[var(--zf-muted)] mb-1">Schedule</span>
          <select className={input} value={form.scheduleKind} disabled={readOnly} onChange={(e) => set('scheduleKind', e.target.value as PolicyForm['scheduleKind'])}>
            <option value="interval">Every N minutes</option>
            <option value="cron">Cron (UTC)</option>
          </select>
        </label>
        {form.scheduleKind === 'interval' ? (
          <label>
            <span className="block text-xs text-[var(--zf-muted)] mb-1">Interval (minutes)</span>
            <input className={input} inputMode="numeric" value={form.intervalMinutes} disabled={readOnly} onChange={(e) => set('intervalMinutes', e.target.value)} />
          </label>
        ) : (
          <label>
            <span className="block text-xs text-[var(--zf-muted)] mb-1">Cron (min hour dom mon dow)</span>
            <input className={input} value={form.cron} disabled={readOnly} onChange={(e) => set('cron', e.target.value)} />
          </label>
        )}
        <label>
          <span className="block text-xs text-[var(--zf-muted)] mb-1">Keep last N</span>
          <input className={input} inputMode="numeric" value={form.keepLast} disabled={readOnly} onChange={(e) => set('keepLast', e.target.value)} />
        </label>
        <label>
          <span className="block text-xs text-[var(--zf-muted)] mb-1">Max age (hours)</span>
          <input className={input} inputMode="numeric" value={form.maxAgeHours} disabled={readOnly} onChange={(e) => set('maxAgeHours', e.target.value)} />
        </label>
        <label>
          <span className="block text-xs text-[var(--zf-muted)] mb-1">Name prefix</span>
          <input className={input} value={form.namePrefix} disabled={readOnly} onChange={(e) => set('namePrefix', e.target.value)} />
        </label>
        <label>
          <span className="block text-xs text-[var(--zf-muted)] mb-1">Snapshot type</span>
          <select className={input} value={form.snapshotType} disabled={readOnly} onChange={(e) => set('snapshotType', e.target.value as PolicyForm['snapshotType'])}>
            <option value="Disk">Disk only</option>
            <option value="Full">Full (disk + memory)</option>
          </select>
        </label>
      </div>

      {errors.length > 0 && (
        <ul className="text-xs text-[var(--zf-danger-text)] list-disc pl-5" data-testid="policy-errors">
          {errors.map((e) => <li key={e}>{e}</li>)}
        </ul>
      )}

      {!readOnly && (
        <div className="flex flex-wrap gap-2">
          <button type="button" className="zf-btn zf-btn-primary zf-btn-sm" disabled={p.busy || errors.length > 0} onClick={p.onSave}>
            Save policy
          </button>
          <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" disabled={p.busy || !p.policy} onClick={p.onPreview} title="Dry run: shows what would be created and pruned, changes nothing">
            Preview run
          </button>
          {p.policy && (
            <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" disabled={p.busy} onClick={p.onDelete}>
              Remove policy
            </button>
          )}
        </div>
      )}

      {p.preview && (
        <div className="border border-[var(--zf-hairline)] rounded p-3 text-sm" data-testid="policy-preview">
          <div className="font-medium text-[var(--zf-ink)]">Dry run (nothing was changed)</div>
          <div>Would create: <code>{p.preview.created ?? '-'}</code></div>
          <div>Would delete: {deletes.length === 0 ? 'nothing' : deletes.length}</div>
          {p.preview.decisions.length > 0 && (
            <ul className="mt-1 space-y-0.5 text-xs">
              {p.preview.decisions.map((d) => (
                <li key={d.id} className={d.action === 'delete' ? 'text-red-700' : 'text-[var(--zf-muted)]'}>
                  {d.action}: {d.name} ({d.reason})
                </li>
              ))}
            </ul>
          )}
          {p.preview.errors.length > 0 && <div className="mt-1 text-xs text-[var(--zf-danger-text)]">{p.preview.errors.join('; ')}</div>}
        </div>
      )}
    </section>
  )
}
