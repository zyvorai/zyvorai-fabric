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

const NETD_LINKS = [
  { index: 2, name: 'eth0', kind: 'ether', operational_state: 'carrier', setup_state: 'configured' },
  { index: 3, name: 'eth1', kind: 'ether', operational_state: 'carrier', setup_state: 'configured' },
  { index: 4, name: 'eth2', kind: 'ether', operational_state: 'no-carrier', setup_state: 'configured' },
]
const netd = (o) => ({ created: ago(86400 * 9), updated: ago(86400), managed: true, operational_state: 'routable', ...o })
const NETD_BONDS = [netd({ id: 'bond-1', name: 'bond0', mode: '802.3ad', slave_interfaces: ['eth0', 'eth1'], addresses: [], dns: [], dhcp: 'no', mtu: 9000 })]
const NETD_BRIDGES = [
  netd({ id: 'br-1', name: 'br0', stp: true, addresses: ['10.20.0.1/24'], dns: ['10.20.0.2'], dhcp: 'no', mtu: 1500 }),
  netd({ id: 'br-2', name: 'br-mgmt', stp: false, addresses: ['10.30.0.1/24'], dns: [], dhcp: 'no', mtu: 1500 }),
]
const NETD_VLANS = [
  netd({ id: 'vl-1', name: 'vlan100', vlan_id: 100, parent_interface: 'bond0', addresses: [], dns: [], dhcp: 'no' }),
  netd({ id: 'vl-2', name: 'vlan200', vlan_id: 200, parent_interface: 'bond0', addresses: [], dns: [], dhcp: 'no' }),
]
const NETD_MACVTAPS = [netd({ id: 'mv-1', name: 'mv0', parent_interface: 'eth2', mode: 'bridge', operational_state: 'no-carrier' })]
const NETD_TAPS = [
  netd({ id: 'tp-1', name: 'tap-web-01', bridge: 'br0', operational_state: 'carrier' }),
  netd({ id: 'tp-2', name: 'tap-web-02', bridge: 'br0', operational_state: 'carrier' }),
  netd({ id: 'tp-3', name: 'tap-db', bridge: 'br-mgmt', operational_state: 'degraded' }),
]
const NETD_VXLANS = [netd({ id: 'vx-1', name: 'vx100', vni: 100, parent_interface: 'bond0', remote: '10.40.0.9', addresses: [], dns: [], dhcp: 'no' })]
const NETD_NETFILES = [
  netd({ id: 'nf-1', match_name: 'vlan100', bridge: 'br0', addresses: [], dns: [], dhcp: 'no', routes: [] }),
  netd({ id: 'nf-2', match_name: 'vlan200', bridge: 'br-mgmt', addresses: [], dns: [], dhcp: 'no', routes: [] }),
]
const NETD_PORT_FORWARDS = [netd({ id: 'pf-1', name: 'web-https', protocol: 'tcp', host_port: 443, guest_ip: '10.20.0.11', guest_port: 443, enabled: true, description: 'HTTPS to web-01' })]

// --- Seeds for the pages that crash on empty defaults (all invented) ---
const D = 86400
const CERT_CAS = [
  { id: 'ca-root', name: 'Fabric Root CA', ca_type: 'root', subject: 'CN=Fabric Root CA,O=Example', issuer: 'CN=Fabric Root CA,O=Example', serial_number: '01A2B3C4', valid_from: ago(900 * D), valid_to: ago(-3000 * D), key_algorithm: 'RSA', key_size: 4096, status: 'active', issued_count: 2, created: ago(900 * D) },
  { id: 'ca-int', name: 'Fabric Issuing CA', ca_type: 'intermediate', subject: 'CN=Fabric Issuing CA,O=Example', issuer: 'CN=Fabric Root CA,O=Example', serial_number: '02B3C4D5', valid_from: ago(500 * D), valid_to: ago(-1300 * D), key_algorithm: 'ECDSA', key_size: 384, status: 'active', issued_count: 3, created: ago(500 * D) },
  { id: 'ca-old', name: 'Legacy CA', ca_type: 'external', subject: 'CN=Legacy CA,O=Example', issuer: 'CN=Legacy CA,O=Example', serial_number: '03C4D5E6', valid_from: ago(2000 * D), valid_to: ago(30 * D), key_algorithm: 'RSA', key_size: 2048, status: 'expired', issued_count: 0, created: ago(2000 * D) },
]
const CERTS = [
  { id: 'crt-1', subject: 'node-a.lab.example', issuer: 'Fabric Issuing CA', serial_number: '10AA01', ca_id: 'ca-int', cert_type: 'host', valid_from: ago(200 * D), valid_to: ago(-165 * D), key_algorithm: 'ECDSA', key_size: 384, san: ['node-a.lab.example', '10.0.0.11'], status: 'active', host_name: 'node-a', fingerprint: 'AB:CD:01:23', created: ago(200 * D) },
  { id: 'crt-2', subject: 'api.lab.example', issuer: 'Fabric Issuing CA', serial_number: '10AA02', ca_id: 'ca-int', cert_type: 'server', valid_from: ago(350 * D), valid_to: ago(-12 * D), key_algorithm: 'RSA', key_size: 2048, san: ['api.lab.example'], status: 'expiring_soon', service_name: 'fabricd', fingerprint: 'AB:CD:02:34', created: ago(350 * D) },
  { id: 'crt-3', subject: 'old-svc.lab.example', issuer: 'Legacy CA', serial_number: '10AA03', ca_id: 'ca-old', cert_type: 'service', valid_from: ago(800 * D), valid_to: ago(20 * D), key_algorithm: 'RSA', key_size: 2048, status: 'expired', service_name: 'old-svc', fingerprint: 'AB:CD:03:45', created: ago(800 * D) },
]
const CERT_REQUESTS = [
  { id: 'req-1', subject: 'node-b.lab.example', requestor: 'admin', ca_id: 'ca-int', cert_type: 'host', key_size: 2048, san: ['node-b.lab.example'], status: 'pending', submitted_at: ago(3600) },
  { id: 'req-2', subject: 'web-01.lab.example', requestor: 'operator', ca_id: 'ca-int', cert_type: 'server', key_size: 2048, status: 'approved', submitted_at: ago(2 * D), reviewed_at: ago(D), reviewed_by: 'admin' },
  { id: 'req-3', subject: 'test.lab.example', requestor: 'operator', ca_id: 'ca-int', cert_type: 'client', key_size: 1024, status: 'rejected', submitted_at: ago(5 * D), reviewed_at: ago(4 * D), reviewed_by: 'admin', rejection_reason: 'Key size too small' },
]
const CERT_ATTESTATIONS = [
  { id: 'att-1', host_id: 'host-a', hostname: 'node-a', tpm_present: true, tpm_version: '2.0', attestation_status: 'trusted', last_attestation: ago(600), boot_integrity: true, secure_boot_enabled: true, created: ago(30 * D) },
  { id: 'att-2', host_id: 'host-b', hostname: 'node-b', tpm_present: true, tpm_version: '2.0', attestation_status: 'untrusted', last_attestation: ago(900), boot_integrity: false, secure_boot_enabled: true, created: ago(30 * D) },
  { id: 'att-3', host_id: 'host-c', hostname: 'node-c', tpm_present: false, attestation_status: 'unknown', boot_integrity: false, secure_boot_enabled: false, created: ago(30 * D) },
]
const CERT_BASELINES = [
  { id: 'sb-1', name: 'CIS Level 1', description: 'Baseline hardening checks', checks: [
    { check_id: 'c1', name: 'Disk encryption enabled', category: 'storage', severity: 'high', enabled: true },
    { check_id: 'c2', name: 'Guest agent current', category: 'guest', severity: 'low', enabled: true },
  ], vm_count: 10, compliant_count: 8, non_compliant_count: 2, last_scan: ago(7200), created: ago(60 * D) },
  { id: 'sb-2', name: 'PCI Strict', checks: [
    { check_id: 'c3', name: 'Secure boot', category: 'boot', severity: 'critical', enabled: true },
  ], vm_count: 4, compliant_count: 4, non_compliant_count: 0, created: ago(20 * D) },
]
const CERT_HEALTH = {
  total_certificates: 3, active: 1, expiring_soon: 1, expired: 1, revoked: 0, pending_requests: 1, ca_count: 3,
  trusted_hosts: 1, untrusted_hosts: 1, security_baselines: 2, overall_compliance_pct: 86.5,
  expiring_within_30_days: [{ cert_id: 'crt-2', subject: 'api.lab.example', valid_to: ago(-12 * D), days_remaining: 12 }],
}
const COMPLIANCE = {
  score: 82, total: 4, passed: 2, warnings: 1, failed: 1, categories: ['Network', 'Storage', 'Access'],
  checks: [
    { id: 'n1', category: 'Network', name: 'TLS enforced', status: 'pass', description: 'API served over TLS only' },
    { id: 's1', category: 'Storage', name: 'Backups recent', status: 'warning', description: 'Last backup older than 24h', remediation: 'Run a backup' },
    { id: 'a1', category: 'Access', name: 'Default admin password changed', status: 'pass', description: 'No default credentials' },
    { id: 'a2', category: 'Access', name: 'MFA enabled', status: 'fail', description: 'MFA not enforced', remediation: 'Enable MFA in settings' },
  ],
  last_scan: ago(1800),
}
const DRS_CONFIG = { cluster_id: 'default', enabled: true, automation_level: 'partially_automated', migration_threshold: 3, balance_cpu_weight: 0.5, balance_memory_weight: 0.5, check_interval_seconds: 300, min_improvement_pct: 5, updated: ago(D) }
const DRS_BALANCE = {
  cluster_id: 'default', cpu_imbalance_pct: 18.4, memory_imbalance_pct: 12.1, overall_score: 78.5,
  hosts: [
    { host_id: 'host-a', hostname: 'node-a', cpu_usage_pct: 72, memory_usage_pct: 65, vm_count: 6, deviation_from_mean_cpu: 14.5, deviation_from_mean_memory: 9 },
    { host_id: 'host-b', hostname: 'node-b', cpu_usage_pct: 35, memory_usage_pct: 42, vm_count: 3, deviation_from_mean_cpu: -22.5, deviation_from_mean_memory: -14 },
  ],
  recommendation_count: 2, last_analyzed: ago(300),
}
const DRS_RECS = [
  { id: 'rec-1', cluster_id: 'default', vm_id: 'vm-1', vm_name: 'web-01', source_host_id: 'host-a', source_hostname: 'node-a', target_host_id: 'host-b', target_hostname: 'node-b', reason: 'Reduce CPU imbalance', priority: 'high', estimated_benefit_pct: 9.5, status: 'pending', created: ago(600) },
  { id: 'rec-2', cluster_id: 'default', vm_id: 'vm-2', vm_name: 'db-01', source_host_id: 'host-a', source_hostname: 'node-a', target_host_id: 'host-b', target_hostname: 'node-b', reason: 'Free memory on node-a', priority: 'medium', estimated_benefit_pct: 4.2, status: 'completed', created: ago(D), executed_at: ago(D - 60) },
]
const DRS_RULES = [
  { id: 'rule-1', cluster_id: 'default', name: 'Keep web tier apart', rule_type: 'anti_affinity', mandatory: true, vm_ids: ['vm-1', 'vm-3'], enabled: true, created: ago(10 * D) },
  { id: 'rule-2', cluster_id: 'default', name: 'App with cache', rule_type: 'affinity', mandatory: false, vm_ids: ['vm-2', 'vm-4'], enabled: false, created: ago(5 * D) },
]
const FED_OVERVIEW = {
  generated_at: ago(5),
  totals: { peers: 3, reachable: 1, hosts: 2, hosts_in_maintenance: 0, vms: 9 },
  peers: [
    { id: 'peer-1', name: 'site-east', url: 'https://10.1.0.5:8443', status: 'ok', latency_ms: 12, vm_total: 9,
      totals: { hosts: 2, hosts_in_maintenance: 0, cpus: 32, memory_mb: 131072 },
      hosts: [
        { id: 'ph-1', hostname: 'east-node-a', status: 'online', cpus: 16, memory_mb: 65536, vm_count: 5, cpu_usage_pct: 41, memory_usage_pct: 55 },
        { id: 'ph-2', hostname: 'east-node-b', status: 'online', cpus: 16, memory_mb: 65536, vm_count: 4, cpu_usage_pct: 28, memory_usage_pct: 37 },
      ] },
    { id: 'peer-2', name: 'site-west', url: 'https://10.2.0.5:8443', status: 'unreachable', latency_ms: 0, message: 'connection timed out', vm_total: null },
    { id: 'peer-3', name: 'site-lab', url: 'https://10.3.0.5:8443', status: 'unauthorized', latency_ms: 20, message: 'token rejected' },
  ],
}
const LC_BASELINES = [
  { id: 'bl-1', name: 'Security patches 2026-10', description: 'Monthly security patches', baseline_type: 'patch', release_date: ago(20 * D), severity: 'critical', patches: ['KB-1001', 'KB-1002'], host_count: 3, compliant_count: 2, created: ago(20 * D) },
  { id: 'bl-2', name: 'Agent upgrade 2.4', baseline_type: 'upgrade', release_date: ago(40 * D), severity: 'moderate', patches: ['agent-2.4'], host_count: 3, compliant_count: 3, created: ago(40 * D) },
]
const LC_COMPLIANCE = [
  { id: 'hc-1', host_id: 'host-a', hostname: 'node-a', baseline_id: 'bl-1', baseline_name: 'Security patches 2026-10', status: 'compliant', missing_patches: [], installed_patches: ['KB-1001', 'KB-1002'], last_scanned: ago(3600) },
  { id: 'hc-2', host_id: 'host-b', hostname: 'node-b', baseline_id: 'bl-1', baseline_name: 'Security patches 2026-10', status: 'non_compliant', missing_patches: ['KB-1002'], installed_patches: ['KB-1001'], last_scanned: ago(3600) },
  { id: 'hc-3', host_id: 'host-c', hostname: 'node-c', baseline_id: 'bl-2', baseline_name: 'Agent upgrade 2.4', status: 'unknown', missing_patches: [], installed_patches: [], last_scanned: ago(D) },
]
const LC_REMEDIATIONS = [
  { id: 'rem-1', host_id: 'host-b', hostname: 'node-b', baseline_id: 'bl-1', baseline_name: 'Security patches 2026-10', status: 'remediating', progress: 50, patches_applied: 1, patches_total: 2, started_at: ago(600) },
  { id: 'rem-2', host_id: 'host-c', hostname: 'node-c', baseline_id: 'bl-2', baseline_name: 'Agent upgrade 2.4', status: 'failed', progress: 20, patches_applied: 0, patches_total: 1, started_at: ago(D), completed_at: ago(D - 300), error: 'host unreachable' },
  { id: 'rem-3', host_id: 'host-a', hostname: 'node-a', baseline_id: 'bl-1', baseline_name: 'Security patches 2026-10', status: 'completed', progress: 100, patches_applied: 2, patches_total: 2, started_at: ago(2 * D), completed_at: ago(2 * D - 900) },
]
const LC_ROLLING = [
  { id: 'ru-1', name: 'October patch wave', baseline_id: 'bl-1', host_ids: ['host-a', 'host-b', 'host-c'], parallel_count: 1, failure_threshold: 1, pre_check_enabled: true, auto_remediate: true, status: 'running', completed_hosts: 1, failed_hosts: 0, total_hosts: 3, current_host: 'node-b', started_at: ago(1800) },
  { id: 'ru-2', name: 'Agent rollout', baseline_id: 'bl-2', host_ids: ['host-a', 'host-b'], parallel_count: 2, failure_threshold: 1, pre_check_enabled: false, auto_remediate: false, status: 'completed', completed_hosts: 2, failed_hosts: 0, total_hosts: 2, started_at: ago(5 * D), completed_at: ago(5 * D - 1200) },
]
const HOSTS = [
  { id: 'host-a', hostname: 'node-a', address: '10.0.0.11', cluster_id: 'default', datacenter_id: 'dc-1', cpus: 16, memory_mb: 65536, status: 'online', last_heartbeat: ago(5), vm_count: 6, cpu_usage_pct: 72, memory_usage_pct: 65, agent_version: '2.4.0', created: ago(100 * D) },
  { id: 'host-b', hostname: 'node-b', address: '10.0.0.12', cluster_id: 'default', datacenter_id: 'dc-1', cpus: 16, memory_mb: 65536, status: 'maintenance', last_heartbeat: ago(8), vm_count: 3, cpu_usage_pct: 35, memory_usage_pct: 42, agent_version: '2.3.1', created: ago(100 * D) },
  { id: 'host-c', hostname: 'node-c', address: '10.0.0.13', cluster_id: 'default', datacenter_id: 'dc-1', cpus: 8, memory_mb: 32768, status: 'offline', vm_count: 0, cpu_usage_pct: 0, memory_usage_pct: 0, created: ago(60 * D) },
]
const MIG_REPORT = {
  total: 3, successful: 1, failed: 1, running: 1, avg_duration: '4m12s', timestamp: ago(60),
  migrations: [
    { id: 'mig-1', name: 'web-01-to-node-b', vm_name: 'web-01', status: 'completed', duration: '3m40s', output_path: '/var/lib/fabric/migrations/web-01' },
    { id: 'mig-2', name: 'db-01-to-node-b', vm_name: 'db-01', status: 'failed', duration: '1m05s', error: 'target datastore full' },
    { id: 'mig-3', name: 'cache-01-to-node-a', vm_name: 'cache-01', status: 'running' },
  ],
}
const REPL_SITES = [
  { id: 'site-1', name: 'primary-dc', description: 'Main site', endpoint: 'https://10.0.0.1:8443', site_type: 'primary', status: 'healthy', replication_count: 2, last_sync: ago(120), created: ago(90 * D) },
  { id: 'site-2', name: 'dr-dc', endpoint: 'https://10.9.0.1:8443', site_type: 'secondary', status: 'degraded', replication_count: 2, last_sync: ago(3 * 3600), created: ago(90 * D) },
]
const REPL_CONFIGS = [
  { id: 'rp-1', vm_id: 'vm-1', vm_name: 'web-01', source_site_id: 'site-1', target_site_id: 'site-2', rpo_minutes: 15, quiesce: true, compression_enabled: true, encryption_enabled: true, bandwidth_limit_mbps: 200, status: 'active', last_sync: ago(300), next_sync: ago(-600), created: ago(30 * D) },
  { id: 'rp-2', vm_id: 'vm-2', vm_name: 'db-01', source_site_id: 'site-1', target_site_id: 'site-2', rpo_minutes: 5, quiesce: true, compression_enabled: false, encryption_enabled: true, status: 'error', last_sync: ago(3 * 3600), created: ago(30 * D) },
  { id: 'rp-3', vm_id: 'vm-3', vm_name: 'app-01', source_site_id: 'site-1', target_site_id: 'site-2', rpo_minutes: 30, quiesce: false, compression_enabled: true, encryption_enabled: false, status: 'initial_sync', sync_progress_pct: 42, created: ago(D) },
]
const REPL_VIOLATIONS = [
  { replication_id: 'rp-2', vm_id: 'vm-2', vm_name: 'db-01', avg_sync_time_seconds: 95.5, last_sync_size_bytes: 524288000, total_bytes_transferred: 21474836480, sync_count: 120, failure_count: 7, current_rpo_minutes: 180, rpo_target_minutes: 5, rpo_compliant: false, bandwidth_usage_mbps: 0, collected_at: ago(60) },
]
const REPL_HEALTH = {
  total_replications: 3, active: 1, paused: 0, error: 1, rpo_violations: 1, avg_rpo_minutes: 16.7, total_bytes_transferred_24h: 53687091200,
  sites: [
    { site_id: 'site-1', site_name: 'primary-dc', replication_count: 3, health: 'healthy' },
    { site_id: 'site-2', site_name: 'dr-dc', replication_count: 3, health: 'degraded' },
  ],
}
const SR_EXECUTIONS = [
  { id: 'ex-1', plan_id: 'plan-1', plan_name: 'Web tier failover', execution_type: 'test_failover', status: 'completed', progress_pct: 100, current_step: 'Done',
    steps: [{ name: 'Power on web-01', status: 'completed', started_at: ago(7200), completed_at: ago(7000) }, { name: 'Run post script', status: 'completed', started_at: ago(7000), completed_at: ago(6900) }],
    vms_recovered: 2, vms_total: 2, rto_actual_seconds: 300, started_at: ago(7200), completed_at: ago(6900) },
  { id: 'ex-2', plan_id: 'plan-2', plan_name: 'Database recovery', execution_type: 'disaster_recovery', status: 'failed', progress_pct: 40, current_step: 'Power on db-01',
    steps: [{ name: 'Prepare storage', status: 'completed' }, { name: 'Power on db-01', status: 'failed', error: 'datastore unavailable' }, { name: 'Verify', status: 'skipped' }],
    vms_recovered: 0, vms_total: 1, started_at: ago(D), completed_at: ago(D - 400), error: 'datastore unavailable' },
  { id: 'ex-3', plan_id: 'plan-1', plan_name: 'Web tier failover', execution_type: 'planned_migration', status: 'running', progress_pct: 55, current_step: 'Replicating final delta',
    steps: [{ name: 'Replicating final delta', status: 'running', started_at: ago(120) }, { name: 'Power on web-01', status: 'pending' }],
    vms_recovered: 0, vms_total: 2, started_at: ago(180) },
]
const SR_PLANS = [
  { id: 'plan-1', name: 'Web tier failover', description: 'Recover web VMs at DR site', source_site_id: 'site-1', target_site_id: 'site-2',
    vm_groups: [{ name: 'web', vm_ids: ['vm-1', 'vm-3'], boot_order: 1, boot_delay_seconds: 30 }],
    network_mappings: [{ source_network: 'prod-net', target_network: 'dr-net' }],
    storage_mappings: [{ source_datastore: 'ds-a', target_datastore: 'ds-dr' }],
    status: 'ready', last_tested: ago(7200), created: ago(60 * D) },
  { id: 'plan-2', name: 'Database recovery', source_site_id: 'site-1', target_site_id: 'site-2',
    vm_groups: [{ name: 'db', vm_ids: ['vm-2'], boot_order: 1, boot_delay_seconds: 0 }],
    network_mappings: [], storage_mappings: [], status: 'untested', created: ago(30 * D) },
]
const SR_DASHBOARD = {
  total_plans: 2, plans_tested: 1, plans_untested: 1, last_test_date: ago(7200), active_executions: 1, total_protected_vms: 3, total_unprotected_vms: 4,
  avg_rto_seconds: 300, avg_rpo_minutes: 15,
  sites: [
    { site_id: 'site-1', site_name: 'primary-dc', status: 'healthy', protected_vms: 3, plans: 2 },
    { site_id: 'site-2', site_name: 'dr-dc', status: 'degraded', protected_vms: 0, plans: 2 },
  ],
  recent_executions: SR_EXECUTIONS,
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
  if (p === '/networkd/links') return NETD_LINKS
  if (p === '/networkd/bonds') return NETD_BONDS
  if (p === '/networkd/bridges') return NETD_BRIDGES
  if (p === '/networkd/vlans') return NETD_VLANS
  if (p === '/networkd/macvtaps') return NETD_MACVTAPS
  if (p === '/networkd/taps') return NETD_TAPS
  if (p === '/networkd/vxlans') return NETD_VXLANS
  if (p === '/networkd/network-files') return NETD_NETFILES
  if (p === '/networkd/port-forwards') return NETD_PORT_FORWARDS
  if (p === '/sessions') return { items: SESSIONS }
  if (p === '/certificates/cas') return CERT_CAS
  if (p === '/certificates') return CERTS
  if (p === '/certificates/requests') return CERT_REQUESTS
  if (p === '/certificates/attestations') return CERT_ATTESTATIONS
  if (p === '/certificates/security-baselines') return CERT_BASELINES
  if (p === '/certificates/health') return CERT_HEALTH
  if (p === '/system/compliance') return COMPLIANCE
  if (p === '/drs/config/default') return DRS_CONFIG
  if (p === '/drs/balance/default') return DRS_BALANCE
  if (p === '/drs/recommendations/default') return DRS_RECS
  if (p === '/drs/affinity-rules') return DRS_RULES
  if (p === '/federation/overview') return FED_OVERVIEW
  if (p === '/lifecycle/baselines') return LC_BASELINES
  if (p === '/lifecycle/compliance') return LC_COMPLIANCE
  if (p === '/lifecycle/remediations') return LC_REMEDIATIONS
  if (p === '/lifecycle/rolling-updates') return LC_ROLLING
  if (p === '/hosts') return HOSTS
  if (p === '/migrations/report') return MIG_REPORT
  if (p === '/replication/sites') return REPL_SITES
  if (p === '/replication/configs') return REPL_CONFIGS
  if (p === '/replication/rpo-violations') return REPL_VIOLATIONS
  if (p === '/replication/health') return REPL_HEALTH
  if (p === '/site-recovery/plans') return SR_PLANS
  if (p === '/site-recovery/executions') return SR_EXECUTIONS
  if (p === '/site-recovery/dashboard') return SR_DASHBOARD
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
