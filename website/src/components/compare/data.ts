// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

/*
 * Facts for the homepage matrix sections. Every entry cites the repo doc it
 * comes from; nothing here is invented.
 */

export type Level = 'yes' | 'part' | 'no' | 'na';
export type Cell = {t: string; l: Level};

export const cell = (t: string, l: Level = 'yes'): Cell => ({t, l});

const GH_DOCS = 'https://github.com/zyvorai/fabric/tree/main/docs/merion';

/* ------------------------------------------------------------ 1. profiles */
/* Source: docs/merion/MERION.md "Security profiles", docs/merion/MERION-0.2.md. */

export type ProfileStatus = 'available' | 'software' | 'gated';

export type Profile = {
  id: 'standard' | 'measured' | 'confidential';
  name: string;
  sub: string;
  evidence: string;
  attestation: string;
  hostRead: string;
  claim: string;
  status: ProfileStatus;
  statusLabel: string;
  /** Field names are from MERION-0.2.md; the values are illustrative. */
  receipt: [field: string, value: string][];
};

export const PROFILES: Profile[] = [
  {
    id: 'standard',
    name: 'standard',
    sub: 'Ordinary cell',
    evidence: 'none',
    attestation: 'No',
    hostRead: 'Not claimed',
    claim: 'Nothing — no evidence class is attached.',
    status: 'available',
    statusLabel: 'Available',
    receipt: [
      ['profile', '"standard"'],
      ['image_hash', '"(soft)"'],
      ['evidence_class', '"none"'],
      ['snp_launch_verified', 'false'],
      ['tdx_launch_verified', 'false'],
      ['host_recover_allowed', 'true  // dual keys, audited'],
      ['operator_can_read', 'true'],
    ],
  },
  {
    id: 'measured',
    name: 'measured',
    sub: 'Firecracker / KVM microVM',
    evidence: 'software-test',
    attestation: 'Never',
    hostRead: 'Yes — the host can still see a measured VM',
    claim: 'A software-test measurement. Never hardware attestation.',
    status: 'software',
    statusLabel: 'Shipped · software-test',
    receipt: [
      ['profile', '"measured"'],
      ['image_hash', '"(soft)"'],
      ['evidence_class', '"software-test"'],
      ['snp_launch_verified', 'false'],
      ['tdx_launch_verified', 'false'],
      ['host_recover_allowed', 'true  // dual keys, audited'],
      ['operator_can_read', 'true'],
    ],
  },
  {
    id: 'confidential',
    name: 'confidential-snp / tdx',
    sub: 'Hardware-attested guest',
    evidence: 'sev-snp / tdx — only after a verified hardware run',
    attestation: 'Gated',
    hostRead: 'Only once verified: no host-recover path (always 403)',
    claim: 'Unread-by-operator — not claimable until the flags flip.',
    status: 'gated',
    statusLabel: 'Gated on real hardware',
    receipt: [
      ['profile', '"confidential-snp"'],
      ['image_hash', '"(soft)"'],
      ['evidence_class', '"pending hardware run"'],
      ['snp_launch_verified', 'false  // flips after one real launch'],
      ['tdx_launch_verified', 'false'],
      ['host_recover_allowed', 'false  // POST …/host-recover → 403'],
      ['operator_can_read', '"until verified"'],
    ],
  },
];

/* ------------------------------------------------------------ 2. roadmap */
/* Source: docs/merion/STATUS.md, MERION-0.2.md, browser/BROWSER-0.3.md, design/auto-approve-changesets.md, VERIFICATION.md. */

export type RoadState = 'shipped' | 'gated' | 'next';

export type RoadItem = {
  id: string;
  state: RoadState;
  title: string;
  points: string[];
  href: string;
  cta: string;
};

export const ROADMAP: RoadItem[] = [
  {
    id: 'phase6',
    state: 'shipped',
    title: 'FluxVM Phase 6',
    points: ['security_profile field', 'measured profile → software-test evidence'],
    href: 'https://github.com/zyvorai/fluxvm',
    cta: 'FluxVM',
  },
  {
    id: 'pilot',
    state: 'shipped',
    title: 'Merion 0.1 pilot',
    points: ['Live gate passed twice on a FluxVM host', 'Happy path and deny path'],
    href: '/docs/merion/pilot-runs/',
    cta: 'Pilot runs',
  },
  {
    id: 'merion01',
    state: 'shipped',
    title: 'Merion 0.1',
    points: [
      'BYO model socket, signed Sentinel policy',
      'Phone approvals, pack / unpack',
      'Cockpit, PDF brief, host eBPF pin',
    ],
    href: '/docs/merion/',
    cta: 'Merion docs',
  },
  {
    id: 'browser03',
    state: 'shipped',
    title: 'Browser 0.3',
    points: [
      'Split-sight pause, vault-typed fill',
      'Trajectory-as-code, goal-bound tabs',
      'Origin taint lattice',
    ],
    href: '/docs/merion/browser/BROWSER-0.3',
    cta: 'Browser 0.3',
  },
  {
    id: 'merion04',
    state: 'shipped',
    title: 'Merion 0.4',
    points: [
      'Auto-approve policies for speculate, off by default',
      'Journaled with the policy id; a phone signature always wins',
      'Proven through a policy on one real FluxVM lab host',
    ],
    href: '/docs/merion/design/auto-approve-changesets',
    cta: 'Auto-approve',
  },
  {
    id: 'merion02',
    state: 'gated',
    title: 'Merion 0.2',
    points: [
      'Soft scaffolding complete: attestation receipt, no host recover on confidential',
      'Pending hardware: user-held unwrap, verified SNP/TDX flags',
    ],
    href: '/docs/merion/MERION-0.2',
    cta: 'Merion 0.2',
  },
  {
    id: 'next',
    state: 'next',
    title: 'Browser 0.4 / 0.5',
    points: [
      'PacketWolf as an optional CONNECT 5-tuple observer',
      'Signed site adapters (adapters/vcenter.yaml)',
      'Confidential cells: CDP only via attested vsock',
    ],
    href: '/docs/merion/ROADMAP',
    cta: 'Roadmap',
  },
];

/* ------------------------------------------------------------ 3. receipts */
/* Source: each run's SUMMARY.md and pilot-runs/README.md under docs/merion/pilot-runs. */

export type Run = {
  id: string;
  when: string;
  title: string;
  template: string;
  results: string[];
  note?: string;
};

export const RUNS: Run[] = [
  {
    id: '20260924T141927Z',
    when: '2026-09-24 · 14:19 UTC',
    title: 'Pilot gate',
    template: 'node22-agent',
    results: ['Happy path PASS', 'Deny path PASS'],
  },
  {
    id: '20260924T154950Z',
    when: '2026-09-24 · 15:49 UTC',
    title: 'Pilot gate',
    template: 'node22-agent',
    results: ['Happy path PASS', 'Deny path PASS'],
  },
  {
    id: '20260924T182930Z',
    when: '2026-09-24 · 18:29 UTC',
    title: 'Pilot gate on Firecracker',
    template: 'node22-fc',
    results: ['Happy path PASS', 'Deny path PASS', 'cell_backend=flux-vm', 'guest_worker=ok'],
  },
  {
    id: '20260924T190954Z',
    when: '2026-09-24 · 19:09 UTC',
    title: 'Browser screenshot lab proof',
    template: 'node22-agent',
    results: ['Tab listed via …/browser/view', 'Screenshot: HTTP 200, JPEG'],
    note: 'Lab stub (keep-cdp-stub) — no input takeover.',
  },
];

export const GATE_CHECKS: [check: string, result: string][] = [
  ['Template required', 'Missing template → FAIL'],
  ['Merion mode policy', 'Unsigned PUT refused; empty signers refuse start'],
  ['Session + cockpit', 'evidence_class: software-test; restart recovers session'],
  ['Out-of-band approval', 'Approve and deny paths; no unapproved mutate'],
  ['Packaged agents', 'examples/merion-agents/ + goals and artifacts'],
];

export const runHref = (id: string) => `${GH_DOCS}/pilot-runs/${id}`;

/* ---------------------------------------------------------- 6. quickstart */
/* Source: scripts/merion-demo-pdf.sh, scripts/merion-pack-demo.sh,
 * docs/merion/merionctl/README.md and the pack READMEs under examples/merion-agents. */

export type Quick = {
  id: string;
  label: string;
  blurb: string;
  code: string;
  expect?: string;
  needs?: string[];
  href?: string;
  cta?: string;
};

export const QUICKSTARTS: Quick[] = [
  {
    id: 'pdf',
    label: 'PDF brief',
    blurb: 'One request: PDF in, brief.md out, and the script fails if anything connects out.',
    code: `export MERION_API=http://127.0.0.1:9096
export MERION_TOKEN=…   # agent-runtime token
./scripts/merion-demo-pdf.sh examples/merion-agents/pdf-brief/sample.pdf`,
    expect: 'OK — brief.md ready, 0 CONNECT',
    needs: [
      'A FluxVM host and agent-runtime (/healthz)',
      'Template node22-agent with pdftotext (poppler)',
      'Strict confinement: deny_udp + gateway-only ports',
    ],
    href: '/docs/tutorials/merion-pdf-brief',
    cta: 'Tutorial 17',
  },
  {
    id: 'pack',
    label: 'Run a pack',
    blurb: 'Deploy a packaged agent, create a goal, and watch the fix step wait for your approval.',
    code: `export MERION_API=http://127.0.0.1:9096
export MERION_TOKEN=…        # agent-runtime token
export FABRIC_API_TOKEN=…  # fabricd JWT (infra-ops)
./scripts/merion-pack-demo.sh infra-ops`,
    expect: 'Incident-timeline artifact; the fix stays blocked until POST /v1/approvals/{id}',
    needs: [
      'FluxVM template node22-agent (or agent-node)',
      'fabricd reachable over HTTPS',
      'Or pick migration-op / deploy-op',
    ],
    href: '/docs/tutorials/merion-workstation',
    cta: 'Tutorial 16',
  },
  {
    id: 'leave',
    label: 'Pack & leave',
    blurb: 'Export the workstation, then unpack it onto another FluxVM node.',
    code: `merionctl pack   /tmp/merion-pack my-agent
merionctl unpack /tmp/merion-pack my-agent   # on the other node`,
    expect: 'Policy, agent pin and vault names — no raw secrets',
    needs: ['A scoped export token (see PRODUCTION.md)', 'A second FluxVM node to unpack onto'],
    href: '/docs/merion/merionctl/',
    cta: 'merionctl',
  },
];
