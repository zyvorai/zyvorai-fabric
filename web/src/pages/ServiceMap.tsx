// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { GitBranch, Server, ArrowRight } from 'lucide-react'
import { apiFetch } from '../api/client'
import PageLoadBanner from '../components/PageLoadBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody } from '../utils/apiError'
import { usePageLoader } from '../hooks/usePageLoader'

interface ServiceNode { name: string; type: string; vm: string; port?: number; status: string }
interface ServiceLink { from: string; to: string; protocol: string; port: number }
interface ServiceMapData { nodes: ServiceNode[]; links: ServiceLink[]; timestamp: string }

function statusDot(status: string): string {
  switch (status?.toLowerCase()) {
    case 'healthy': case 'running': return 'bg-emerald-500'
    case 'degraded': case 'warning': return 'bg-amber-500'
    case 'down': case 'error': return 'bg-red-500'
    default: return 'bg-[var(--zf-muted)]'
  }
}

function typeBadge(type: string): string {
  switch (type?.toLowerCase()) {
    case 'web': return 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    case 'database': case 'db': return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    case 'cache': return 'text-amber-800 bg-amber-50 border-amber-200'
    case 'queue': case 'mq': return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    case 'api': return 'text-emerald-700 bg-emerald-50 border-emerald-200'
    case 'proxy': case 'lb': return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    default: return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }
}

export default function ServiceMap() {
  const [data, setData] = useState<ServiceMapData | null>(null)
  const { loading, loadError, run } = usePageLoader('Failed to load service map')
    const [selectedNode, setSelectedNode] = useState<string | null>(null)

  const fetchMap = useCallback((options?: { silent?: boolean }) => {
    return run(async () => {
      const res = await apiFetch('/api/services/map')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      setData(await res.json())
    }, options)
  }, [run])

  useEffect(() => {
    void fetchMap()
    const interval = setInterval(() => void fetchMap({ silent: true }), 15000)
    return () => clearInterval(interval)
  }, [fetchMap])

  const nodes = data?.nodes || []
  const links = data?.links || []

  const selectedLinks = selectedNode ? links.filter(l => l.from === selectedNode || l.to === selectedNode) : links
  const connectedNodes = selectedNode ? new Set([selectedNode, ...selectedLinks.map(l => l.from), ...selectedLinks.map(l => l.to)]) : null

  const healthyCount = nodes.filter(n => n.status === 'healthy' || n.status === 'running').length
  const degradedCount = nodes.filter(n => n.status === 'degraded' || n.status === 'warning').length
  const downCount = nodes.filter(n => n.status === 'down' || n.status === 'error').length

  if (loading) return <div className="flex items-center justify-center h-64 text-[var(--zf-muted)]"><div className="animate-spin w-6 h-6 border-2 border-[var(--zf-ink)] border-t-transparent rounded-full mr-3" />Loading service map...</div>

  return (
    <div className="space-y-6">
      <PageLoadBanner title="Could not load service map" headline={loadError} onRetry={() => void fetchMap()} />

      <PageHeader
        title="Service Map"
        description="Service dependencies and health across VMs"
        onRefresh={() => void fetchMap()}
        refreshing={loading}
      />

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="stat-card-blue rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{nodes.length}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Services</div>
        </div>
        <div className="stat-card-green rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-green transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{healthyCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Healthy</div>
        </div>
        <div className="stat-card-orange rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{degradedCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Degraded</div>
        </div>
        <div className="stat-card-red rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{downCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Down</div>
        </div>
      </div>

      {/* Dependencies */}
      {selectedLinks.length > 0 && (
        <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] p-5">
          <h3 className="text-sm font-semibold text-[var(--zf-ink)] mb-3">
            {selectedNode ? `Dependencies for ${selectedNode}` : 'All Dependencies'}
            {selectedNode && <button onClick={() => setSelectedNode(null)} className="ml-2 text-xs text-[var(--zf-link)] hover:text-[var(--zf-link-hover)]">Show all</button>}
          </h3>
          <div className="space-y-2">
            {selectedLinks.map((link, idx) => (
              <div key={idx} className="flex items-center gap-3 px-3 py-2 bg-[var(--zf-canvas)] rounded-lg">
                <span className="text-sm text-[var(--zf-ink)] font-medium">{link.from}</span>
                <ArrowRight className="w-4 h-4 text-[var(--zf-muted)]" />
                <span className="text-sm text-[var(--zf-ink)] font-medium">{link.to}</span>
                <span className="text-xs text-[var(--zf-muted)] ml-auto">{link.protocol}:{link.port}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Service Nodes */}
      {nodes.length === 0 ? (
        <div className="bg-[var(--zf-surface)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)]"><GitBranch className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">No services discovered</p></div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {nodes.map(node => {
            const isSelected = selectedNode === node.name
            const isConnected = connectedNodes ? connectedNodes.has(node.name) : true
            const inLinks = links.filter(l => l.to === node.name).length
            const outLinks = links.filter(l => l.from === node.name).length
            return (
              <div key={node.name} onClick={() => setSelectedNode(isSelected ? null : node.name)}
                className={`bg-[var(--zf-surface)] rounded-xl border p-4 cursor-pointer transition-all hover:scale-[1.01] ${isSelected ? 'border-[var(--zf-ink)] ring-1 ring-black/[0.06]' : isConnected ? 'border-[var(--zf-hairline)]' : 'border-[var(--zf-hairline)] opacity-40'}`}>
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <span className={`w-2.5 h-2.5 rounded-full ${statusDot(node.status)}`} />
                    <span className="text-sm font-semibold text-[var(--zf-ink)]">{node.name}</span>
                  </div>
                  <span className={`px-2 py-0.5 rounded-full text-xs font-medium border capitalize ${typeBadge(node.type)}`}>{node.type}</span>
                </div>
                <div className="flex items-center gap-2 text-xs text-[var(--zf-muted)] mb-2">
                  <Server className="w-3.5 h-3.5" /><span>{node.vm}</span>
                  {node.port && <><span className="text-[var(--zf-hairline)]">|</span><span>:{node.port}</span></>}
                </div>
                <div className="flex items-center gap-3 text-xs">
                  <span className="text-[var(--zf-muted)]">{inLinks} inbound</span>
                  <span className="text-[var(--zf-muted)]">{outLinks} outbound</span>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
