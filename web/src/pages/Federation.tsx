// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, Wrench } from 'lucide-react'
import {
  getFederationOverview,
  registerFederationPeer,
  setFederationPeerToken,
  deleteFederationPeer,
  setPeerHostMaintenance,
  type FederationOverview,
  type PeerOverview,
} from '../api/federation'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader, Modal } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'

export const PEER_STATUS_LABEL: Record<string, string> = {
  ok: 'Reachable',
  unreachable: 'Unreachable',
  unauthorized: 'Token refused',
  token_missing: 'Token needed',
  error: 'Error',
}

const STATUS_CLASS: Record<string, string> = {
  ok: 'text-emerald-700 bg-emerald-50 border-emerald-200',
  unreachable: 'text-red-700 bg-red-50 border-red-200',
  unauthorized: 'text-amber-800 bg-amber-50 border-amber-200',
  token_missing: 'text-amber-800 bg-amber-50 border-amber-200',
  error: 'text-red-700 bg-red-50 border-red-200',
}

export default function Federation() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [overview, setOverview] = useState<FederationOverview | null>(null)
  const [showAdd, setShowAdd] = useState(false)
  const [tokenFor, setTokenFor] = useState<PeerOverview | null>(null)
  const { loading, loadError, run } = usePageLoader('Failed to load the federation overview')

  const load = useCallback(() => run(async () => setOverview(await getFederationOverview())), [run])

  useEffect(() => {
    void load()
  }, [load])

  const remove = async (peer: PeerOverview) => {
    if (!await confirm('Remove peer', `Stop showing ${peer.name}? Nothing on that cluster changes.`)) return
    try {
      await deleteFederationPeer(peer.id)
      toast.success('Peer removed')
      void load()
    } catch (err) { toastFailure(toast, 'Failed to remove the peer', err) }
  }

  const toggleMaintenance = async (peer: PeerOverview, hostId: string, inMaintenance: boolean) => {
    try {
      await setPeerHostMaintenance(peer.id, hostId, !inMaintenance)
      toast.success(inMaintenance ? 'Host leaving maintenance' : 'Host entering maintenance')
      void load()
    } catch (err) { toastFailure(toast, 'The peer did not accept that change', err) }
  }

  const totals = overview?.totals

  return (
    <div className="p-6">
      <PageHeader
        title="Federation"
        onRefresh={() => void load()}
        refreshing={loading}
        actions={
          <button onClick={() => setShowAdd(true)} className="zf-btn zf-btn-primary">
            <Plus className="w-4 h-4" />
            Add peer
          </button>
        }
      />
      <PageLoadBanner title="Could not load the federation overview" headline={loadError} onRetry={() => void load()} />

      {loading && !overview && !loadError ? (
        <div className="text-center py-8 text-[var(--zf-muted)]">Loading…</div>
      ) : overview ? (
        <>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mb-4">
            {[
              ['Peers reachable', `${totals?.reachable ?? 0} / ${totals?.peers ?? 0}`],
              ['Hosts', totals?.hosts ?? 0],
              ['In maintenance', totals?.hosts_in_maintenance ?? 0],
              ['VMs', totals?.vms ?? 0],
            ].map(([label, value]) => (
              <div key={String(label)} className="zf-panel px-4 py-3">
                <div className="text-[var(--zf-muted)] text-sm mb-1">{label}</div>
                <div className="text-2xl font-bold text-[var(--zf-ink)]">{value}</div>
              </div>
            ))}
          </div>

          {overview.peers.length === 0 ? (
            <div className="zf-panel text-center py-12 text-[var(--zf-muted)]">
              No peers yet. Add another Fabric daemon to see its hosts here.
            </div>
          ) : (
            overview.peers.map(peer => (
              <div key={peer.id} className="zf-panel mb-4" data-testid={`peer-${peer.id}`}>
                <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--zf-hairline)]">
                  <div>
                    <span className="font-semibold text-[var(--zf-ink)]">{peer.name}</span>
                    <span className="ml-2 text-sm text-[var(--zf-muted)]">{peer.url}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className={`text-xs px-2 py-0.5 rounded border ${STATUS_CLASS[peer.status] ?? ''}`}>
                      {PEER_STATUS_LABEL[peer.status] ?? peer.status}
                    </span>
                    {(peer.status === 'token_missing' || peer.status === 'unauthorized') && (
                      <button className="zf-btn zf-btn-ghost" onClick={() => setTokenFor(peer)}>Set token</button>
                    )}
                    <button className="zf-btn zf-btn-ghost" aria-label={`Remove ${peer.name}`} onClick={() => void remove(peer)}>
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
                {peer.status !== 'ok' ? (
                  <div className="px-4 py-3 text-sm text-[var(--zf-muted)]">{peer.message ?? 'No data.'}</div>
                ) : (
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-[var(--zf-muted)]">
                        <th className="px-4 py-2">Host</th><th>Status</th><th>CPUs</th><th>Memory</th><th>VMs</th><th />
                      </tr>
                    </thead>
                    <tbody>
                      {(peer.hosts ?? []).map(h => (
                        <tr key={h.id} className="border-t border-[var(--zf-hairline)]">
                          <td className="px-4 py-2 text-[var(--zf-ink)]">{h.hostname}</td>
                          <td>{h.status}</td>
                          <td>{h.cpus}</td>
                          <td>{Math.round(h.memory_mb / 1024)} GB</td>
                          <td>{h.vm_count}</td>
                          <td className="text-right pr-4">
                            <button
                              className="zf-btn zf-btn-ghost"
                              title="Forwarded to the peer with your own token"
                              onClick={() => void toggleMaintenance(peer, h.id, h.status === 'maintenance')}
                            >
                              <Wrench className="w-4 h-4" />
                              {h.status === 'maintenance' ? 'Exit maintenance' : 'Maintenance'}
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            ))
          )}
        </>
      ) : null}

      {showAdd && (
        <AddPeerModal
          onClose={() => setShowAdd(false)}
          onAdded={() => { setShowAdd(false); void load() }}
        />
      )}
      {tokenFor && (
        <TokenModal
          peer={tokenFor}
          onClose={() => setTokenFor(null)}
          onSaved={() => { setTokenFor(null); void load() }}
        />
      )}
      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel ?? 'Remove'}
          variant={confirmState.variant ?? 'danger'}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}

function AddPeerModal({ onClose, onAdded }: { onClose: () => void; onAdded: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [url, setUrl] = useState('')
  const [token, setToken] = useState('')

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await registerFederationPeer({ name, url, token: token || undefined })
      onAdded()
    } catch (err) { toastFailure(toast, 'Failed to add the peer', err) }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Add peer</h2>
      <form onSubmit={submit} className="space-y-4">
        <div>
          <label htmlFor="peer-name" className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input id="peer-name" type="text" value={name} onChange={e => setName(e.target.value)} className="input-field" required />
        </div>
        <div>
          <label htmlFor="peer-url" className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Address</label>
          <input id="peer-url" type="url" value={url} onChange={e => setUrl(e.target.value)} className="input-field" placeholder="https://fabric-b.example:9095" required />
        </div>
        <div>
          <label htmlFor="peer-token" className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Read-only token</label>
          <input id="peer-token" type="password" value={token} onChange={e => setToken(e.target.value)} className="input-field" autoComplete="off" />
          <p className="text-xs text-[var(--zf-muted)] mt-1">Kept in memory on this daemon; set it again after a restart.</p>
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Add</button>
        </div>
      </form>
    </Modal>
  )
}

function TokenModal({ peer, onClose, onSaved }: { peer: PeerOverview; onClose: () => void; onSaved: () => void }) {
  const toast = useToastContext()
  const [token, setToken] = useState('')

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await setFederationPeerToken(peer.id, token)
      onSaved()
    } catch (err) { toastFailure(toast, 'Failed to set the token', err) }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Token for {peer.name}</h2>
      <form onSubmit={submit} className="space-y-4">
        <input type="password" aria-label="Token" value={token} onChange={e => setToken(e.target.value)} className="input-field" autoComplete="off" required />
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Save</button>
        </div>
      </form>
    </Modal>
  )
}
