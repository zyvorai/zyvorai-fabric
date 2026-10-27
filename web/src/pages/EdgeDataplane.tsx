// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react'
import { ExternalLink, Loader2, Plus, Trash2 } from 'lucide-react'
import {
  CiliumEndpointView,
  DataplaneHealth,
  IdentityInfo,
  IpcacheEntry,
  NetworkServiceSpec,
  HostServiceStatus,
  SecurityGroup,
  applyDataplaneCnp,
  deleteDataplaneCnp,
  deleteDataplaneGroup,
  deleteDataplaneService,
  emptyPolicy,
  getDataplaneHealth,
  getDataplaneHubbleFlows,
  getDataplaneObserve,
  getDataplaneServicesStatus,
  getDataplaneServicesHealth,
  getDataplaneServicesAdvertisements,
  reconcileDataplaneServicesHealth,
  gcDataplaneServicesConntrack,
  listDataplaneCnp,
  listDataplaneEndpoints,
  listDataplaneGroups,
  listDataplaneIdentities,
  listDataplaneIpcache,
  listDataplaneServices,
  refreshDataplaneDns,
  upsertDataplaneGroup,
  upsertDataplaneService,
} from '../api/dataplane'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'
import { formatUserError } from '../utils/apiError'
import { usePermissions } from '../hooks/usePermissions'
import SubsystemBanner from '../components/SubsystemBanner'
import { PageHeader } from '../components/ui'
import { TerminalTextarea } from '../components/AppleTerminalFrame'
import { usePlatformInfo } from '../contexts/PlatformInfoContext'
import PacketFlowPanel from '../components/PacketFlowPanel'
import { fromHubbleFlow, type PacketFlowView } from '../lib/packetflow'

type Tab =
  | 'health'
  | 'services'
  | 'groups'
  | 'cnp'
  | 'endpoints'
  | 'identities'
  | 'observe'
  | 'flows'
  | 'ipcache'

const SAMPLE_SERVICE = `{
  "name": "payments",
  "vip": "10.96.10.25",
  "port": 443,
  "protocol": "tcp",
  "algorithm": "maglev",
  "mode": "nat",
  "exposure": "east-west",
  "advertise": false,
  "max_egress_mbps": 5000,
  "flow_sample_rate": 100,
  "host_routing": true,
  "maglev_table_size": 4093,
  "backends": [
    {"address": "10.40.1.21", "port": 8443, "weight": 2, "enabled": true, "state": "ready"},
    {"address": "10.40.1.22", "port": 8443, "weight": 1, "enabled": true, "state": "ready"}
  ]
}`

const SAMPLE_SERVICE_NS = `{
  "name": "edge-http",
  "vip": "203.0.113.50",
  "port": 80,
  "protocol": "tcp",
  "mode": "nat",
  "exposure": "north-south",
  "snat_address": "203.0.113.10",
  "advertise": false,
  "max_egress_mbps": 2000,
  "flow_sample_rate": 50,
  "host_routing": false,
  "health_check": {
    "kind": "tcp",
    "timeout_ms": 500,
    "unhealthy_threshold": 3,
    "healthy_threshold": 2
  },
  "backends": [
    {"address": "10.40.1.21", "port": 8080, "weight": 1, "enabled": true, "state": "ready"},
    {"address": "10.40.2.21", "port": 8080, "weight": 1, "enabled": true, "state": "draining", "drain_until_unix_ms": 1788850000000}
  ]
}`

const SAMPLE_CNP = `{
  "apiVersion": "cilium.io/v2",
  "kind": "CiliumNetworkPolicy",
  "metadata": { "name": "web-egress" },
  "spec": {
    "endpointSelector": { "matchLabels": { "app": "web" } },
    "egress": [{
      "toCIDR": ["10.0.0.0/8"],
      "toPorts": [{ "ports": [{ "port": "443", "protocol": "TCP" }] }]
    }]
  }
}`

export default function EdgeDataplane() {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const { capabilities } = usePlatformInfo()
  const hubbleUrl = capabilities?.hubble_ui_url?.trim() || ''
  const [tab, setTab] = useState<Tab>('health')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [health, setHealth] = useState<DataplaneHealth | null>(null)
  const [groups, setGroups] = useState<SecurityGroup[]>([])
  const [services, setServices] = useState<NetworkServiceSpec[]>([])
  const [serviceHostStatus, setServiceHostStatus] = useState<HostServiceStatus | null>(null)
  const [serviceHealth, setServiceHealth] = useState<unknown>(null)
  const [serviceAds, setServiceAds] = useState<unknown>(null)
  const [cnps, setCnps] = useState<unknown[]>([])
  const [identities, setIdentities] = useState<IdentityInfo[]>([])
  const [endpoints, setEndpoints] = useState<CiliumEndpointView[]>([])
  const [observe, setObserve] = useState<Record<string, unknown> | null>(null)
  const [ipcache, setIpcache] = useState<IpcacheEntry[]>([])
  const [packetFlows, setPacketFlows] = useState<PacketFlowView[]>([])
  const [groupName, setGroupName] = useState('web')
  const [groupLabel, setGroupLabel] = useState('app=web')
  const [cnpJson, setCnpJson] = useState(SAMPLE_CNP)
  const [serviceJson, setServiceJson] = useState(SAMPLE_SERVICE)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    setError(null)
    try {
      const [h, g, svc, svcStatus, svcHealth, svcAds, c, ids, eps, obs, ipc, hubble] = await Promise.all([
        getDataplaneHealth(),
        listDataplaneGroups().catch(() => ({ items: [] as SecurityGroup[] })),
        listDataplaneServices().catch(() => ({ items: [] as NetworkServiceSpec[] })),
        getDataplaneServicesStatus().catch(() => null),
        getDataplaneServicesHealth().catch(() => null),
        getDataplaneServicesAdvertisements().catch(() => null),
        listDataplaneCnp().catch(() => ({ items: [] as unknown[] })),
        listDataplaneIdentities().catch(() => ({ items: [] as IdentityInfo[] })),
        listDataplaneEndpoints().catch(() => ({ items: [] as CiliumEndpointView[] })),
        getDataplaneObserve().catch(() => null),
        listDataplaneIpcache().catch(() => ({ items: [] as IpcacheEntry[] })),
        getDataplaneHubbleFlows(64).catch(() => ({ items: [] })),
      ])
      setHealth(h)
      setGroups(g.items ?? [])
      setServices(svc.items ?? [])
      setServiceHostStatus(svcStatus)
      setServiceHealth(svcHealth)
      setServiceAds(svcAds)
      setCnps(c.items ?? [])
      setIdentities(ids.items ?? [])
      setEndpoints(eps.items ?? [])
      setObserve(obs)
      setIpcache(ipc.items ?? [])
      const mode = h.mode || 'ebpf'
      setPacketFlows((hubble.items ?? []).map((f) => fromHubbleFlow(f, mode)))
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const createGroup = async () => {
    if (!groupName.trim()) return
    setBusy(true)
    try {
      const labels = groupLabel.trim() ? [groupLabel.trim()] : []
      await upsertDataplaneGroup({
        name: groupName.trim(),
        labels,
        identity: 0,
        priority: 10,
        description: 'Created from Edge Dataplane console',
        policy: {
          ...emptyPolicy(),
          default_allow: false,
          allow_cidrs: ['10.0.0.0/8'],
          allow_ports: ['tcp/443', 'udp/53'],
          allow_icmp: true,
        },
      })
      toast.success(`Group '${groupName.trim()}' saved`)
      await load()
    } catch (err) {
      toastFailure(toast, 'Failed to save group', err)
    } finally {
      setBusy(false)
    }
  }

  const removeGroup = async (name: string) => {
    setBusy(true)
    try {
      await deleteDataplaneGroup(name)
      toast.success(`Deleted group '${name}'`)
      await load()
    } catch (err) {
      toastFailure(toast, 'Failed to delete group', err)
    } finally {
      setBusy(false)
    }
  }

  const applyService = async () => {
    setBusy(true)
    try {
      const service = JSON.parse(serviceJson) as NetworkServiceSpec
      const status = await upsertDataplaneService(service)
      toast.success(
        `Service '${status.name}' saved (${status.active_backends} backends, Maglev ${status.maglev_table_size})`,
      )
      await load()
    } catch (err) {
      toastFailure(toast, 'Failed to save service', err)
    } finally {
      setBusy(false)
    }
  }

  const removeService = async (name: string) => {
    setBusy(true)
    try {
      await deleteDataplaneService(name)
      toast.success(`Deleted service '${name}'`)
      await load()
    } catch (err) {
      toastFailure(toast, 'Failed to delete service', err)
    } finally {
      setBusy(false)
    }
  }

  const applyCnp = async () => {
    setBusy(true)
    try {
      const doc = JSON.parse(cnpJson)
      await applyDataplaneCnp(doc)
      toast.success('CNP applied')
      await load()
    } catch (err) {
      toastFailure(toast, 'Failed to apply CNP', err)
    } finally {
      setBusy(false)
    }
  }

  const removeCnp = async (name: string) => {
    setBusy(true)
    try {
      await deleteDataplaneCnp(name)
      toast.success(`Deleted CNP '${name}'`)
      await load()
    } catch (err) {
      toastFailure(toast, 'Failed to delete CNP', err)
    } finally {
      setBusy(false)
    }
  }

  const doRefreshDns = async () => {
    setBusy(true)
    try {
      const r = await refreshDataplaneDns()
      toast.success(`Refreshed ${r.refreshed} FQDN polic(ies)`)
      await load()
    } catch (err) {
      toastFailure(toast, 'refresh-dns failed', err)
    } finally {
      setBusy(false)
    }
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: 'health', label: 'Health' },
    { id: 'services', label: 'Services' },
    { id: 'groups', label: 'Groups' },
    { id: 'cnp', label: 'CNP' },
    { id: 'endpoints', label: 'Endpoints' },
    { id: 'identities', label: 'Identities' },
    { id: 'observe', label: 'Observe' },
    { id: 'flows', label: 'Packet flow' },
    { id: 'ipcache', label: 'Ipcache' },
  ]

  if (loading) {
    return (
      <div className="p-8 text-center">
        <Loader2 className="w-6 h-6 text-[var(--zf-muted)] mx-auto mb-2 animate-spin" />
        <p className="text-sm text-[var(--zf-muted)]">Loading edge dataplane…</p>
      </div>
    )
  }

  return (
    <div className="space-y-4 p-4 sm:p-6 max-w-6xl mx-auto">
      <SubsystemBanner subsystem="vm_dataplane" title="Edge dataplane" />
      <PageHeader
        title="Edge Dataplane"
        description="FluxVM Network Fabric schema v4 — Maglev services, security groups, CNP, CEP endpoints (`identity_source`), identities, Hubble-style packet flow, health, and ipcache. VM edge plane, not Fabric SDN."
        onRefresh={() => {
          setLoading(true)
          void load()
        }}
        actions={
          <>
            {hubbleUrl && (
              <a
                href={hubbleUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="zf-btn zf-btn-ghost zf-btn-sm"
                title="Opens external Hubble UI (Cilium). VMs are not Cilium endpoints."
              >
                <ExternalLink className="w-3.5 h-3.5" />
                Open Hubble
              </a>
            )}
            {canWrite && (
              <button
                type="button"
                disabled={busy}
                onClick={() => void doRefreshDns()}
                className="zf-btn zf-btn-ghost zf-btn-sm"
              >
                Refresh DNS
              </button>
            )}
          </>
        }
      />

      {error && (
        <p className="text-sm text-red-700 bg-red-50 border border-red-200 rounded-lg px-3 py-2">
          {error}
        </p>
      )}

      <div className="flex gap-1 border-b border-[var(--zf-hairline)] overflow-x-auto">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => setTab(t.id)}
            className={`px-3 py-2 text-sm font-medium relative whitespace-nowrap ${
              tab === t.id ? 'text-[var(--zf-link)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
            }`}
          >
            {t.label}
            {tab === t.id && (
              <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-[var(--zf-link-fill)] rounded-full" />
            )}
          </button>
        ))}
      </div>

      {tab === 'health' && health && (
        <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 space-y-3">
          <div className="flex flex-wrap gap-2 text-xs">
            <span
              className={`px-2 py-1 rounded-full border ${
                health.ok
                  ? 'bg-emerald-50 text-emerald-700 border-emerald-200'
                  : 'bg-amber-50 text-amber-800 border-amber-200'
              }`}
            >
              {health.ok ? 'ok' : 'check notes'}
            </span>
            <span className="px-2 py-1 rounded-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)]">
              mode={health.mode}
            </span>
          </div>
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-sm">
            <Cell label="BPF object" value={health.bpf_object_present ? 'yes' : 'no'} />
            <Cell label="Pin root" value={health.pin_root_present ? 'yes' : 'no'} />
            <Cell label="bpffs" value={health.bpffs_present ? 'yes' : 'no'} />
            <Cell label="Required" value={health.required ? 'yes' : 'no'} />
            <Cell label="Groups" value={String(health.groups)} />
            <Cell label="CNP policies" value={String(health.policies)} />
            <Cell label="Ipcache" value={String(health.ipcache_entries)} />
            <Cell label="Default allow" value={health.default_allow ? 'yes' : 'no'} />
          </div>
          {health.notes?.length > 0 && (
            <ul className="text-sm text-amber-800 list-disc pl-5">
              {health.notes.map((n) => (
                <li key={n}>{n}</li>
              ))}
            </ul>
          )}
        </div>
      )}

      {tab === 'services' && (
        <div className="space-y-4">
          {serviceHostStatus && (
            <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 text-sm space-y-1">
              <p className="font-medium text-[var(--zf-ink)]">
                Service Fabric schema v{serviceHostStatus.schema_version}
                {serviceHostStatus.xdp_acceleration ? ' · XDP acceleration on' : ''}
              </p>
              <p className="text-[var(--zf-muted)]">
                North-south interfaces:{' '}
                {serviceHostStatus.north_south_interfaces.length > 0
                  ? serviceHostStatus.north_south_interfaces.join(', ')
                  : 'none (east-west / VM-edge only)'}
              </p>
              {canWrite && (
                <div className="flex flex-wrap gap-2 pt-2">
                  <button
                    type="button"
                    disabled={busy}
                    className="text-xs px-2 py-1 rounded border border-[var(--zf-hairline)]"
                    onClick={() => {
                      setBusy(true)
                      void reconcileDataplaneServicesHealth()
                        .then((v) => {
                          setServiceHealth(v)
                          toast.success('Health reconciled')
                        })
                        .catch((e) => toastFailure(toast, 'Health reconcile failed', e))
                        .finally(() => setBusy(false))
                    }}
                  >
                    Reconcile health
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    className="text-xs px-2 py-1 rounded border border-[var(--zf-hairline)]"
                    onClick={() => {
                      setBusy(true)
                      void gcDataplaneServicesConntrack()
                        .then(() => toast.success('Conntrack GC complete'))
                        .catch((e) => toastFailure(toast, 'Conntrack GC failed', e))
                        .finally(() => setBusy(false))
                    }}
                  >
                    Conntrack GC
                  </button>
                </div>
              )}
              {serviceHealth != null && (
                <pre className="mt-2 max-h-40 overflow-auto text-xs font-mono text-[var(--zf-muted)] bg-[var(--zf-canvas)] p-2 rounded">
                  {JSON.stringify(serviceHealth, null, 2)}
                </pre>
              )}
              {serviceAds != null && (
                <pre className="mt-2 max-h-40 overflow-auto text-xs font-mono text-[var(--zf-muted)] bg-[var(--zf-canvas)] p-2 rounded">
                  {JSON.stringify(serviceAds, null, 2)}
                </pre>
              )}
            </div>
          )}
          {canWrite && (
            <div className="space-y-2">
              <div className="flex gap-2">
                <button
                  type="button"
                  className="text-xs px-2 py-1 rounded border border-[var(--zf-hairline)]"
                  onClick={() => setServiceJson(SAMPLE_SERVICE)}
                >
                  East-west sample
                </button>
                <button
                  type="button"
                  className="text-xs px-2 py-1 rounded border border-[var(--zf-hairline)]"
                  onClick={() => setServiceJson(SAMPLE_SERVICE_NS)}
                >
                  North-south + drain sample
                </button>
              </div>
              <TerminalTextarea
                title="Maglev service JSON"
                className="min-h-[180px] font-mono text-xs"
                value={serviceJson}
                onChange={(e) => setServiceJson(e.target.value)}
                spellCheck={false}
              />
              <button
                type="button"
                disabled={busy}
                onClick={() => void applyService()}
                className="inline-flex items-center gap-1.5 px-3 py-2 text-sm rounded-lg bg-[var(--zf-link-fill)] text-white"
              >
                <Plus className="w-3.5 h-3.5" /> Apply Maglev service
              </button>
              <p className="text-xs text-[var(--zf-muted)]">
                Service Fabric v5: conntrack affinity, ready/draining/unhealthy backends,
                optional TCP health checks, VIP advertise intent, EDT pacing,
                FluxScope sampling, opt-in host-routing, and HA delta journal replication
                (sequence/ack with full-snapshot fallback). North-south NAT still needs
                <code>snat_address</code> and FluxVM <code>north_south_interfaces</code>.
              </p>
            </div>
          )}
          <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
            <table className="w-full text-sm">
              <thead className="bg-[var(--zf-canvas)] text-[var(--zf-muted)] text-left">
                <tr>
                  <th className="px-3 py-2 font-medium">Name</th>
                  <th className="px-3 py-2 font-medium">VIP</th>
                  <th className="px-3 py-2 font-medium">Port</th>
                  <th className="px-3 py-2 font-medium">Mode</th>
                  <th className="px-3 py-2 font-medium">Exposure</th>
                  <th className="px-3 py-2 font-medium">Advertise</th>
                  <th className="px-3 py-2 font-medium">Backends</th>
                  <th className="px-3 py-2 font-medium" />
                </tr>
              </thead>
              <tbody>
                {services.length === 0 && (
                  <tr>
                    <td colSpan={8} className="px-3 py-4 text-[var(--zf-muted)]">
                      No Maglev services yet.
                    </td>
                  </tr>
                )}
                {services.map((s) => (
                  <tr key={s.name} className="border-t border-[var(--zf-hairline)]">
                    <td className="px-3 py-2 font-medium text-[var(--zf-ink)]">{s.name}</td>
                    <td className="px-3 py-2 font-mono text-xs">{s.vip}</td>
                    <td className="px-3 py-2 font-mono text-xs">{s.port}</td>
                    <td className="px-3 py-2 uppercase text-xs">{s.mode ?? 'nat'}</td>
                    <td className="px-3 py-2 text-xs">{s.exposure ?? 'east-west'}</td>
                    <td className="px-3 py-2 text-xs">{s.advertise ? 'yes' : 'no'}</td>
                    <td className="px-3 py-2 font-mono text-xs">
                      {(s.backends ?? [])
                        .map((b) => `${b.state ?? 'ready'}`)
                        .join(', ') || '—'}
                      {s.snat_address ? ` · snat ${s.snat_address}` : ''}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {canWrite && (
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void removeService(s.name)}
                          className="text-red-600 hover:text-red-700"
                          aria-label={`Delete ${s.name}`}
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {tab === 'groups' && (
        <div className="space-y-4">
          {canWrite && (
            <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 flex flex-wrap gap-2 items-end">
              <div>
                <label className="block text-xs text-[var(--zf-muted)] mb-1">Name</label>
                <input
                  className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm"
                  value={groupName}
                  onChange={(e) => setGroupName(e.target.value)}
                />
              </div>
              <div>
                <label className="block text-xs text-[var(--zf-muted)] mb-1">Label</label>
                <input
                  className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm"
                  value={groupLabel}
                  onChange={(e) => setGroupLabel(e.target.value)}
                  placeholder="app=web"
                />
              </div>
              <button
                type="button"
                disabled={busy}
                onClick={() => void createGroup()}
                className="inline-flex items-center gap-1.5 px-3 py-2 text-sm rounded-lg bg-[var(--zf-link-fill)] text-white"
              >
                <Plus className="w-3.5 h-3.5" /> Save group
              </button>
            </div>
          )}
          <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
            <table className="w-full text-sm">
              <thead className="bg-[var(--zf-canvas)] text-[var(--zf-muted)] text-left">
                <tr>
                  <th className="px-3 py-2 font-medium">Name</th>
                  <th className="px-3 py-2 font-medium">Identity</th>
                  <th className="px-3 py-2 font-medium">Labels</th>
                  <th className="px-3 py-2 font-medium">Priority</th>
                  <th className="px-3 py-2 font-medium" />
                </tr>
              </thead>
              <tbody>
                {groups.length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-3 py-4 text-[var(--zf-muted)]">
                      No security groups yet.
                    </td>
                  </tr>
                )}
                {groups.map((g) => (
                  <tr key={g.name} className="border-t border-[var(--zf-hairline)]">
                    <td className="px-3 py-2 font-medium text-[var(--zf-ink)]">{g.name}</td>
                    <td className="px-3 py-2 font-mono text-xs">{g.identity}</td>
                    <td className="px-3 py-2 font-mono text-xs">{g.labels.join(', ') || '—'}</td>
                    <td className="px-3 py-2">{g.priority}</td>
                    <td className="px-3 py-2 text-right">
                      {canWrite && (
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void removeGroup(g.name)}
                          className="text-red-600 hover:text-red-700"
                          aria-label={`Delete ${g.name}`}
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {tab === 'cnp' && (
        <div className="space-y-4">
          {canWrite && (
            <div className="space-y-2">
              <TerminalTextarea
                title="CNP JSON"
                className="h-56"
                value={cnpJson}
                onChange={(e) => setCnpJson(e.target.value)}
              />
              <button
                type="button"
                disabled={busy}
                onClick={() => void applyCnp()}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg bg-[var(--zf-link-fill)] text-white"
              >
                Apply CNP
              </button>
            </div>
          )}
          <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-4 space-y-2">
            {cnps.length === 0 && <p className="text-sm text-[var(--zf-muted)]">No CNP documents.</p>}
            {cnps.map((raw, i) => {
              const doc = raw as { metadata?: { name?: string } }
              const name = doc.metadata?.name ?? `cnp-${i}`
              return (
                <div
                  key={name}
                  className="flex items-center justify-between gap-2 border-b border-[var(--zf-hairline)] last:border-0 py-2"
                >
                  <span className="text-sm font-medium text-[var(--zf-ink)]">{name}</span>
                  {canWrite && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void removeCnp(name)}
                      className="text-red-600"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      )}

      {tab === 'endpoints' && (
        <div className="space-y-2">
          <p className="text-sm text-[var(--zf-muted)]">
            FluxVM CiliumEndpoint-*shaped* views. When dataplane mode is{' '}
            <code className="text-xs">cilium</code>, identity may come from the Cilium agent
            (`identity_source=cilium-agent`); otherwise FluxVM hash. Never writes Cilium private maps.
          </p>
          <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
            <table className="w-full text-sm">
              <thead className="bg-[var(--zf-canvas)] text-[var(--zf-muted)] text-left">
                <tr>
                  <th className="px-3 py-2 font-medium">Identity</th>
                  <th className="px-3 py-2 font-medium">Source</th>
                  <th className="px-3 py-2 font-medium">UUID</th>
                  <th className="px-3 py-2 font-medium">IPv4</th>
                  <th className="px-3 py-2 font-medium">State</th>
                  <th className="px-3 py-2 font-medium">Labels</th>
                </tr>
              </thead>
              <tbody>
                {endpoints.length === 0 && (
                  <tr>
                    <td colSpan={6} className="px-3 py-4 text-[var(--zf-muted)]">
                      No VM endpoints yet. Attach a guest CIDR dataplane policy on a running VM.
                    </td>
                  </tr>
                )}
                {endpoints.map((ep) => {
                  const labels = ep['identity-labels'] ?? ep.identity_labels ?? []
                  const ipv4 =
                    ep.networking?.addressing?.find((a) => a.ipv4)?.ipv4 ?? '—'
                  const src = ep.identity_source ?? 'fluxvm-hash'
                  return (
                    <tr key={ep.uuid} className="border-t border-[var(--zf-hairline)]">
                      <td className="px-3 py-2 font-mono text-xs">{ep.identity}</td>
                      <td className="px-3 py-2">
                        <span
                          className={`px-2 py-0.5 rounded-full text-xs border ${
                            src === 'cilium-agent'
                              ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                              : 'bg-[var(--zf-canvas)] text-[var(--zf-muted)] border-[var(--zf-hairline)]'
                          }`}
                        >
                          {src}
                        </span>
                      </td>
                      <td className="px-3 py-2 font-mono text-xs truncate max-w-[10rem]" title={ep.uuid}>
                        {ep.uuid}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs">{ipv4}</td>
                      <td className="px-3 py-2">{ep.state}</td>
                      <td className="px-3 py-2 font-mono text-xs">{labels.join(', ') || '—'}</td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {tab === 'identities' && (
        <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-[var(--zf-canvas)] text-[var(--zf-muted)] text-left">
              <tr>
                <th className="px-3 py-2 font-medium">ID</th>
                <th className="px-3 py-2 font-medium">Name</th>
                <th className="px-3 py-2 font-medium">Reserved</th>
                <th className="px-3 py-2 font-medium">Labels</th>
              </tr>
            </thead>
            <tbody>
              {identities.map((id) => (
                <tr key={`${id.id}-${id.name}`} className="border-t border-[var(--zf-hairline)]">
                  <td className="px-3 py-2 font-mono text-xs">{id.id}</td>
                  <td className="px-3 py-2">{id.name}</td>
                  <td className="px-3 py-2">{id.reserved ? 'yes' : 'no'}</td>
                  <td className="px-3 py-2 font-mono text-xs">{id.labels.join(', ')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {tab === 'observe' && (
        <TerminalTextarea
          title="observe"
          className="h-96"
          value={JSON.stringify(observe ?? {}, null, 2)}
          readOnly
        />
      )}

      {tab === 'flows' && (
        <div className="space-y-2">
          <p className="text-sm text-[var(--zf-muted)]">
            Hubble-lite packet path from FluxVM (guest → tap → tc/eBPF → uplink → peer). Not Cilium Hubble gRPC.
            Use <strong>Colorful</strong> or <strong>Normal</strong>. External Hubble remains the Open Hubble link when configured.
          </p>
          <PacketFlowPanel
            views={packetFlows}
            emptyHint="No sampled flows yet. Enable sample_rate on a VM dataplane policy and generate traffic, or apply the FluxVM packet-flow PR so /v1/network/hubble/flows returns hops."
            onReload={() => {
              setLoading(true)
              void load()
            }}
          />
        </div>
      )}

      {tab === 'ipcache' && (
        <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-[var(--zf-canvas)] text-[var(--zf-muted)] text-left">
              <tr>
                <th className="px-3 py-2 font-medium">IP</th>
                <th className="px-3 py-2 font-medium">Identity</th>
                <th className="px-3 py-2 font-medium">VM ID</th>
              </tr>
            </thead>
            <tbody>
              {ipcache.length === 0 && (
                <tr>
                  <td colSpan={3} className="px-3 py-4 text-[var(--zf-muted)]">
                    No ipcache entries.
                  </td>
                </tr>
              )}
              {ipcache.map((e) => (
                <tr key={e.ip} className="border-t border-[var(--zf-hairline)]">
                  <td className="px-3 py-2 font-mono text-xs">{e.ip}</td>
                  <td className="px-3 py-2 font-mono text-xs">{e.identity}</td>
                  <td className="px-3 py-2 font-mono text-xs">{e.vm_id}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

function Cell({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-xs text-[var(--zf-muted)]">{label}</p>
      <p className="font-medium text-[var(--zf-ink)]">{value}</p>
    </div>
  )
}
