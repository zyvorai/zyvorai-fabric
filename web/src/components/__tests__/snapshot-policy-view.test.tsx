// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { SnapshotPolicyView, type SnapshotPolicyViewProps } from '../SnapshotPolicyView'
import { SnapshotTagChips } from '../SnapshotTagChips'
import { DEFAULT_FORM } from '../../lib/snapshotPolicy'
import type { SnapshotPolicy } from '../../api/snapshotPolicy'

const policy: SnapshotPolicy = {
  vm_name: 'vm',
  updated_at: '2026-03-01T00:00:00Z',
  enabled: true,
  interval_secs: 3600,
  keep_last: 7,
  name_prefix: 'auto',
  snapshot_type: 'Disk',
  last_run: null,
  next_run: '2099-01-01T00:00:00Z',
}

const base: SnapshotPolicyViewProps = {
  policy: null,
  form: DEFAULT_FORM,
  onFormChange: () => {},
  onSave: () => {},
  onPreview: () => {},
  onDelete: () => {},
  preview: null,
  busy: false,
  error: null,
  readOnly: false,
}

describe('SnapshotPolicyView', () => {
  it('shows an empty state without a policy and disables preview', () => {
    const html = renderToStaticMarkup(<SnapshotPolicyView {...base} />)
    expect(html).toContain('No policy set')
    expect(html).toContain('Save policy')
    expect(html).not.toContain('Remove policy')
    expect(html).toMatch(/<button[^>]*disabled[^>]*>Preview run/)
  })

  it('shows next and last run for a stored policy', () => {
    const html = renderToStaticMarkup(<SnapshotPolicyView {...base} policy={policy} />)
    expect(html).toContain('Next run: 2099-01-01 00:00 UTC')
    expect(html).toContain('Last run: never')
    expect(html).toContain('Remove policy')
  })

  it('reports a disabled policy as not scheduled', () => {
    const html = renderToStaticMarkup(<SnapshotPolicyView {...base} policy={{ ...policy, enabled: false, next_run: null }} />)
    expect(html).toContain('Not scheduled (policy disabled)')
  })

  it('lists validation errors and disables save', () => {
    const html = renderToStaticMarkup(<SnapshotPolicyView {...base} form={{ ...DEFAULT_FORM, keepLast: '', maxAgeHours: '' }} />)
    expect(html).toContain('at least one retention rule')
    expect(html).toMatch(/<button[^>]*disabled[^>]*>Save policy/)
  })

  it('renders the dry-run preview with what would be deleted', () => {
    const html = renderToStaticMarkup(
      <SnapshotPolicyView
        {...base}
        policy={policy}
        preview={{
          vm_name: 'vm',
          dry_run: true,
          created: 'auto-20260301T010000000Z',
          decisions: [
            { id: '1', name: 'auto-old', created: '2026-01-01T00:00:00Z', action: 'delete', reason: 'beyond retention' },
            { id: '2', name: 'auto-new', created: '2026-03-01T00:00:00Z', action: 'keep', reason: 'newest snapshot is never pruned' },
          ],
          deleted: [],
          delete_failed: [],
          errors: [],
        }}
      />,
    )
    expect(html).toContain('nothing was changed')
    expect(html).toContain('auto-20260301T010000000Z')
    expect(html).toContain('delete: auto-old (beyond retention)')
    expect(html).toContain('Would delete: 1')
  })

  it('is read-only for viewers: no action buttons, inputs disabled', () => {
    const html = renderToStaticMarkup(<SnapshotPolicyView {...base} policy={policy} readOnly />)
    expect(html).not.toContain('Save policy')
    expect(html).not.toContain('Preview run')
    expect(html).toContain('disabled')
  })
})

describe('SnapshotTagChips', () => {
  it('shows the keep chip and an Unkeep toggle when kept', () => {
    const html = renderToStaticMarkup(<SnapshotTagChips tags={['keep', 'golden']} onToggleKeep={() => {}} />)
    expect(html).toContain('golden')
    expect(html).toContain('Unkeep')
    expect(html).toContain('aria-pressed="true"')
  })
  it('offers Keep when not kept and no toggle when read-only', () => {
    expect(renderToStaticMarkup(<SnapshotTagChips tags={[]} onToggleKeep={() => {}} />)).toContain('>Keep<')
    const ro = renderToStaticMarkup(<SnapshotTagChips tags={['keep']} />)
    expect(ro).not.toContain('<button')
    expect(ro).toContain('keep')
  })
})
