// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import { Bot, Cpu, KeyRound, Layers, Radio, Server } from 'lucide-react'
import {
  FabricGpuView,
  InferenceApiKey,
  InferenceDeployment,
  InferenceEndpoint,
  InferenceNode,
  InferenceProfile,
  ModelArtifact,
  listCapacity,
  listDeployments,
  listEndpoints,
  listGpus,
  listKeys,
  listModels,
  listNodes,
  listProfiles,
} from '../api/ai'
import { PageHeader, EmptyState, Card } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'

type Tab = 'models' | 'deployments' | 'endpoints' | 'keys' | 'gpus' | 'nodes'

export default function AiWorkloads() {
  const [tab, setTab] = useState<Tab>('deployments')
  const [models, setModels] = useState<ModelArtifact[]>([])
  const [profiles, setProfiles] = useState<InferenceProfile[]>([])
  const [deployments, setDeployments] = useState<InferenceDeployment[]>([])
  const [endpoints, setEndpoints] = useState<InferenceEndpoint[]>([])
  const [keys, setKeys] = useState<InferenceApiKey[]>([])
  const [gpus, setGpus] = useState<FabricGpuView[]>([])
  const [nodes, setNodes] = useState<InferenceNode[]>([])
  const [capacity, setCapacity] = useState<Record<string, unknown> | null>(null)
  const { loading, loadError, run } = usePageLoader('Failed to load AI workloads')

  const load = useCallback(() => {
    return run(async () => {
      const [m, p, d, e, k, g, n, c] = await Promise.all([
        listModels(),
        listProfiles(),
        listDeployments(),
        listEndpoints(),
        listKeys(),
        listGpus(),
        listNodes(),
        listCapacity(),
      ])
      setModels(Array.isArray(m) ? m : [])
      setProfiles(Array.isArray(p) ? p : [])
      setDeployments(Array.isArray(d) ? d : [])
      setEndpoints(Array.isArray(e) ? e : [])
      setKeys(Array.isArray(k) ? k : [])
      const gItems = Array.isArray(g) ? g : (g as { items?: FabricGpuView[] }).items ?? []
      setGpus(gItems)
      const nItems = Array.isArray(n) ? n : (n as { items?: InferenceNode[] }).items ?? []
      setNodes(nItems)
      setCapacity(c && typeof c === 'object' ? (c as Record<string, unknown>) : null)
    })
  }, [run])

  useEffect(() => {
    void load()
  }, [load])

  const tabs: { id: Tab; label: string; icon: typeof Bot }[] = [
    { id: 'models', label: 'Models', icon: Layers },
    { id: 'deployments', label: 'Deployments', icon: Bot },
    { id: 'endpoints', label: 'Endpoints', icon: Radio },
    { id: 'keys', label: 'API keys', icon: KeyRound },
    { id: 'nodes', label: 'Nodes', icon: Server },
    { id: 'gpus', label: 'Host GPUs', icon: Cpu },
  ]

  const gpusSummary = capacity?.gpus as { total?: number; free?: number; allocated?: number } | undefined
  const nodeGpuRows = nodes.flatMap((node) =>
    (node.gpus ?? []).map((gpu) => ({
      key: `${node.id}:${gpu.bdf}`,
      node: node.id,
      site: node.site ?? '—',
      state: node.state ?? '—',
      bdf: gpu.bdf,
      model: gpu.model ?? '—',
      vram: gpu.vram_gib,
      mig: gpu.mig_profile || (gpu.parent_bdf ? 'slice' : ''),
      parent: gpu.parent_bdf || '',
      healthy: gpu.healthy !== false,
      janus: (gpu.bdf ?? '').startsWith('janus:') || (gpu.model ?? '').startsWith('janus:'),
    })),
  )

  return (
    <div>
      <PageHeader
        title="AI Workloads"
        description="Preview: vLLM on NVIDIA GPU VMs, Maglev endpoints, optional Janus lab upstream, and an API-key OpenAI gateway."
        onRefresh={() => void load()}
        refreshing={loading}
      />
      <PageLoadBanner title="Could not load AI workloads" headline={loadError} onRetry={() => void load()} />

      {gpusSummary && (
        <p className="mb-3 text-xs text-[var(--zf-muted)]">
          Capacity: {gpusSummary.free ?? '—'} free / {gpusSummary.total ?? '—'} GPUs
          {capacity?.dry_run ? ' · dry-run' : ''}
          {typeof capacity?.site === 'string' ? ` · site ${capacity.site}` : ''}
          {nodes.length > 0 ? ` · ${nodes.length} inference node(s)` : ''}
        </p>
      )}

      <div className="flex gap-2 mb-4 flex-wrap">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`zf-btn zf-btn-sm ${tab === t.id ? 'zf-btn-primary' : 'zf-btn-ghost'}`}
            onClick={() => setTab(t.id)}
          >
            <t.icon className="w-3.5 h-3.5" /> {t.label}
          </button>
        ))}
      </div>

      {tab === 'models' && (
        models.length === 0 ? (
          <EmptyState icon={<Layers className="w-8 h-8" />} title="No model artifacts" description="fabricctl ai model add NAME --source hf://org/model" />
        ) : (
          <Card className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                  <th className="px-4 py-3">Name</th>
                  <th className="px-4 py-3">Source</th>
                  <th className="px-4 py-3">Format</th>
                  <th className="px-4 py-3">Residency</th>
                  <th className="px-4 py-3">Local path</th>
                </tr>
              </thead>
              <tbody>
                {models.map((m) => (
                  <tr key={m.name} className="border-b border-[var(--zf-hairline)]">
                    <td className="px-4 py-3 font-medium">{m.name}</td>
                    <td className="px-4 py-3 font-mono text-xs">{m.source}</td>
                    <td className="px-4 py-3">{m.format}</td>
                    <td className="px-4 py-3">{m.residency ?? '—'}</td>
                    <td className="px-4 py-3 font-mono text-xs">{m.local_path ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        )
      )}

      {tab === 'deployments' && (
        deployments.length === 0 ? (
          <EmptyState icon={<Bot className="w-8 h-8" />} title="No inference deployments" description="fabricctl ai deploy MODEL --runtime vllm --gpu 1 --replicas 1" />
        ) : (
          <Card className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                  <th className="px-4 py-3">Name</th>
                  <th className="px-4 py-3">Model</th>
                  <th className="px-4 py-3">Replicas</th>
                  <th className="px-4 py-3">Autoscale</th>
                  <th className="px-4 py-3">Phase</th>
                  <th className="px-4 py-3">Weights</th>
                </tr>
              </thead>
              <tbody>
                {deployments.map((d) => (
                  <tr key={d.name} className="border-b border-[var(--zf-hairline)]">
                    <td className="px-4 py-3 font-medium">{d.name}</td>
                    <td className="px-4 py-3">{d.model}</td>
                    <td className="px-4 py-3">{d.status.replicas?.filter((r) => r.ready).length ?? 0}/{d.replicas}</td>
                    <td className="px-4 py-3 text-xs">
                      {d.autoscaling?.enabled
                        ? `${d.autoscaling.min_replicas}–${d.autoscaling.max_replicas}`
                        : 'off'}
                    </td>
                    <td className="px-4 py-3">{d.status.phase || '—'}</td>
                    <td className="px-4 py-3 font-mono text-xs">
                      {(d.status.replicas ?? [])
                        .map((r) => r.maglev_weight ?? '—')
                        .join(', ') || '—'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        )
      )}

      {tab === 'endpoints' && (
        endpoints.length === 0 ? (
          <EmptyState icon={<Radio className="w-8 h-8" />} title="No endpoints" description="fabricctl ai endpoint expose DEPLOYMENT --openai-compatible" />
        ) : (
          <Card className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                  <th className="px-4 py-3">Name</th>
                  <th className="px-4 py-3">Deployment</th>
                  <th className="px-4 py-3">Routing</th>
                  <th className="px-4 py-3">VIP</th>
                  <th className="px-4 py-3">Gateway</th>
                </tr>
              </thead>
              <tbody>
                {endpoints.map((ep) => (
                  <tr key={ep.name} className="border-b border-[var(--zf-hairline)]">
                    <td className="px-4 py-3 font-medium">{ep.name}</td>
                    <td className="px-4 py-3">{ep.deployment}</td>
                    <td className="px-4 py-3 font-mono text-xs">{ep.routing_strategy ?? '—'}</td>
                    <td className="px-4 py-3 font-mono text-xs">{ep.vip ?? '—'}:{ep.port}</td>
                    <td className="px-4 py-3 font-mono text-xs">/api/ai/openai/{ep.name}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        )
      )}

      {tab === 'keys' && (
        keys.length === 0 ? (
          <EmptyState icon={<KeyRound className="w-8 h-8" />} title="No API keys" description="fabricctl ai key create NAME --endpoint ENDPOINT" />
        ) : (
          <Card className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                  <th className="px-4 py-3">Name</th>
                  <th className="px-4 py-3">Endpoint</th>
                  <th className="px-4 py-3">Prefix</th>
                  <th className="px-4 py-3">Used</th>
                  <th className="px-4 py-3">Quota</th>
                </tr>
              </thead>
              <tbody>
                {keys.map((k) => (
                  <tr key={k.id} className="border-b border-[var(--zf-hairline)]">
                    <td className="px-4 py-3 font-medium">{k.name}</td>
                    <td className="px-4 py-3">{k.endpoint}</td>
                    <td className="px-4 py-3 font-mono text-xs">{k.prefix}…</td>
                    <td className="px-4 py-3">{k.requests_used}</td>
                    <td className="px-4 py-3">{k.request_quota ?? '∞'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        )
      )}

      {tab === 'nodes' && (
        nodeGpuRows.length === 0 ? (
          <EmptyState
            icon={<Server className="w-8 h-8" />}
            title="No inference nodes"
            description="Set FLUXVM_AI_JANUS_URL for a lab GPU stand-in, or POST /api/ai/nodes for a real host."
          />
        ) : (
          <Card className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                  <th className="px-4 py-3">Node</th>
                  <th className="px-4 py-3">Site</th>
                  <th className="px-4 py-3">State</th>
                  <th className="px-4 py-3">BDF</th>
                  <th className="px-4 py-3">Model</th>
                  <th className="px-4 py-3">VRAM</th>
                  <th className="px-4 py-3">MIG</th>
                  <th className="px-4 py-3">Source</th>
                </tr>
              </thead>
              <tbody>
                {nodeGpuRows.map((row) => (
                  <tr key={row.key} className="border-b border-[var(--zf-hairline)]">
                    <td className="px-4 py-3 font-medium">{row.node}</td>
                    <td className="px-4 py-3">{row.site}</td>
                    <td className="px-4 py-3">{row.state}</td>
                    <td className="px-4 py-3 font-mono text-xs">{row.bdf}</td>
                    <td className="px-4 py-3 font-mono text-xs">{row.model}</td>
                    <td className="px-4 py-3">{row.vram != null ? `${row.vram} GiB` : '—'}</td>
                    <td className="px-4 py-3 font-mono text-xs">{row.mig || '—'}</td>
                    <td className="px-4 py-3">{row.janus ? 'janus' : row.healthy ? 'pci' : 'unhealthy'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        )
      )}

      {tab === 'gpus' && (
        gpus.length === 0 ? (
          <EmptyState icon={<Cpu className="w-8 h-8" />} title="No host GPUs" description="FluxVM inventory empty — set FLUXVM_AI_JANUS_URL or FLUXVM_AI_DRY_RUN=1 for lab paths without NVIDIA." />
        ) : (
          <Card className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                  <th className="px-4 py-3">BDF</th>
                  <th className="px-4 py-3">Vendor</th>
                  <th className="px-4 py-3">Driver</th>
                  <th className="px-4 py-3">VRAM</th>
                  <th className="px-4 py-3">VFIO</th>
                  <th className="px-4 py-3">Held</th>
                  <th className="px-4 py-3">Allocated</th>
                </tr>
              </thead>
              <tbody>
                {gpus.map((g) => (
                  <tr key={g.bdf} className="border-b border-[var(--zf-hairline)]">
                    <td className="px-4 py-3 font-mono text-xs">{g.bdf}</td>
                    <td className="px-4 py-3">{g.vendor}</td>
                    <td className="px-4 py-3">{g.driver ?? '—'}</td>
                    <td className="px-4 py-3">{g.vram_gib != null ? `${g.vram_gib} GiB` : '—'}</td>
                    <td className="px-4 py-3">{g.group_bound_to_vfio ? 'yes' : 'no'}</td>
                    <td className="px-4 py-3">{g.group_held ? 'yes' : 'no'}</td>
                    <td className="px-4 py-3 text-xs">{g.allocated_to?.deployment ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        )
      )}

      {profiles.length > 0 && tab === 'deployments' && (
        <p className="mt-3 text-xs text-[var(--zf-muted)]">{profiles.length} profile(s) registered</p>
      )}
    </div>
  )
}
