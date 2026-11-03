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

const RECORDING = (s) => ({
  bundle: {
    format: 'zyvor-session-recording/1',
    session: { id: s.id, agent: s.agent, status: s.status },
    masking: 'message text, request bodies and approval wording are not included',
    steps: [
      { n: 0, source: 'journal', at: ago(3300), action: 'session.created', phase: 'Performed' },
      { n: 1, source: 'event', at: ago(3290), kind: 'stdout', data_bytes: 412 },
      { n: 2, source: 'journal', at: ago(3240), action: 'model.call', phase: 'Performed', detail: { host: 'api.example.com' } },
      { n: 3, source: 'journal', at: ago(3200), action: 'egress.drop', phase: 'Denied', detail: { host: 'tracker.example.net' } },
      { n: 4, source: 'approval', at: ago(3000), kind: 'changeset', status: 'pending' },
      { n: 5, source: 'approval', at: ago(1700), kind: 'changeset', status: 'approved', decided_at: ago(1700) },
      { n: 6, source: 'journal', at: ago(1690), action: 'changeset.apply', phase: 'Failed' },
    ],
  },
  digest: 'a'.repeat(64),
  signature: 'b'.repeat(128),
  signer: 'c'.repeat(64),
})

const ARTIFACTS = [
  { id: 'ar-0001', kind: 'brief', title: 'invoices-march.md', created_at: ago(7200), session_id: SESSIONS[1].id, agent: 'pdf-brief', metadata: { demo: 'pdf-brief' } },
  { id: 'ar-0002', kind: 'csv', title: 'ledger-clean.csv', created_at: ago(5400), session_id: SESSIONS[2].id, agent: 'csv-clean', metadata: { demo: 'csv-clean' } },
  { id: 'ar-0003', kind: 'brief', title: 'invoices-april.md', created_at: ago(1800), session_id: SESSIONS[1].id, agent: 'pdf-brief', metadata: { demo: 'pdf-brief' } },
]

const APPROVALS = [
  { id: 'a1b2c3d4-0000-4000-8000-000000000001', session_id: SESSIONS[0].id, kind: 'changeset', subject: 'python3 check.py --in /work/in', prompt: 'Apply the file changes?', status: 'pending', created_at: ago(3000) },
  { id: 'a1b2c3d4-0000-4000-8000-000000000002', session_id: SESSIONS[0].id, kind: 'changeset', subject: 'echo two > /tmp/rt/b.txt', prompt: 'Apply the file changes?', status: 'approved', created_at: ago(1900), decided_at: ago(1700) },
  { id: 'a1b2c3d4-0000-4000-8000-000000000003', session_id: SESSIONS[3].id, kind: 'egress', subject: 'api.example.com', prompt: 'Allow api.example.com?', status: 'denied', created_at: ago(9000), decided_at: ago(8900) },
]

const AUDIT = {
  items: [
    { seq: 41, at: ago(3300), session_id: SESSIONS[0].id, phase: 'Performed', action: 'session.created' },
    { seq: 42, at: ago(3240), session_id: SESSIONS[0].id, phase: 'Performed', action: 'model.call' },
    { seq: 43, at: ago(3200), session_id: SESSIONS[0].id, phase: 'Denied', action: 'egress.drop' },
    { seq: 44, at: ago(1700), session_id: SESSIONS[0].id, phase: 'Approved', action: 'approval.changeset' },
    { seq: 45, at: ago(1690), session_id: SESSIONS[0].id, phase: 'Failed', action: 'changeset.apply' },
  ],
  chain: { entries: 45, chain_ok: true, broken_at: null },
}

const DEMOS = [
  { id: 'pdf-brief', title: 'PDF brief', description: 'Drop a PDF, get a one-page brief.md.', accepts: ['pdf'], max_bytes: 33554432, builtin: true, has_sample: true },
  { id: 'csv-clean', title: 'CSV cleanup', description: 'Drop a messy CSV, get a clean one and a report.', accepts: ['csv'], max_bytes: 8388608, builtin: true, has_sample: true },
  { id: 'contract-clauses', title: 'Contract clauses', description: 'Pull the key clauses out of a contract.', accepts: ['pdf', 'docx'], max_bytes: 33554432, builtin: true, has_sample: false, model: { host: 'api.example.com', model: 'fable-5-1' } },
  { id: 'invoice-check', title: 'Invoice check', description: 'Check invoices against your rules.', accepts: ['pdf', 'csv'], max_bytes: 8388608, builtin: false, has_sample: false },
]

const BACKUPS = [
  { id: 'bk-0001', vm_name: 'db-primary', backup_type: 'full', size_bytes: 21474836480, compressed: true, created: ago(86400), status: 'completed', storage_location: '/backups/db-primary', retention_days: 30 },
  { id: 'bk-0002', vm_name: 'db-primary', backup_type: 'incremental', size_bytes: 1610612736, compressed: true, created: ago(43200), status: 'completed', storage_location: '/backups/db-primary', retention_days: 30 },
  { id: 'bk-0003', vm_name: 'web-01', backup_type: 'full', size_bytes: 8589934592, compressed: false, created: ago(7200), status: 'in_progress', storage_location: '/backups/web-01', retention_days: 14 },
  { id: 'bk-0004', vm_name: 'batch-7', backup_type: 'full', size_bytes: 0, compressed: false, created: ago(3600), status: 'failed', storage_location: '/backups/batch-7', retention_days: 7 },
]
const BACKUP_JOBS = [
  { id: 'job-1', backup_id: 'bk-0003', vm_name: 'web-01', operation: 'backup', status: 'running', progress: 62, started_at: ago(900) },
  { id: 'job-2', backup_id: 'bk-0004', vm_name: 'batch-7', operation: 'backup', status: 'failed', progress: 12, started_at: ago(3600), completed_at: ago(3500), error: 'disk image not found' },
]
const BACKUP_STATS = { total_backups: 4, total_size_bytes: 31675383808, by_type: { full: 3, incremental: 1 }, by_vm: { 'db-primary': 2, 'web-01': 1, 'batch-7': 1 }, oldest_backup: ago(86400), newest_backup: ago(3600) }

const SYSTEM_PERF = Array.from({ length: 24 }, (_, i) => ({
  timestamp: ago((23 - i) * 3600),
  total_vms: 6,
  running_vms: 3 + (i % 3 === 0 ? 1 : 0),
  total_cpu_usage: 30 + Math.round(25 * Math.sin(i / 3) + 20),
  total_memory_usage: 45 + Math.round(10 * Math.cos(i / 4) + 10),
  total_network_rx: 1200000 + i * 40000,
  total_network_tx: 800000 + i * 25000,
}))
const INSIGHTS = [
  { type: 'high_cpu', vm_name: 'db-primary', resource: 'cpu', value: 83, threshold: 80, severity: 'warning', recommendation: 'Add a vCPU or move the batch job off this VM.' },
  { type: 'underutilized', vm_name: 'ci-runner', resource: 'cpu', value: 4, threshold: 10, severity: 'info', recommendation: 'Shrink to 1 vCPU, or stop it outside working hours.' },
]
const TOP = [
  { vm_name: 'db-primary', value: 83 },
  { vm_name: 'api-01', value: 61 },
  { vm_name: 'web-01', value: 38 },
]

const AUDIT_LOGS = [
  { id: 'au-1', timestamp: ago(600), user: 'admin', action: 'vm.start', resource_type: 'vm', resource_name: 'web-01', status: 'success', ip_address: '10.20.0.5' },
  { id: 'au-2', timestamp: ago(1800), user: 'asha', action: 'backup.create', resource_type: 'backup', resource_name: 'db-primary', status: 'success', ip_address: '10.20.0.9' },
  { id: 'au-3', timestamp: ago(3600), user: 'ravi', action: 'vm.delete', resource_type: 'vm', resource_name: 'old-test', status: 'failed', ip_address: '10.20.0.14', error: 'permission denied' },
  { id: 'au-4', timestamp: ago(7200), user: 'admin', action: 'user.create', resource_type: 'user', resource_name: 'ravi', status: 'success', ip_address: '10.20.0.5' },
]
const AUDIT_STATS = { total_logs: 4, by_action: { 'vm.start': 1, 'backup.create': 1, 'vm.delete': 1, 'user.create': 1 }, by_user: { admin: 2, asha: 1, ravi: 1 }, by_status: { success: 3, failed: 1 }, recent_failures: 1 }

const CPU_TOPOLOGY = {
  total_cpus: 8, sockets: 1, cores_per_socket: 4, threads_per_core: 2,
  cpus: Array.from({ length: 8 }, (_, i) => ({ id: i, socket_id: 0, core_id: i % 4, thread_id: Math.floor(i / 4), online: true, numa_node: 0 })),
  online_cpus: [0, 1, 2, 3, 4, 5, 6, 7], offline_cpus: [],
}
const NUMA = { nodes: [{ id: 0, cpus: [0, 1, 2, 3, 4, 5, 6, 7], memory_total_mb: 65536, memory_free_mb: 24576, hugepages_2mb_total: 1024, hugepages_2mb_free: 512, hugepages_1gb_total: 4, hugepages_1gb_free: 2 }], distances: [[10]] }
const SYS_MEMORY = { total_kb: 67108864, free_kb: 25165824, available_kb: 41943040, buffers_kb: 1048576, cached_kb: 12582912 }
const HUGEPAGES = { size: 'Size2MB', total: 1024, free: 512, reserved: 0, surplus: 0 }
const RECOMMENDATIONS = [
  { vm_name: 'db-primary', recommendations: [{ resource: 'memory', current_value: '32 GB', recommended_value: '24 GB', reason: 'Peak use over 30 days was 19 GB.', impact: 'Frees 8 GB for other VMs.' }] },
]

const MIGRATIONS = [
  { id: 'mg-1', vm_name: 'web-02', target_host: 'node-b', migration_type: 'live', state: 'syncing', progress_percent: 64, bytes_transferred: 5368709120, started: ago(420) },
  { id: 'mg-2', vm_name: 'ci-runner', target_host: 'node-c', migration_type: 'offline', state: 'completed', progress_percent: 100, bytes_transferred: 4294967296, started: ago(86400), completed: ago(85000) },
  { id: 'mg-3', vm_name: 'batch-7', target_host: 'node-b', migration_type: 'storage', state: 'failed', progress_percent: 18, bytes_transferred: 805306368, started: ago(7200), completed: ago(7000), error: 'target pool full' },
]
const RUNTIME_CAPS = {
  apiVersion: 'v1', scope: 'host', orchestrationOwner: 'fabric',
  migration: [{ backend: 'qemu', live: true, preCopy: true, postCopy: false, multifd: true, requiresSharedStorage: false, transports: ['tcp', 'unix'] }],
  snapshot: [{ backend: 'qemu', memory: true, disk: true, portable: false }],
}

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
  if (p === '/backups') return BACKUPS
  if (p === '/backups/jobs') return BACKUP_JOBS
  if (p === '/backups/stats') return BACKUP_STATS
  if (p === '/analytics/system') return SYSTEM_PERF
  if (p === '/analytics/insights') return INSIGHTS
  if (p === '/analytics/utilization') return { cpu_utilization: 47, memory_utilization: 58, disk_utilization: 71, network_utilization: 22 }
  if (p === '/analytics/top') return TOP
  if (p === '/audit/logs') return AUDIT_LOGS
  if (p === '/audit/stats') return AUDIT_STATS
  if (p === '/system/cpu/topology') return CPU_TOPOLOGY
  if (p === '/system/numa/topology') return NUMA
  if (p === '/system/memory') return SYS_MEMORY
  if (p === '/system/memory/hugepages') return HUGEPAGES
  if (p === '/system/optimization/recommendations') return RECOMMENDATIONS
  if (p === '/migrations') return MIGRATIONS
  if (p === '/runtime/capabilities') return RUNTIME_CAPS
  if (p === '/sessions') return { items: SESSIONS }
  if (p === '/artifacts') return { items: ARTIFACTS }
  if (p === '/approvals') return { items: APPROVALS }
  if (p === '/audit/agent-actions') return AUDIT
  if (p === '/triggers') return { items: [{ id: 'tr-1', use_case: 'csv-clean', kind: 'folder', enabled: true, runs: 12, dir: '/inbox/csv', interval_seconds: 60, last_run_at: ago(120) }], watch_root_configured: true }
  if (p === '/demos') return { demos: DEMOS }
  if (p === '/merion/status') return { merion_mode: true, signature_required: true, trusted_signers: 1, fluxvm: { ready: true }, demo_template: 'node22-agent', demos: { builtin: 3, custom: 1 } }
  if (p === '/recordings/verify') return { valid: true, signer: 'c'.repeat(64) }
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
  m = p.match(/^\/sessions\/([^/]+)\/recording$/)
  if (m) return RECORDING(SESSIONS.find((s) => s.id === m[1]) ?? SESSIONS[0])
  m = p.match(/^\/sessions\/([^/]+)\/report$/)
  if (m) return REPORT(SESSIONS.find((s) => s.id === m[1]) ?? SESSIONS[0])
  m = p.match(/^\/sessions\/([^/]+)\/cockpit$/)
  if (m) return COCKPIT(SESSIONS.find((s) => s.id === m[1]) ?? SESSIONS[0])
  return undefined
}

export const SEEDED_SESSION_ID = SESSIONS[0].id
