// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback, useMemo } from 'react'
import { Network, Server } from 'lucide-react'
import { apiFetch } from '../api/client'
import { listBridges, listLinks, type BridgeConfig, type LinkInfo } from '../api/networkd'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, EmptyState, Card } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'
import { useSilentPoll } from '../hooks/useSilentPoll'

interface VMInterface {
  type: string
  source: string
  model: string
  mac: string
}

interface NetworkVM {
  name: string
  state: string
  interfaces: VMInterface[]
}

interface NetworkInfo {
  name: string
  state: string
  autostart: string
}

interface HostNode {
  name: string
  kind: string
  operational_state: string
  addresses: string[]
  isBridge: boolean
}

export default function NetworkTopology() {
  const toast = useToastContext()
  const [vms, setVMs] = useState<NetworkVM[]>([])
  const [networks, setNetworks] = useState<NetworkInfo[]>([])
  const [bridges, setBridges] = useState<BridgeConfig[]>([])
  const [links, setLinks] = useState<LinkInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const fetchTopology = useCallback(async (silent = false) => {
    if (!silent) {
      setLoading(true)
      setLoadError(null)
    }
    try {
      const [topoResp, bridgeList, linkList] = await Promise.all([
        apiFetch('/api/network/topology'),
        listBridges().catch(() => [] as BridgeConfig[]),
        listLinks().catch(() => [] as LinkInfo[]),
      ])
      if (!topoResp.ok) {
        const body = await topoResp.text()
        throw new Error(formatHttpErrorBody(topoResp.status, topoResp.statusText, body))
      }
      const data = await topoResp.json()
      setVMs(data.vms || [])
      setNetworks(data.networks || [])
      setBridges(bridgeList)
      setLinks(linkList)
    } catch (err) {
      if (!silent) {
        const msg = formatUserError(err)
        setLoadError(msg)
        toastFailure(toast, 'Failed to load network topology', err)
      }
    } finally {
      if (!silent) setLoading(false)
    }
  }, [toast])

  useEffect(() => {
    void fetchTopology()
  }, [fetchTopology])
  useSilentPoll(() => fetchTopology(true), 15000)

  const hostNodes = useMemo((): HostNode[] => {
    const bridgeNames = new Set(bridges.map(b => b.name))
    const nodes: HostNode[] = []

    for (const b of bridges) {
      nodes.push({
        name: b.name,
        kind: 'bridge',
        operational_state: b.operational_state ?? 'unknown',
        addresses: b.addresses,
        isBridge: true,
      })
    }

    for (const l of links) {
      if (bridgeNames.has(l.name)) continue
      const kind = l.kind?.toLowerCase() ?? ''
      const isPhysical = kind === 'ether' || kind === '' || l.name.match(/^(eno|eth|enp|ens)/)
      if (!isPhysical) continue
      nodes.push({
        name: l.name,
        kind: l.kind || 'interface',
        operational_state: l.operational_state,
        addresses: [],
        isBridge: false,
      })
    }

    return nodes.sort((a, b) => a.name.localeCompare(b.name))
  }, [bridges, links])

  const vmsByNetwork = useMemo(() => {
    const grouped: Record<string, NetworkVM[]> = {}
    const allNetNames = new Set([
      ...networks.map(n => n.name),
      ...bridges.map(b => b.name),
    ])
    for (const name of allNetNames) grouped[name] = []

    for (const vm of vms) {
      const assigned = new Set<string>()
      if (vm.interfaces) {
        for (const iface of vm.interfaces) {
          if (iface.source) {
            if (!grouped[iface.source]) grouped[iface.source] = []
            if (!assigned.has(iface.source)) {
              grouped[iface.source].push(vm)
              assigned.add(iface.source)
            }
          }
        }
      }
      if (assigned.size === 0) {
        if (!grouped['unattached']) grouped['unattached'] = []
        grouped['unattached'].push(vm)
      }
    }
    return grouped
  }, [vms, networks, bridges])

  const networkNames = Object.keys(vmsByNetwork).sort((a, b) => {
    if (a === 'unattached') return 1
    if (b === 'unattached') return -1
    return a.localeCompare(b)
  })

  const attachmentEdges = useMemo(() => {
    const edges: { vm: string; vmState: string; network: string; ifaceType: string; mac: string }[] = []
    for (const vm of vms) {
      for (const iface of vm.interfaces ?? []) {
        if (iface.source) {
          edges.push({
            vm: vm.name,
            vmState: vm.state,
            network: iface.source,
            ifaceType: iface.type,
            mac: iface.mac || '—',
          })
        }
      }
    }
    return edges.sort((a, b) => a.vm.localeCompare(b.vm) || a.network.localeCompare(b.network))
  }, [vms])

  const hasContent = vms.length > 0 || networks.length > 0 || hostNodes.length > 0

  return (
    <div className="space-y-6">
      <PageHeader
        title="Network Topology"
        description="VMs grouped by network, plus host bridges and physical NICs"
        onRefresh={fetchTopology}
        refreshing={loading}
      />

      {loadError && (
        <ErrorBanner
          title="Could not load network topology"
          headline={loadError}
          hints={hintsForError(loadError, 'network')}
          onRetry={fetchTopology}
        />
      )}

      {loading && !loadError ? (
        <Card>
          <div className="p-10 flex flex-col items-center justify-center text-[var(--zf-muted)] gap-3">
            <div className="w-6 h-6 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-ink)] rounded-full animate-spin" />
            <span className="text-sm">Loading network topology…</span>
          </div>
        </Card>
      ) : !loadError && !hasContent ? (
        <Card>
          <EmptyState
            icon={<Network className="w-10 h-10" />}
            title="No networks or VMs found"
            description="Configure bridges and attach VM interfaces to see topology"
          />
        </Card>
      ) : !loadError ? (
        <div className="flex flex-col gap-6">
          <div className="flex items-center justify-between text-xs text-[var(--zf-muted)]">
            <span>
              {networks.length} virtual network{networks.length !== 1 ? 's' : ''} · {bridges.length} host bridge
              {bridges.length !== 1 ? 's' : ''} · {vms.length} VM{vms.length !== 1 ? 's' : ''}
            </span>
          </div>

          {hostNodes.length > 0 && (
            <section>
              <h2 className="text-sm font-semibold text-[var(--zf-ink)] mb-3 flex items-center gap-2">
                <Server className="w-4 h-4" />
                Host interfaces
              </h2>
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                {hostNodes.map(node => (
                  <div
                    key={node.name}
                    className="zf-panel p-3"
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-medium text-[var(--zf-ink)]">{node.name}</span>
                      <span className={`text-xs px-2 py-0.5 rounded ${
                        node.isBridge ? 'bg-[var(--zf-link)]/10 text-[var(--zf-link)]' : 'bg-black/[0.04] text-[var(--zf-muted)]'
                      }`}>
                        {node.isBridge ? 'bridge' : node.kind}
                      </span>
                    </div>
                    <div className="text-xs text-[var(--zf-muted)] mt-1">{node.operational_state}</div>
                    {node.addresses.length > 0 && (
                      <div className="text-xs font-mono text-[var(--zf-muted)] mt-1">{node.addresses.join(', ')}</div>
                    )}
                  </div>
                ))}
              </div>
            </section>
          )}

          {attachmentEdges.length > 0 && (
            <section>
              <h2 className="text-sm font-semibold text-[var(--zf-ink)] mb-3">VM → network connections</h2>
              <Card className="overflow-hidden">
                <table className="w-full text-sm">
                  <thead className="bg-[var(--zf-surface)] text-[var(--zf-muted)] text-xs">
                    <tr>
                      <th className="text-left p-3 font-medium">VM</th>
                      <th className="text-left p-3 font-medium w-8" />
                      <th className="text-left p-3 font-medium">Network / bridge</th>
                      <th className="text-left p-3 font-medium">Type</th>
                      <th className="text-left p-3 font-medium">MAC</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-[var(--zf-hairline)]">
                    {attachmentEdges.map((e, i) => (
                      <tr key={`${e.vm}-${e.network}-${i}`} className="hover:bg-black/[0.03]">
                        <td className="p-3">
                          <span className="font-medium text-[var(--zf-ink)]">{e.vm}</span>
                          <span className="text-xs text-[var(--zf-muted)] ml-2">{e.vmState}</span>
                        </td>
                        <td className="p-3 text-[var(--zf-muted)]">→</td>
                        <td className="p-3 font-medium text-[var(--zf-link)]">{e.network}</td>
                        <td className="p-3 text-[var(--zf-muted)]">{e.ifaceType}</td>
                        <td className="p-3 font-mono text-xs text-[var(--zf-muted)]">{e.mac}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </Card>
            </section>
          )}

          <section>
            <h2 className="text-sm font-semibold text-[var(--zf-ink)] mb-3">VM attachments</h2>
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
              {networkNames.map(netName => {
                const netVMs = vmsByNetwork[netName] || []
                const netInfo = networks.find(n => n.name === netName)
                const hostBridge = bridges.find(b => b.name === netName)
                return (
                  <div key={netName} className="zf-panel p-4">
                    <div className="flex items-center justify-between mb-3">
                      <h3 className="text-sm font-semibold text-[var(--zf-ink)] capitalize">
                        {netName === 'unattached' ? 'Unattached VMs' : netName}
                      </h3>
                      {netInfo && (
                        <span className="text-xs text-[var(--zf-muted)]">
                          {netInfo.state} · autostart {netInfo.autostart}
                        </span>
                      )}
                      {!netInfo && hostBridge && (
                        <span className="text-xs text-[var(--zf-link)]">host bridge · {hostBridge.operational_state ?? '—'}</span>
                      )}
                    </div>
                    {netVMs.length === 0 ? (
                      <p className="text-xs text-[var(--zf-muted)]">No VMs on this network</p>
                    ) : (
                      <ul className="space-y-2">
                        {netVMs.map(vm => (
                          <li
                            key={vm.name}
                            className="text-sm bg-[var(--zf-canvas)] rounded-lg px-3 py-2 border border-[var(--zf-hairline)]/60"
                          >
                            <div className="font-medium text-[var(--zf-ink)]">{vm.name}</div>
                            <div className="text-xs text-[var(--zf-muted)] mt-0.5">{vm.state}</div>
                            {vm.interfaces?.length > 0 && (
                              <div className="mt-2 space-y-1">
                                {vm.interfaces.map((iface, i) => (
                                  <div key={i} className="text-xs text-[var(--zf-muted)] font-mono">
                                    {iface.type} → {iface.source || '—'} · {iface.model} · {iface.mac || 'no MAC'}
                                  </div>
                                ))}
                              </div>
                            )}
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                )
              })}
            </div>
          </section>
        </div>
      ) : null}
    </div>
  )
}
