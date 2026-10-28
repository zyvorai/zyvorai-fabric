// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { Plus, Trash2, RefreshCw, Pencil } from 'lucide-react'
import * as api from '../../api/network-security'
import type { VpnTunnel, CreateVpnTunnelRequest, VpnNetwork, CreateVpnNetworkRequest, VpnTopology } from '../../api/network-security'
import { ModalWrapper, InputField, HostBadge, HostManagedActions, isHostManaged, extractErrorMessage } from '../network/ModalShared'
import { LabelSelectorInput, LabelTags, StatusBadge } from './ModalShared'
import { useReadOnly } from '../../contexts/ReadOnlyContext'

interface VpnTabProps {
  tunnels: VpnTunnel[]
  networks: VpnNetwork[]
  onDeleteTunnel: (id: string) => void
  onDeleteNetwork: (id: string) => void
  onAdoptTunnel?: (id: string) => void
  onEditTunnel?: (id: string) => void
  onCreate: () => void
  onSync: () => void
}

function VpnTabContent({ tunnels, networks, onDeleteTunnel, onDeleteNetwork, onAdoptTunnel, onEditTunnel, onCreate, onSync }: VpnTabProps) {
  const readOnly = useReadOnly()
  const [view, setView] = useState<'tunnels' | 'networks'>('tunnels')
  const [status, setStatus] = useState<{ active_tunnels: number; networks: number } | null>(null)

  const refreshStatus = () => { api.vpnStatus().then(setStatus).catch(() => {}) }
  useEffect(() => { refreshStatus() }, [])
  const handleSyncClick = async () => { await onSync(); refreshStatus() }

  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
      <div className="p-6 border-b border-[var(--zf-hairline)] flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h2 className="text-xl font-semibold">VPN Mesh</h2>
          <div className="flex bg-[var(--zf-surface)] rounded-lg p-0.5">
            {(['tunnels', 'networks'] as const).map(v => (
              <button key={v} onClick={() => setView(v)} className={`px-3 py-1 rounded text-sm transition ${view === v ? 'bg-[var(--zf-canvas-alt)] text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'}`}>
                {v.charAt(0).toUpperCase() + v.slice(1)}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {status && (
            <span className="text-xs text-[var(--zf-muted)] bg-[var(--zf-surface)] rounded-lg px-3 py-1.5 border border-[var(--zf-hairline)]">
              {status.active_tunnels} active &middot; {status.networks} networks
            </span>
          )}
          {!readOnly && <button onClick={handleSyncClick} className="flex items-center gap-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] text-[var(--zf-ink)] py-2 px-4 rounded-lg transition text-sm">
            <RefreshCw className="w-4 h-4" /> Sync
          </button>}
          {!readOnly && <button onClick={onCreate} className="flex items-center gap-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white py-2 px-4 rounded-lg transition text-sm">
            <Plus className="w-4 h-4" /> Add {view === 'tunnels' ? 'Tunnel' : 'Network'}
          </button>}
        </div>
      </div>

      {view === 'tunnels' && (
        tunnels.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No VPN tunnels configured. Create a WireGuard tunnel to connect VMs.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Interface</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Port</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Peers</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Key</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {tunnels.map(t => (
                  <tr key={t.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4">
                      <div className="font-medium">{t.name}{isHostManaged(t) && <HostBadge />}</div>
                      {t.description && <div className="text-xs text-[var(--zf-muted)] mt-1">{t.description}</div>}
                    </td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-link)]">{t.interface_name}</td>
                    <td className="p-4 font-mono text-sm">{t.listen_port}</td>
                    <td className="p-4 font-medium text-cyan-400">{t.peers.length}</td>
                    <td className="p-4">
                      <StatusBadge
                        status={(t.private_key_set ?? Boolean(t.private_key_ref)) ? 'set' : 'missing'}
                        color={(t.private_key_set ?? Boolean(t.private_key_ref)) ? 'green' : 'red'}
                      />
                    </td>
                    <td className="p-4">
                      <StatusBadge status={t.enabled ? 'active' : 'disabled'} color={t.enabled ? 'green' : 'gray'} />
                    </td>
                    <td className="p-4">
                      <div className="flex items-center gap-1">
                        {!readOnly && !isHostManaged(t) && onEditTunnel && (
                          <button onClick={() => onEditTunnel(t.id)} className="p-2 hover:bg-[var(--zf-hairline)] rounded transition" title="Edit">
                            <Pencil className="w-4 h-4" />
                          </button>
                        )}
                        <HostManagedActions readOnly={readOnly}
                          item={{ id: t.id, managed: t.managed }}
                          onDelete={() => onDeleteTunnel(t.id)}
                          onAdopt={onAdoptTunnel ? () => onAdoptTunnel(t.id) : undefined}
                        />
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {view === 'networks' && (
        networks.length === 0 ? (
          <div className="p-12 text-center text-[var(--zf-muted)]">No VPN networks configured.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-[var(--zf-surface)]">
                <tr>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Name</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Topology</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Address Range</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Labels</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Tunnels</th>
                  <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {networks.map(n => (
                  <tr key={n.id} className="hover:bg-white/[0.03] transition">
                    <td className="p-4 font-medium">{n.name}</td>
                    <td className="p-4">
                      <StatusBadge status={n.topology} color="blue" />
                    </td>
                    <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{n.address_range}</td>
                    <td className="p-4"><LabelTags labels={n.labels} /></td>
                    <td className="p-4 font-medium text-cyan-400">{n.tunnel_ids.length}</td>
                    <td className="p-4">
                      <button onClick={() => onDeleteNetwork(n.id)} className="p-2 hover:bg-red-600 rounded transition" aria-label={`Delete ${n.name}`} title={`Delete ${n.name}`}>
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}
    </div>
  )
}

export function CreateVpnTunnelModal({ onClose, onCreated }: { onClose: () => void; onCreated: (t: VpnTunnel) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [interfaceName, setInterfaceName] = useState('')
  const [listenPort, setListenPort] = useState('51820')
  const [address, setAddress] = useState('')
  const [privateKey, setPrivateKey] = useState('')
  const [peerKey, setPeerKey] = useState('')
  const [peerEndpoint, setPeerEndpoint] = useState('')
  const [peerAllowedIps, setPeerAllowedIps] = useState('')
  const [peers, setPeers] = useState<{ public_key: string; endpoint?: string; allowed_ips: string[] }[]>([])
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const addPeer = () => {
    if (!peerKey.trim()) return
    setPeers(prev => [...prev, {
      public_key: peerKey.trim(),
      endpoint: peerEndpoint.trim() || undefined,
      allowed_ips: peerAllowedIps.trim() ? peerAllowedIps.split(',').map(s => s.trim()) : [],
    }])
    setPeerKey('')
    setPeerEndpoint('')
    setPeerAllowedIps('')
  }

  const handleSubmit = async () => {
    if (!name.trim() || !interfaceName.trim() || !address.trim() || !privateKey.trim()) {
      setErr('Name, interface name, address, and private key are required')
      return
    }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateVpnTunnelRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        interface_name: interfaceName.trim(),
        listen_port: parseInt(listenPort) || 51820,
        address: address.trim(),
        private_key_ref: privateKey.trim(),
        peers: peers.length > 0 ? peers : undefined,
      }
      const t = await api.createVpnTunnel(req)
      onCreated(t)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create VPN Tunnel" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="wg-site-a" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Site A WireGuard tunnel" />
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Interface Name" value={interfaceName} onChange={setInterfaceName} placeholder="wg0" />
          <InputField label="Listen Port" value={listenPort} onChange={setListenPort} placeholder="51820" type="number" />
        </div>
        <InputField label="Address" value={address} onChange={setAddress} placeholder="10.10.0.1/24" />
        <InputField label="Private Key" value={privateKey} onChange={setPrivateKey} placeholder="Base64 private key" />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Peer</div>
          <InputField label="Public Key" value={peerKey} onChange={setPeerKey} placeholder="Base64 public key" />
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Endpoint" value={peerEndpoint} onChange={setPeerEndpoint} placeholder="1.2.3.4:51820" />
            <InputField label="Allowed IPs (comma-separated)" value={peerAllowedIps} onChange={setPeerAllowedIps} placeholder="10.0.0.0/24" />
          </div>
          <button type="button" onClick={addPeer} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Peer
          </button>
          {peers.length > 0 && (
            <div className="space-y-1 mt-2">
              {peers.map((p, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <span className="text-[var(--zf-ink)] truncate">{p.public_key.slice(0, 20)}...</span>
                  {p.endpoint && <span className="text-[var(--zf-muted)]">{p.endpoint}</span>}
                  <button onClick={() => setPeers(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Tunnel'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function CreateVpnNetworkModal({ onClose, onCreated }: { onClose: () => void; onCreated: (n: VpnNetwork) => void }) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [topology, setTopology] = useState<VpnTopology>('full-mesh')
  const [addressRange, setAddressRange] = useState('')
  const [labels, setLabels] = useState<Record<string, string>>({})
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  const handleSubmit = async () => {
    if (!name.trim() || !addressRange.trim()) { setErr('Name and address range are required'); return }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateVpnNetworkRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        topology,
        address_range: addressRange.trim(),
        labels: Object.keys(labels).length > 0 ? labels : undefined,
      }
      const n = await api.createVpnNetwork(req)
      onCreated(n)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <ModalWrapper title="Create VPN Network" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="site-network" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Multi-site VPN network" />
        <div>
          <label className="block text-sm font-medium text-[var(--zf-ink)] mb-1">Topology</label>
          <select aria-label="Topology" value={topology} onChange={e => setTopology(e.target.value as VpnTopology)} className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500">
            <option value="full-mesh">Full Mesh</option>
            <option value="hub-spoke">Hub & Spoke</option>
            <option value="point-to-point">Point to Point</option>
          </select>
        </div>
        <InputField label="Address Range" value={addressRange} onChange={setAddressRange} placeholder="10.10.0.0/16" />
        <LabelSelectorInput labels={labels} onChange={setLabels} />
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Creating...' : 'Create Network'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export function EditVpnTunnelModal({ id, onClose, onUpdated }: { id: string; onClose: () => void; onUpdated: (t: VpnTunnel) => void }) {
  const [loading, setLoading] = useState(true)
  const [loadErr, setLoadErr] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [interfaceName, setInterfaceName] = useState('')
  const [listenPort, setListenPort] = useState('51820')
  const [address, setAddress] = useState('')
  const [privateKey, setPrivateKey] = useState('')
  const [peerKey, setPeerKey] = useState('')
  const [peerEndpoint, setPeerEndpoint] = useState('')
  const [peerAllowedIps, setPeerAllowedIps] = useState('')
  const [peers, setPeers] = useState<{ public_key: string; endpoint?: string; allowed_ips: string[] }[]>([])
  const [submitting, setSubmitting] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    let cancelled = false
    api.getVpnTunnel(id).then(t => {
      if (cancelled) return
      setName(t.name)
      setDescription(t.description ?? '')
      setInterfaceName(t.interface_name)
      setListenPort(String(t.listen_port))
      setAddress(t.address ?? '')
      setPeers(t.peers)
      setLoading(false)
    }).catch((e: unknown) => {
      if (cancelled) return
      setLoadErr(extractErrorMessage(e))
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [id])

  const addPeer = () => {
    if (!peerKey.trim()) return
    setPeers(prev => [...prev, {
      public_key: peerKey.trim(),
      endpoint: peerEndpoint.trim() || undefined,
      allowed_ips: peerAllowedIps.trim() ? peerAllowedIps.split(',').map(s => s.trim()) : [],
    }])
    setPeerKey('')
    setPeerEndpoint('')
    setPeerAllowedIps('')
  }

  const handleSubmit = async () => {
    if (!name.trim() || !interfaceName.trim() || !address.trim()) {
      setErr('Name, interface name, and address are required')
      return
    }
    setSubmitting(true)
    setErr('')
    try {
      const req: CreateVpnTunnelRequest = {
        name: name.trim(),
        description: description.trim() || undefined,
        interface_name: interfaceName.trim(),
        listen_port: parseInt(listenPort) || 51820,
        address: address.trim(),
        // Blank means "keep the existing key" -- the backend preserves it
        // server-side since GET redacts the real key, so the client can
        // never legitimately resend it.
        private_key_ref: privateKey.trim(),
        peers: peers.length > 0 ? peers : undefined,
      }
      const t = await api.updateVpnTunnel(id, req)
      onUpdated(t)
    } catch (e: unknown) {
      setErr(extractErrorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  if (loading) {
    return (
      <ModalWrapper title="Edit VPN Tunnel" onClose={onClose}>
        <div className="text-[var(--zf-muted)] text-sm">Loading...</div>
      </ModalWrapper>
    )
  }
  if (loadErr) {
    return (
      <ModalWrapper title="Edit VPN Tunnel" onClose={onClose}>
        <p className="text-red-600 text-sm">{loadErr}</p>
      </ModalWrapper>
    )
  }

  return (
    <ModalWrapper title="Edit VPN Tunnel" onClose={onClose}>
      <div className="space-y-4">
        <InputField label="Name" value={name} onChange={setName} placeholder="wg-site-a" />
        <InputField label="Description" value={description} onChange={setDescription} placeholder="Site A WireGuard tunnel" />
        <div className="grid grid-cols-2 gap-2">
          <InputField label="Interface Name" value={interfaceName} onChange={setInterfaceName} placeholder="wg0" />
          <InputField label="Listen Port" value={listenPort} onChange={setListenPort} placeholder="51820" type="number" />
        </div>
        <InputField label="Address" value={address} onChange={setAddress} placeholder="10.10.0.1/24" />
        <InputField label="Private Key" value={privateKey} onChange={setPrivateKey} placeholder="Leave blank to keep existing key" />
        <div className="border border-[var(--zf-hairline)] rounded-lg p-4 space-y-3">
          <div className="text-sm font-medium text-[var(--zf-ink)]">Add Peer</div>
          <InputField label="Public Key" value={peerKey} onChange={setPeerKey} placeholder="Base64 public key" />
          <div className="grid grid-cols-2 gap-2">
            <InputField label="Endpoint" value={peerEndpoint} onChange={setPeerEndpoint} placeholder="1.2.3.4:51820" />
            <InputField label="Allowed IPs (comma-separated)" value={peerAllowedIps} onChange={setPeerAllowedIps} placeholder="10.0.0.0/24" />
          </div>
          <button type="button" onClick={addPeer} className="flex items-center gap-1 text-sm text-[var(--zf-link)] hover:text-blue-300 transition">
            <Plus className="w-3.5 h-3.5" /> Add Peer
          </button>
          {peers.length > 0 && (
            <div className="space-y-1 mt-2">
              {peers.map((p, i) => (
                <div key={i} className="flex items-center gap-2 text-xs bg-[var(--zf-surface)] rounded px-2 py-1">
                  <span className="text-[var(--zf-ink)] truncate">{p.public_key.slice(0, 20)}...</span>
                  {p.endpoint && <span className="text-[var(--zf-muted)]">{p.endpoint}</span>}
                  <button onClick={() => setPeers(prev => prev.filter((_, j) => j !== i))} className="ml-auto text-red-600 hover:text-red-300" aria-label="Delete" title="Delete">
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
        {err && <p className="text-red-600 text-sm">{err}</p>}
        <button onClick={handleSubmit} disabled={submitting} className="w-full bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] disabled:opacity-50 text-white py-2 px-4 rounded-lg transition">
          {submitting ? 'Saving...' : 'Save Changes'}
        </button>
      </div>
    </ModalWrapper>
  )
}

export default VpnTabContent
