import {useCallback, useEffect, useRef, useState} from 'react';
import type {ReactNode, RefObject} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import {PageMetadata} from '@docusaurus/theme-common';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import Reveal from '@site/src/components/Reveal';
import {CtaBand, HeroShell, HonestyBand, Page, Section, useHashScroll} from '@site/src/components/marketing/Shell';
import shell from '@site/src/components/marketing/Shell.module.css';
import CockpitMock from '@site/src/components/compare/CockpitMock';
import ProfileLadder from '@site/src/components/compare/ProfileLadder';
import Quickstart from '@site/src/components/compare/Quickstart';
import Receipts from '@site/src/components/compare/Receipts';
import Roadmap from '@site/src/components/compare/Roadmap';
import {FABRIC_COLS, FABRIC_GROUPS, FLUX_COLS, FLUX_GROUPS, cell as c} from '@site/src/components/compare/data';
import type {Cell, Col, Level, TGroup} from '@site/src/components/compare/data';
import styles from './ComparePage.module.css';

type Tag = 'security' | 'ops' | 'portability';
type Row = {label: string; muse: Cell; keep: Cell; fabric: Cell; flux: Cell};
type Group = {title: string; tag: Tag; rows: Row[]};

const NA_FABRIC = c('Delegates to FluxVM', 'na');
const NA = c('—', 'na');
const UNDOC = c('Not documented publicly', 'na');

/*
 * Muse cells are "as publicly described" — public detail is thin, see the
 * sourcing note on the page. Like the /keep page, only rows stated in
 * docs/merion/MERION.md keep a Muse claim; the rest read "Not documented publicly". Merion/Fabric/FluxVM cells trace to
 * docs/merion/MERION.md, docs/PRODUCT_OVERVIEW.md and fluxvm/README.md.
 */
const GROUPS: Group[] = [
  {
    title: 'Where it runs',
    tag: 'ops',
    rows: [
      {
        label: 'Home',
        muse: c('Meta’s cloud', 'part'),
        keep: c('Laptop, mini-PC, FluxVM host, or rented SNP/TDX — same API'),
        fabric: c('Any Linux host — one ~15 MB Rust daemon'),
        flux: c('One Linux host is enough; grow to multi-host fleets'),
      },
      {
        label: 'Control surface',
        muse: UNDOC,
        keep: c('merionctl, /app/merion console, vsock admin'),
        fabric: c('780+ REST endpoints, CLI, web console, K8s operator, Terraform'),
        flux: c('REST API + CLI with the same verbs'),
      },
    ],
  },
  {
    title: 'Trust & policy',
    tag: 'security',
    rows: [
      {
        label: 'Policy',
        muse: c('Closed Sentinel', 'part'),
        keep: c('Signed merion.policy.yaml — fails closed, diffable in git'),
        fabric: c('3-tier RBAC, audit log, LDAP/OIDC SSO'),
        flux: c('Per-VM security_profile; multi-tenant controls are opt-in', 'part'),
      },
      {
        label: 'Credentials',
        muse: c('Surrogates swapped at egress'),
        keep: c('Vault — the agent never sees real passwords'),
        fabric: NA,
        flux: NA,
      },
      {
        label: 'Approvals',
        muse: c('Bound to a connector'),
        keep: c('Phone-only high-risk approvals, bound to a connector'),
        fabric: NA,
        flux: NA,
      },
      {
        label: 'Training',
        muse: c('Trajectories may train after sanitization', 'no'),
        keep: c('Default off — export needs a scoped token'),
        fabric: NA,
        flux: NA,
      },
    ],
  },
  {
    title: 'Isolation',
    tag: 'security',
    rows: [
      {
        label: 'The cell',
        muse: c('Its own cloud VM; Sentinel on the same machine, kept apart at the system level', 'part'),
        keep: c('Firecracker / KVM microVM via FluxVM'),
        fabric: NA_FABRIC,
        flux: c('Firecracker, Cloud Hypervisor, QEMU/KVM or in-tree — one trait'),
      },
      {
        label: 'Guest access',
        muse: UNDOC,
        keep: c('vsock admin — no SSH to the agent'),
        fabric: NA_FABRIC,
        flux: c('vsock agent: exec, PTY, file copy — no SSH'),
      },
      {
        label: 'Confidential',
        muse: UNDOC,
        keep: c('measured today (software-test); SNP/TDX gated on a verified run', 'part'),
        fabric: NA,
        flux: c('security_profile field; hardware evidence gated', 'part'),
      },
    ],
  },
  {
    title: 'Network proof',
    tag: 'security',
    rows: [
      {
        label: 'Egress pin',
        muse: UNDOC,
        keep: c('deny_udp + gateway-only ports; cockpit CONNECT 0'),
        fabric: c('Cilium-style network policies, WireGuard mesh'),
        flux: c('TC/eBPF Network Fabric (GA): L3/L4 policy, rate limits, live reconfigure'),
      },
      {
        label: 'Proof',
        muse: UNDOC,
        keep: c('Audit journal + FluxVM drop_reasons (PacketWolf optional)'),
        fabric: c('Audit logging'),
        flux: c('drop_reasons from the host pin'),
      },
    ],
  },
  {
    title: 'Model & data',
    tag: 'ops',
    rows: [
      {
        label: 'Model',
        muse: c('Tied to Muse Spark', 'no'),
        keep: c('BYO model socket'),
        fabric: c('AI Workloads (Beta): OpenAI-compatible gateway', 'part'),
        flux: NA,
      },
    ],
  },
  {
    title: 'Portability',
    tag: 'portability',
    rows: [
      {
        label: 'Leaving',
        muse: UNDOC,
        keep: c('merionctl pack / unpack onto another FluxVM'),
        fabric: c('Live migration (disk-copy GA, native preview), VMDK/VDI import'),
        flux: c('qcow2 CoW clones, memory snapshots'),
      },
      {
        label: 'Fleet',
        muse: NA,
        keep: c('Rides on Fabric + FluxVM'),
        fabric: c('Kubernetes operator, Terraform provider'),
        flux: c('Multi-host fleet; K8s operator verified on real k3s'),
      },
      {
        label: 'License',
        muse: c('Closed', 'no'),
        keep: c('Apache-2.0'),
        fabric: c('Apache-2.0'),
        flux: c('Apache-2.0'),
      },
    ],
  },
];

type UseCase = {
  id: string;
  group: 'Packaged agents' | 'Brokered browser' | 'Workstation';
  name: string;
  kind: string;
  headline: string;
  blurb: string;
  flow: [string, string, string];
  guards: string[];
  out: string;
  cmd: string;
  href: string;
  cta: string;
};

const GH = 'https://github.com/zyvorai/fabric/tree/main/examples/merion-agents';

/* Every entry traces to examples/merion-agents/*, docs/merion/browser/BROWSER-0.3.md,
 * docs/merion/approve, docs/merion/PRODUCTION.md or docs/merion/merionctl. */
const USE_CASES: UseCase[] = [
  {
    id: 'pdf-brief',
    group: 'Packaged agents',
    name: 'PDF brief',
    kind: 'Pack · pdf-brief',
    headline: 'PDF in. brief.md out. Zero connects.',
    blurb:
      'The one-click stage demo: drop a document on the cockpit and get a brief back, with no browser and nothing leaving the cell.',
    flow: ['Drop a PDF on /app/merion', 'Agent reads it with pdftotext in the cell', 'brief.md lands as an artifact'],
    guards: ['No browser', 'deny_udp + gateway-only ports', 'Expects egress_connects: 0'],
    out: 'brief.md',
    cmd: './scripts/merion-demo-pdf.sh examples/merion-agents/pdf-brief/sample.pdf',
    href: '/docs/tutorials/merion-pdf-brief',
    cta: 'Tutorial 17',
  },
  {
    id: 'infra-ops',
    group: 'Packaged agents',
    name: 'Incident triage',
    kind: 'Pack · infra-ops',
    headline: 'Read the alerts. Ask before touching a VM.',
    blurb:
      'A Fabric-facing operator that reads system alerts, VM inventory and lifecycle compliance, then proposes a restart or remediation only after you approve it.',
    flow: [
      'Read alerts, VMs and compliance',
      'Write an incident timeline',
      'Apply the fix — blocked until you approve',
    ],
    guards: ['Fix step is requires_approval', 'Fabric API via a vault credential', 'Mutations go through Merion ask'],
    out: 'Incident timeline (markdown) + proposed_fix JSON',
    cmd: './scripts/merion-pack-demo.sh infra-ops',
    href: `${GH}/infra-ops`,
    cta: 'infra-ops on GitHub',
  },
  {
    id: 'migration-op',
    group: 'Packaged agents',
    name: 'Migration planning',
    kind: 'Pack · migration-op',
    headline: 'Plan the waves. Approve the cutover.',
    blurb:
      'Works against Fabric’s /api/migrations and GuestKit inspect / rescue to plan migration waves and check a guest before you move it.',
    flow: ['Inspect the guest with GuestKit', 'Draft the wave plan and preflight report', 'Cutover checklist for you to run'],
    guards: ['Create, cancel and rescue always hit approval', 'Only talks to fabricd', 'Transiva / hypersdk stay out of scope'],
    out: 'Wave plan, preflight report, cutover checklist',
    cmd: './scripts/merion-pack-demo.sh migration-op',
    href: `${GH}/migration-op`,
    cta: 'migration-op on GitHub',
  },
  {
    id: 'deploy-op',
    group: 'Packaged agents',
    name: 'Deployment readiness',
    kind: 'Pack · deploy-op',
    headline: 'Is this install ready? Get it in writing.',
    blurb:
      'Probes fabricd’s /readyz and /health and emits a readiness artifact mapped to Fabric Doctor. It proposes privileged install commands; a human runs them.',
    flow: ['Probe /readyz and /health', 'Map results to Fabric Doctor', 'Propose commands — you run them'],
    guards: ['No host writes by default', 'Privileged steps are proposals only'],
    out: 'Prerequisites checklist + readiness report',
    cmd: './scripts/merion-pack-demo.sh deploy-op',
    href: `${GH}/deploy-op`,
    cta: 'deploy-op on GitHub',
  },
  {
    id: 'browser-research',
    group: 'Packaged agents',
    name: 'Web research',
    kind: 'Pack · browser-research',
    headline: 'Read the web. Touch nothing.',
    blurb:
      'A read-only research agent: open allowlisted hosts, snapshot the accessibility tree, write up what was visible. No purchases, no logins.',
    flow: ['browser_open an allowlisted host', 'browser_snapshot — titles and refs, no HTML', 'Write research.md, then stop'],
    guards: ['allow_hosts allowlist', 'snapshot_only · downloads denied', 'file:// blocked · max 4 tabs'],
    out: 'research.md',
    cmd: 'merionctl create -f examples/merion-agents/browser-research/deploy.json',
    href: `${GH}/browser-research`,
    cta: 'browser-research on GitHub',
  },
  {
    id: 'vault-fill',
    group: 'Brokered browser',
    name: 'Sign in without the password',
    kind: 'Browser 0.3 · vault-typed fill',
    headline: 'The model never holds the password.',
    blurb:
      'When a page needs a login, the session pauses and the host types the secret from the vault. You can watch the tab; the audit line records the source, never the value.',
    flow: ['Agent proposes filling a login field', 'Session pauses — operator can watch', 'Host types the vault secret'],
    guards: ['Audit says vault:name, no value', 'Pause is first-class session state', 'Separate agent / operator cookie jars'],
    out: 'Audit entry with source: vault:name',
    cmd: 'POST /v1/sessions/{id}/browser/fill-secret',
    href: '/docs/merion/browser/BROWSER-0.3',
    cta: 'Browser 0.3',
  },
  {
    id: 'replay',
    group: 'Brokered browser',
    name: 'Record once, replay',
    kind: 'Browser 0.3 · trajectory-as-code',
    headline: 'A browsing session you can run again.',
    blurb:
      'Every act is saved as a browse-script artifact, so a trajectory that worked becomes something you can replay instead of re-prompting.',
    flow: ['Agent browses; each act is logged', 'Saved as artifact kind=browse-script', 'Replay it with merionctl browse replay'],
    guards: ['Chromium and CDP stay host objects', 'Agent sees a11y refs, not raw DOM', 'Paste across disjoint origins denied'],
    out: 'browse-script artifact',
    cmd: 'merionctl browse replay',
    href: '/docs/merion/browser/BROWSER-0.3',
    cta: 'Browser 0.3',
  },
  {
    id: 'goal-tabs',
    group: 'Brokered browser',
    name: 'Goal-scoped browsing',
    kind: 'Browser 0.3 · goal-bound tabs',
    headline: 'Give each goal its own hosts.',
    blurb:
      'A goal carries its own allow_hosts, and its tabs open through the goal — so a research task can’t wander onto hosts that belong to another one.',
    flow: ['Create a goal with allow_hosts', 'Open a tab through the goal', 'Tabs stay inside that allowlist'],
    guards: ['Per-goal host allowlist', 'Live view: pixels for you, a11y tree for the agent'],
    out: 'Goal-bound tab + artifacts on the goal',
    cmd: 'POST /v1/goals/{id}/browse',
    href: '/docs/merion/browser/BROWSER-0.3',
    cta: 'Browser 0.3',
  },
  {
    id: 'approvals',
    group: 'Workstation',
    name: 'Approve from your phone',
    kind: 'Out-of-band approvals',
    headline: 'High-risk actions never confirm in chat.',
    blurb:
      'Buy, send, delete, new login: the approval arrives on a channel the guest cannot see — phone push, hardware key or a local tray app.',
    flow: ['Agent proposes a high-risk action', 'Approval arrives on phone, key or tray', 'A capability token releases that one action'],
    guards: ['Token bound to a connector + action', 'Guest cannot see the channel', 'Pilot gate proves approve and deny paths'],
    out: 'Capability token, not a sentence in chat',
    cmd: 'POST /v1/approvals/{id}',
    href: '/docs/merion/approve/',
    cta: 'Approvals',
  },
  {
    id: 'pack',
    group: 'Workstation',
    name: 'Pack up and leave',
    kind: 'merionctl pack / unpack',
    headline: 'Your workstation is portable.',
    blurb:
      'Export the policy, the agent pin and the vault names, then unpack onto another FluxVM. No lock-in, and no raw secrets in the box.',
    flow: ['Mint a scoped export token', 'merionctl pack → policy, agent pin, vault names', 'merionctl unpack onto another FluxVM'],
    guards: ['Export needs X-Merion-Export-Token', 'Vault names only — no raw secrets', 'Training default off'],
    out: 'A portable pack directory',
    cmd: 'merionctl pack /tmp/keep-pack my-agent',
    href: '/docs/merion/merionctl/',
    cta: 'merionctl',
  },
  {
    id: 'byo',
    group: 'Workstation',
    name: 'Bring your own agent',
    kind: 'fabric-agent build',
    headline: 'Write it in TypeScript. Keep the guardrails.',
    blurb:
      'Build your own agent into a bundle and deploy it under the same signed policy, in the same microVM cell, with the same approvals as the shipped packs.',
    flow: ['Write agent.ts against the Fabric client', 'npx fabric-agent build → one bundle', 'merionctl create -f deploy.json'],
    guards: ['Signed merion.policy.yaml applies', 'BYO model socket', 'Cell is a FluxVM microVM'],
    out: 'A deployed agent + its goals and artifacts',
    cmd: 'npx fabric-agent build agent.ts --out bundle.mjs',
    href: `${GH}/_fabric`,
    cta: 'Shared client on GitHub',
  },
];

const USE_GROUPS = ['Packaged agents', 'Brokered browser', 'Workstation'] as const;

const FILTERS: {key: 'all' | Tag; label: string}[] = [
  {key: 'all', label: 'Everything'},
  {key: 'security', label: 'Security'},
  {key: 'ops', label: 'Operations'},
  {key: 'portability', label: 'Portability'},
];

const GOT_RIGHT = [
  ['One computer per person', 'A persistent Linux box, not a chat session.'],
  ['Two domains', 'Untrusted agent cell vs host-side authority.'],
  ['Surrogate secrets', 'The agent never holds the real password.'],
  ['Capability approvals', 'Bound to a connector, not a sentence in chat.'],
  ['Accessibility-tree browser', 'The driver sees refs, not raw DOM + JS.'],
] as const;

const LEVEL_LABEL: Record<Level, string> = {
  yes: 'Supported',
  part: 'Partial or with caveats',
  no: 'Not offered',
  na: 'Not applicable',
};

const GLYPH: Record<Level, string> = {yes: '●', part: '◐', no: '○', na: '–'};

function useInView<T extends Element>(threshold = 0.12): [RefObject<T | null>, boolean] {
  const ref = useRef<T>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node || typeof IntersectionObserver === 'undefined') {
      setSeen(true);
      return;
    }
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          setSeen(true);
          io.disconnect();
        }
      },
      {threshold},
    );
    io.observe(node);
    return () => io.disconnect();
  }, [threshold]);
  return [ref, seen];
}

function Hero(): ReactNode {
  return (
    <HeroShell
      eyebrow="Zyvor Fabric"
      title="Private cloud control plane for Linux."
      accent="See how it stacks up."
      sub="Fabric vs Proxmox, OpenStack and libvirt — then Merion and FluxVM for the agent stack you run yourself."
      buttons={
        <>
          <a className="button button--primary button--lg" href="#matrix">
            See the matrix
          </a>
          <Link className="button button--outline button--lg button--secondary" to="/docs/getting-started/Quick-Start">
            Get started
          </Link>
        </>
      }
      stats={[
        ['1', '~15 MB Rust daemon'],
        ['3', 'open layers, one stack'],
        ['0', 'egress connects, on stage'],
      ]}
      cueHref="#matrix"
      cueLabel="Scroll to the matrix"
    />
  );
}

function StackScene(): ReactNode {
  const layers = [
    ['Merion', 'Product layer', 'Policy, vault, approvals, browser, demos'],
    ['Fabric', 'Control plane', 'Console, JWT, Agents / Sessions'],
    ['FluxVM', 'Hypervisor', 'The cell + the host TC/eBPF pin'],
  ] as const;
  return (
    <div className={styles.scene}>
      <Reveal className={styles.ghost}>
        <span className={styles.ghostTag}>Outside your walls</span>
        <b>Muse</b>
        <span>Closed personal agent on Meta’s cloud, as publicly described.</span>
      </Reveal>
      <div className={styles.vs} aria-hidden>
        vs
      </div>
      <ol className={styles.layers} aria-label="The open stack, top to bottom">
        {layers.map(([name, role, blurb], i) => (
          <li key={name}>
            <Reveal delay={i * 140}>
              <div className={styles.layer}>
                <span className={styles.layerIdx} aria-hidden>
                  0{i + 1}
                </span>
                <span className={styles.layerName}>{name}</span>
                <span className={styles.layerRole}>{role}</span>
                <span className={styles.layerBlurb}>{blurb}</span>
              </div>
            </Reveal>
            {i < layers.length - 1 && (
              <span className={styles.runsOn} aria-hidden>
                runs on
              </span>
            )}
          </li>
        ))}
      </ol>
    </div>
  );
}

const MUSE_COLS: Col[] = [
  {key: 'muse', label: 'Muse', sub: 'as publicly described', dim: true},
  {key: 'keep', label: 'Merion', sub: 'product layer', hl: true},
  {key: 'fabric', label: 'Fabric', sub: 'control plane'},
  {key: 'flux', label: 'FluxVM', sub: 'hypervisor'},
];

const MUSE_GROUPS: (TGroup & {tag: Tag})[] = GROUPS.map((g) => ({
  title: g.title,
  tag: g.tag,
  rows: g.rows.map((r) => ({label: r.label, cells: [r.muse, r.keep, r.fabric, r.flux]})),
}));

type TabId = 'stack' | 'fabric' | 'flux';

const TABS: {
  id: TabId;
  label: string;
  caption: string;
  cols: Col[];
  groups: (TGroup & {tag?: Tag})[];
  note: ReactNode;
}[] = [
  {
    id: 'stack',
    label: 'Muse vs the Merion stack',
    caption:
      'Muse compared with Merion, Fabric and FluxVM across trust, isolation, network proof and portability.',
    cols: MUSE_COLS,
    groups: MUSE_GROUPS,
    note: null,
  },
  {
    id: 'fabric',
    label: 'Fabric vs the field',
    caption: 'Fabric compared with Proxmox VE, OpenStack and libvirt on capabilities.',
    cols: FABRIC_COLS,
    groups: FABRIC_GROUPS,
    note: (
      <>
        Capability rows from the <Link to="/docs/PRODUCT_OVERVIEW">product overview</Link>. The
        other columns are Zyvor’s reading of those projects — check their docs before you decide.
      </>
    ),
  },
  {
    id: 'flux',
    label: 'FluxVM vs libvirt',
    caption: 'FluxVM commands mapped to their libvirt / virsh equivalents.',
    cols: FLUX_COLS,
    groups: FLUX_GROUPS,
    note: (
      <>
        FluxVM is a host-local replacement for libvirt/virsh lifecycle and networking — not a
        drop-in for KubeVirt or OpenShift.
      </>
    ),
  },
];

const TAB_IDS = TABS.map((x) => x.id);
const FILTER_IDS = FILTERS.map((f) => f.key);
const UC_IDS = USE_CASES.map((u) => u.id);

/**
 * State that mirrors a query param (?f=security). The param is applied after
 * mount so the static HTML and first client render always agree, and written
 * with replaceState so Docusaurus doesn't treat it as a navigation.
 */
function useQueryState<T extends string>(
  key: string,
  allowed: readonly T[],
  fallback: T,
): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(fallback);
  useEffect(() => {
    const raw = new URLSearchParams(window.location.search).get(key);
    if (raw && (allowed as readonly string[]).includes(raw)) {
      setValue(raw as T);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const set = useCallback(
    (v: T) => {
      setValue(v);
      const url = new URL(window.location.href);
      if (v === fallback) {
        url.searchParams.delete(key);
      } else {
        url.searchParams.set(key, v);
      }
      window.history.replaceState(window.history.state, '', url);
    },
    [key, fallback],
  );
  return [value, set];
}

function CellView({cell, col}: {cell: Cell; col: Col}): ReactNode {
  return (
    <td
      className={clsx(col.hl && styles.hl, col.dim && styles.dim, styles[`lvl_${cell.l}`])}
      data-col={col.label}>
      <span className={styles.glyph} aria-hidden>
        {GLYPH[cell.l]}
      </span>
      <span className={styles.srOnly}>{LEVEL_LABEL[cell.l]}: </span>
      {cell.t}
    </td>
  );
}

function GroupBody({group, cols}: {group: TGroup; cols: Col[]}): ReactNode {
  const [ref, seen] = useInView<HTMLTableSectionElement>();
  return (
    <tbody ref={ref} className={clsx(styles.group, seen && styles.groupSeen)}>
      <tr className={styles.groupRow}>
        <th colSpan={cols.length + 1} scope="colgroup">
          {group.title}
        </th>
      </tr>
      {group.rows.map((r) => (
        <tr key={r.label}>
          <th scope="row">{r.label}</th>
          {r.cells.map((cl, i) => (
            <CellView key={cols[i].key} cell={cl} col={cols[i]} />
          ))}
        </tr>
      ))}
    </tbody>
  );
}

function Matrix(): ReactNode {
  const [tabId, setTab] = useQueryState<TabId>('t', TAB_IDS, 'fabric');
  const [filter, setFilter] = useQueryState<'all' | Tag>('f', FILTER_IDS, 'all');
  const tab = TABS.find((x) => x.id === tabId) ?? TABS[0];
  const groups = tab.groups.filter((g) => !g.tag || filter === 'all' || g.tag === filter);
  return (
    <>
      <div className={styles.tabs} role="group" aria-label="Choose a comparison">
        {TABS.map((x) => (
          <button
            key={x.id}
            type="button"
            className={clsx(styles.tabBtn, x.id === tab.id && styles.tabOn)}
            aria-pressed={x.id === tab.id}
            onClick={() => setTab(x.id)}>
            {x.label}
          </button>
        ))}
      </div>
      {tab.id === 'stack' && (
        <div className={styles.filters} role="group" aria-label="Filter the comparison">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              type="button"
              className={clsx(styles.chip, filter === f.key && styles.chipOn)}
              aria-pressed={filter === f.key}
              onClick={() => setFilter(f.key)}>
              {f.label}
            </button>
          ))}
        </div>
      )}
      <table className={styles.matrix} key={tab.id}>
        <caption className={styles.srOnly}>{tab.caption}</caption>
        <thead>
          <tr>
            <td />
            {tab.cols.map((col) => (
              <th
                key={col.key}
                scope="col"
                className={clsx(col.hl && styles.hHl, col.dim && styles.hDim)}>
                {col.label}
                {col.sub && <small>{col.sub}</small>}
              </th>
            ))}
          </tr>
        </thead>
        {groups.map((g) => (
          <GroupBody key={g.title} group={g} cols={tab.cols} />
        ))}
      </table>
      <p className={styles.legend} aria-hidden>
        <span>● supported</span>
        <span>◐ partial / caveat</span>
        <span>○ not offered</span>
        <span>– n/a</span>
      </p>
      {tab.note && <p className={styles.tabNote}>{tab.note}</p>}
    </>
  );
}

function UseCases(): ReactNode {
  const [id, setId] = useQueryState('uc', UC_IDS, USE_CASES[0].id);
  const uc = USE_CASES.find((u) => u.id === id) ?? USE_CASES[0];
  return (
    <div className={styles.uc}>
      <nav className={styles.ucRail} aria-label="Use cases">
        {USE_GROUPS.map((g) => (
          <div key={g} className={styles.ucGroup}>
            <p className={styles.ucGroupTitle}>{g}</p>
            {USE_CASES.filter((u) => u.group === g).map((u) => (
              <button
                key={u.id}
                type="button"
                className={clsx(styles.ucBtn, u.id === id && styles.ucBtnOn)}
                aria-pressed={u.id === id}
                onClick={() => setId(u.id)}>
                {u.name}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <article className={styles.ucPanel} key={uc.id} aria-live="polite">
        <span className={styles.ucKind}>{uc.kind}</span>
        <Heading as="h3" className={styles.ucHead}>
          {uc.headline}
        </Heading>
        <p className={styles.ucBlurb}>{uc.blurb}</p>
        <ol className={styles.ucFlow}>
          {uc.flow.map((s, i) => (
            <li key={s}>
              <span aria-hidden>{i + 1}</span>
              {s}
            </li>
          ))}
        </ol>
        <ul className={styles.ucGuards} aria-label="Guardrails">
          {uc.guards.map((g) => (
            <li key={g}>{g}</li>
          ))}
        </ul>
        <pre className={styles.ucCmd}>
          <code>{uc.cmd}</code>
        </pre>
        <p className={styles.ucOut}>
          <b>Produces</b> {uc.out}
        </p>
        <Link className={shell.cardLink} to={uc.href}>
          {uc.cta} ›
        </Link>
      </article>
    </div>
  );
}

function Proof(): ReactNode {
  return (
    <section className={clsx(shell.section, shell.black)}>
      <div className={clsx(shell.wrap, styles.proofGrid)}>
        <Reveal>
          <p className={shell.eyebrow}>Proof on stage</p>
          <Heading as="h2" className={clsx(shell.title, shell.onDark)}>
            Don’t trust the story.
            <br />
            Read the counter.
          </Heading>
          <p className={clsx(shell.lede, shell.onDarkMuted)}>
            The PDF-brief demo runs an agent through a vendor SOW behind a host-side eBPF pin —{' '}
            <code>deny_udp</code> plus gateway-only ports. It expects <code>egress_connects: 0</code>,
            read from Merion’s journal and FluxVM’s <code>drop_reasons</code>.
          </p>
          <div className={shell.btnrow}>
            <Link className="button button--primary button--lg" to="/docs/tutorials/merion-pdf-brief">
              Run Tutorial 17
            </Link>
            <Link
              className="button button--outline button--lg button--secondary"
              to="/docs/merion/demos/pdf-brief">
              Demo docs
            </Link>
          </div>
        </Reveal>
        <Reveal delay={150} className={styles.counter}>
          <div className={styles.ring} aria-hidden />
          <div className={styles.zero}>0</div>
          <div className={styles.counterLabel}>
            <code>egress_connects</code>
          </div>
          <ul className={styles.sources}>
            <li>Merion audit journal</li>
            <li>FluxVM host TC/eBPF pin</li>
          </ul>
        </Reveal>
      </div>
    </section>
  );
}

const CARDS = [
  {
    name: 'Merion',
    role: 'The agent workstation',
    points: ['Signed policy that fails closed', 'Vault, approvals, brokered browser', 'pack / unpack to leave'],
    to: '/docs/merion/',
    cta: 'Merion docs',
  },
  {
    name: 'Fabric',
    role: 'The control plane',
    points: ['780+ endpoint REST API, one daemon', 'RBAC, audit, HA, live migration', 'CLI, console, K8s operator, Terraform'],
    to: '/docs/PRODUCT_OVERVIEW',
    cta: 'Product overview',
  },
  {
    name: 'FluxVM',
    role: 'The hypervisor',
    points: ['Firecracker, Cloud Hypervisor, QEMU/KVM', 'vsock agent — no SSH', 'TC/eBPF Network Fabric (GA)'],
    to: 'https://github.com/zyvorai/fluxvm',
    cta: 'FluxVM on GitHub',
  },
] as const;

/** Scrolls to the URL hash after mount (native hash scroll misses late-laid-out sections). */
export default function ComparePage(): ReactNode {
  useHashScroll();
  return (
    <Layout
      title="Zyvor Fabric — private cloud control plane"
      description="Fabric vs the field: compare Zyvor Fabric with Proxmox, OpenStack and libvirt — then Merion, FluxVM, proof, and a runnable quickstart.">
      <PageMetadata image="/img/compare-social.png" />
      <Page>
        <Hero />
        <main>
          <Section
            id="matrix"
            eyebrow="The matrix"
            title="Side by side."
            lede="Start with Fabric vs the field. Switch tabs for Muse vs the Merion stack, or FluxVM vs libvirt."
            wide>
            <Matrix />
          </Section>

          <Section
            eyebrow="What Muse got right"
            title="Same threat model. Credit where due."
            lede="Muse’s public story gets the shape right. Merion starts from the same five ideas."
            tint>
            <ul className={shell.tiles}>
              {GOT_RIGHT.map(([h, b], i) => (
                <li key={h}>
                  <Reveal delay={i * 80} className={shell.tile}>
                    <b>{h}</b>
                    <span>{b}</span>
                  </Reveal>
                </li>
              ))}
            </ul>
          </Section>

          <Section
            id="stack"
            eyebrow="The stack"
            title="One outsider. Three layers you own."
            lede="Merion, Fabric and FluxVM aren’t rivals — they’re one stack. Muse is the reference point outside it.">
            <StackScene />
          </Section>

          <Section
            id="profiles"
            eyebrow="Security profiles"
            title="Three rungs. One says what it can’t prove."
            lede="Merion labels every cell with the evidence it actually has. Measured is software-test; hardware attestation stays gated until a verified run."
            tint
            wide>
            <ProfileLadder />
          </Section>

          <Proof />

          <Section
            id="cockpit"
            eyebrow="The cockpit"
            title="What you watch while it works."
            lede="A goal, its plan, the decisions the policy made, and the egress counter — with the honesty badge always on."
            wide>
            <CockpitMock />
          </Section>

          <Section
            id="use-cases"
            eyebrow="Use cases"
            title="Not just a PDF."
            lede="Five packaged agents, three brokered-browser workflows and three workstation moves — every one under the same signed policy, in the same microVM cell."
            tint
            wide>
            <UseCases />
            <p className={styles.ucNote}>
              Every run today reports <code>software-test</code> evidence — see the honesty band
              below.
            </p>
          </Section>

          <Section
            id="receipts"
            eyebrow="Receipts"
            title="Run, archived, in the repo."
            lede="The pilot gate has passed on a real FluxVM host — happy path and deny path — with logs archived under docs/merion/pilot-runs/."
            wide>
            <Receipts />
          </Section>

          <Section
            id="roadmap"
            eyebrow="Roadmap"
            title="Shipped, gated, next."
            lede="What is done, what waits on hardware, and what comes after — straight from STATUS.md and the Merion 0.2 notes."
            tint
            wide>
            <Roadmap />
          </Section>

          <Section
            id="quickstart"
            eyebrow="Try it"
            title="Copy, paste, run."
            lede="Three commands from the repo. You need a FluxVM host; nothing here runs in Meta’s cloud.">
            <Quickstart />
          </Section>

          <Section eyebrow="Pick your layer" title="Use one. Use all three." tint>
            <ul className={shell.cards}>
              {CARDS.map((card, i) => (
                <li key={card.name}>
                  <Reveal delay={i * 100} className={shell.fill}>
                    <div className={shell.card}>
                      <span className={shell.cardRole}>{card.role}</span>
                      <Heading as="h3">{card.name}</Heading>
                      <ul>
                        {card.points.map((p) => (
                          <li key={p}>{p}</li>
                        ))}
                      </ul>
                      <Link className={shell.cardLink} to={card.to}>
                        {card.cta} ›
                      </Link>
                    </div>
                  </Reveal>
                </li>
              ))}
            </ul>
          </Section>

          <HonestyBand
            items={[
              <>
                Until Merion 0.2 runs on real SNP/TDX with a user-held key, evidence stays{' '}
                <code>software-test</code> — never “the operator cannot read this.” Muse’s Secure VM
                has the same limit today.
              </>,
              <>FluxVM’s multi-tenant controls are opt-in and are not a public-cloud boundary.</>,
              <>
                Muse details here are as publicly described; public detail is thin.{' '}
                <a href="https://github.com/zyvorai/fabric/issues">Corrections welcome.</a>
              </>,
            ]}
          />

          <CtaBand title="Run the open version.">
            <Link className="button button--primary button--lg" to="/docs/getting-started/Quick-Start">
              Quick start
            </Link>
            <Link className="button button--outline button--lg button--secondary" to="/merion">
              Meet Merion
            </Link>
          </CtaBand>
        </main>
      </Page>
    </Layout>
  );
}
