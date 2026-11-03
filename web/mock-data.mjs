// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// Seed data for the dev mock (`npm run dev:mock`, via mock-api.ts) and for the screenshot/a11y harness
// (page-harness.mjs). One place, so a page looks the same in both. Every list used to answer `[]`, which made
// every baseline image an empty state and hid how tables, cards and charts actually look.
//
// Invented data only: names, addresses and counts are made up and mean nothing.

export const MOCK_USER = { id: 'mock-admin', username: 'admin', role: 'admin', tenant: null }

const now = Date.now()
const ago = (s) => new Date(now - s * 1000).toISOString()

const VMS = [
  { name: 'web-01', state: 'running', cpus: 4, memory: 8192, image: 'ubuntu-24.04', ip: '10.20.0.11', tags: ['prod', 'web'] },
  { name: 'web-02', state: 'running', cpus: 4, memory: 8192, image: 'ubuntu-24.04', ip: '10.20.0.12', tags: ['prod', 'web'] },
  { name: 'db-primary', state: 'running', cpus: 8, memory: 32768, image: 'debian-12', ip: '10.20.0.21', tags: ['prod', 'db'] },
  { name: 'ci-runner', state: 'paused', cpus: 2, memory: 4096, image: 'ubuntu-24.04', ip: '10.20.0.31', tags: ['ci'] },
  { name: 'staging-api', state: 'stopped', cpus: 2, memory: 4096, image: 'alpine-3.20', tags: ['staging'] },
  { name: 'batch-7', state: 'failed', cpus: 4, memory: 8192, image: 'debian-12', last_error: 'disk image not found' },
]

const SESSIONS = [
  { id: '16c0eb3f-2b18-41c8-8f2a-1309a8bf1233', agent: 'invoice-check', status: 'running', user_id: 'asha' },
  { id: '2b1f9d60-7c3a-4e55-9a10-5d2a9b7c1e01', agent: 'pdf-brief', status: 'completed', user_id: 'asha' },
  { id: '3c9a7e12-0d44-4b8e-8f3a-6a1c2d9e7f02', agent: 'csv-clean', status: 'completed', user_id: 'ravi' },
  { id: '4d8b6f23-1e55-4c9f-9a4b-7b2d3eaf8003', agent: 'contract-clauses', status: 'failed', user_id: 'ravi', error: 'model budget reached for this run' },
  { id: '5e7c5a34-2f66-4daa-8b5c-8c3e4fb09004', agent: 'log-triage', status: 'cancelled', user_id: 'asha' },
].map((s, i) => ({
  agent_version: '15ab785375eb',
  sandbox_id: `cb8190a9-86c2-4f43-88b3-07dec431bf3${i}`,
  created_at: ago(3600 * (i + 1)),
  updated_at: ago(60 * (i + 1)),
  last_event_seq: 40 + i,
  ...s,
}))

const PLANNED_ACTION = {
  changes: {
    added: ['/work/out/invoice-summary.md', '/work/out/flagged.csv'],
    modified: ['/work/state/seen.json'],
    deleted: ['/work/tmp/scratch.txt'],
    unchanged: 14,
  },
  changeset_id: '789ee210-f345-4b6e-986f-09c4223676ac',
  command: 'python3 check.py --in /work/in --out /work/out',
  exit_code: 0,
  expires_at: Math.floor(now / 1000) + 3 * 3600,
  paths: ['/work'],
  side_effects: {
    destinations: ['169.254.0.1/32 tcp/18082', 'api.example.com'],
    egress: 'allow_listed',
    non_replayable: ['possible traffic to allow-listed destination api.example.com: recorded, never replayed'],
    replayable: ['4 file change(s) under /work'],
  },
  unstaged: [],
}

const COCKPIT = (s) => ({
  session_id: s.id,
  agent: s.agent,
  status: s.status,
  tainted_by: [],
  taint_visible: false,
  egress_connects: 0,
  drop_reasons: [],
  pending_approvals:
    s.status === 'running'
      ? [
          {
            id: 'a1b2c3d4-0000-4000-8000-000000000001',
            prompt: 'Apply the file changes from `python3 check.py --in /work/in --out /work/out`?',
            status: 'pending',
            kind: 'changeset',
            subject: PLANNED_ACTION.changeset_id,
            planned_action: PLANNED_ACTION,
          },
        ]
      : [],
  failed_applies:
    s.status === 'running'
      ? [
          {
            id: 'a1b2c3d4-0000-4000-8000-000000000002',
            prompt: 'Apply the file changes from `echo two > /tmp/rt/b.txt`?',
            apply_error:
              'FluxVM returned 409 Conflict: {"error":"changeset is rejected; it cannot become approved","code":"CONFLICT"}',
            decided_at: ago(900),
          },
        ]
      : [],
  budget: {
    agent: s.agent,
    reached: false,
    warn: true,
    warn_at: 0.8,
    scopes: [
      { scope: 'day', calls: { used: 41, max: 50 }, cost: { used: 0.72, max: 1.0, currency: 'USD' }, fraction: 0.82, reached: false, warn: true },
      { scope: 'run', calls: { used: 6, max: 40 }, cost: { used: 0.09, max: null, currency: 'USD' }, fraction: 0.15, reached: false, warn: false },
      { scope: 'user', calls: { used: 120, max: 300 }, cost: { used: 1.9, max: null, currency: 'USD' }, fraction: 0.4, reached: false, warn: false },
    ],
  },
  last_decisions: [
    { action: 'approval.changeset', phase: 'Planned', at: ago(1800) },
    { action: 'approval.changeset', phase: 'Approved', at: ago(1700) },
    { action: 'changeset.apply', phase: 'Failed', at: ago(1690) },
  ],
  upcoming_cron: [],
  active_goal: null,
  recent_artifacts: [],
  evidence_class: 'software-test',
  honesty: 'The host can still see this VM. Keep 0.2 + attested hardware is required before claiming otherwise.',
  security_profile: 'measured',
})

const REPORT = (s) => ({
  session_id: s.id,
  agent: s.agent,
  status: s.status,
  generated_at: ago(30),
  did: [
    { at: ago(3300), type: 'session.created', summary: 'Session started for asha on invoice-check', source: 'journal' },
    { at: ago(3240), type: 'model.call', summary: 'Asked the model to classify 12 invoices (1,840 tokens)', source: 'journal' },
    { at: ago(3000), type: 'approval.changeset', summary: 'Proposed 4 file changes under /work for review', source: 'journal' },
    { at: ago(1700), type: 'approval.approved', summary: 'asha approved the changeset', source: 'journal' },
    { at: ago(1690), type: 'changeset.apply', summary: 'FluxVM could not apply it: the changeset was already rejected', source: 'journal' },
  ],
  checked: [
    { check: 'Egress: no connection left the cell (0 CONNECT)', result: 'passed', source: 'sentinel' },
    { check: 'Changeset applied to the cell', result: 'failed', source: 'journal' },
  ],
  not_checked: ['The content of the files in the changeset is masked on the host and was not inspected'],
  remaining: [{ what: 'Retry or re-run the failed apply' }],
  rollback: [{ item: '/work/out/invoice-summary.md', state: 'not applied', note: 'nothing in the cell changed' }],
})

const AGENTS = ['invoice-check', 'pdf-brief', 'csv-clean', 'contract-clauses', 'log-triage'].map((name) => ({
  name,
  version: '15ab785375eb',
  digest_sha256: '0'.repeat(64),
  created_at: ago(86400 * 3),
  manifest: { template: 'node22-agent', confinement: 'strict', egress_allow_hosts: [], credentials: [] },
}))

/** The JSON body for an `/api/...` path (without the `/api` prefix and query), or undefined for the default. */
export function mockBody(path) {
  const p = path.replace(/\/+$/, '')
  if (p === '/auth/me') return MOCK_USER
  if (p === '/vms') return VMS
  let m = p.match(/^\/vms\/([^/]+)\/metrics$/)
  if (m) {
    const vm = VMS.find((v) => v.name === m[1])
    const seed = (vm?.name.length ?? 5) * 7
    return {
      cpu_usage: 18 + (seed % 55),
      memory_usage: Math.round((vm?.memory ?? 4096) * 1024 * 1024 * (0.3 + (seed % 40) / 100)),
      disk_usage: 40 + (seed % 30),
      network_rx: 120000 + seed * 900,
      network_tx: 80000 + seed * 700,
    }
  }
  if (p === '/system/alerts') return [{ severity: 'warning', message: 'db-primary disk 82% full' }]
  if (p === '/sessions') return { items: SESSIONS }
  if (p === '/agents') return { items: AGENTS }
  m = p.match(/^\/sessions\/([^/]+)$/)
  if (m) return SESSIONS.find((s) => s.id === m[1]) ?? SESSIONS[0]
  m = p.match(/^\/users\/([^/]+)\/changeset-policies\/([^/]+)$/)
  if (m) {
    return {
      user_id: m[1],
      agent: m[2],
      updated_at: ago(86400),
      policy: { id: m[2], version: 2, paths: ['/work/out'], kinds: ['added'], max_files: 20, max_bytes: 1048576, deny_globs: ['*.env'] },
    }
  }
  m = p.match(/^\/sessions\/([^/]+)\/report$/)
  if (m) return REPORT(SESSIONS.find((s) => s.id === m[1]) ?? SESSIONS[0])
  m = p.match(/^\/sessions\/([^/]+)\/cockpit$/)
  if (m) return COCKPIT(SESSIONS.find((s) => s.id === m[1]) ?? SESSIONS[0])
  return undefined
}

export const SEEDED_SESSION_ID = SESSIONS[0].id
