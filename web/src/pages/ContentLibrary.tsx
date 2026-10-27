// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, ShieldCheck, CheckCircle, XCircle } from 'lucide-react'
import {
  listLibraries,
  createLibrary,
  deleteLibrary,
  listLibraryItems,
  deleteLibraryItem,
  listCustomizationSpecs,
  createCustomizationSpec,
  deleteCustomizationSpec,
  listHostProfiles,
  createHostProfile,
  deleteHostProfile,
  checkHostCompliance,
  type Library,
  type LibraryItem,
  type GuestCustomizationSpec,
  type HostProfile,
} from '../api/contentLibrary'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader, Modal } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { TerminalTextarea } from '../components/AppleTerminalFrame'

export default function ContentLibrary() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [libraries, setLibraries] = useState<Library[]>([])
  const [items, setItems] = useState<LibraryItem[]>([])
  const [specs, setSpecs] = useState<GuestCustomizationSpec[]>([])
  const [profiles, setProfiles] = useState<HostProfile[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load content library')
  const [activeTab, setActiveTab] = useState<'libraries' | 'items' | 'specs' | 'profiles'>('libraries')
  const [selectedLibrary, setSelectedLibrary] = useState<string | null>(null)
  const [showCreateLibrary, setShowCreateLibrary] = useState(false)
  const [showCreateSpec, setShowCreateSpec] = useState(false)
  const [showCreateProfile, setShowCreateProfile] = useState(false)
  const [complianceModalProfile, setComplianceModalProfile] = useState<HostProfile | null>(null)

  const loadData = useCallback(() => {
    return run(async () => {
      const [libs, sp, pr] = await Promise.all([
        listLibraries(),
        listCustomizationSpecs(),
        listHostProfiles(),
      ])
      setLibraries(libs)
      setSpecs(sp)
      setProfiles(pr)

      const allItems: LibraryItem[] = []
      for (const lib of libs) {
        try {
          const libItems = await listLibraryItems(lib.id)
          allItems.push(...libItems)
        } catch { /* skip */ }
      }
      setItems(allItems)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDeleteLibrary = async (id: string) => {
    if (!await confirm('Delete Library', 'Delete this library and all its items?')) return
    try { await deleteLibrary(id); toast.success('Library deleted'); loadData() }
    catch { toast.error('Failed to delete library') }
  }

  const handleDeleteItem = async (libraryId: string, itemId: string) => {
    if (!await confirm('Delete Item', 'Delete this item?')) return
    try { await deleteLibraryItem(libraryId, itemId); toast.success('Item deleted'); loadData() }
    catch { toast.error('Failed to delete item') }
  }

  const handleDeleteSpec = async (id: string) => {
    if (!await confirm('Delete Customization Spec', 'Delete this customization spec?')) return
    try { await deleteCustomizationSpec(id); toast.success('Spec deleted'); loadData() }
    catch { toast.error('Failed to delete spec') }
  }

  const handleDeleteProfile = async (id: string) => {
    if (!await confirm('Delete Host Profile', 'Delete this host profile?')) return
    try { await deleteHostProfile(id); toast.success('Profile deleted'); loadData() }
    catch { toast.error('Failed to delete profile') }
  }

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
  }

  const getTypeColor = () => 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'

  const filteredItems = selectedLibrary ? items.filter(i => i.library_id === selectedLibrary) : items


  return (
    <div className="p-6">
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Content Library"
      />

      <PageLoadBanner title="Could not load content library" headline={loadError} onRetry={() => void loadData()} />

      {/* Summary */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Libraries</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{libraries.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Items</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{items.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Guest Customizations</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{specs.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Host Profiles</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{profiles.length}</div>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-[var(--zf-canvas)] rounded-lg p-1">
        {(['libraries', 'items', 'specs', 'profiles'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`flex-1 px-4 py-2 rounded text-sm font-medium transition-colors ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'text-[var(--zf-muted)] hover:bg-black/[0.04] hover:text-[var(--zf-ink)]'}`}>
            {tab === 'specs' ? 'Guest Customization' : tab === 'profiles' ? 'Host Profiles' : tab === 'items' ? 'Item Browser' : 'Libraries'}
          </button>
        ))}
      </div>

      {/* Libraries Tab */}
      {activeTab === 'libraries' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateLibrary(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Library
            </button>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {libraries.length === 0 ? (
              <div className="col-span-full text-center py-12 text-[var(--zf-muted)] zf-panel">No libraries.</div>
            ) : libraries.map(lib => (
              <div key={lib.id} className="zf-panel p-4">
                <div className="flex items-center justify-between mb-2">
                  <span className="font-semibold text-lg text-[var(--zf-ink)]">{lib.name}</span>
                  <span className={`px-2 py-1 rounded text-xs font-medium border ${lib.status === 'active' ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}>{lib.status}</span>
                </div>
                {lib.description && <p className="text-sm text-[var(--zf-muted)] mb-3">{lib.description}</p>}
                <div className="flex justify-between text-sm text-[var(--zf-muted)] mb-3">
                  <span>{lib.item_count} items</span>
                  <span>{formatBytes(lib.total_size_bytes)}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="px-2 py-0.5 text-xs rounded border text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]">
                    {lib.library_type}
                  </span>
                  <div className="flex items-center gap-2">
                    <button onClick={() => { setSelectedLibrary(lib.id); setActiveTab('items') }}
                      className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm">Browse</button>
                    <button onClick={() => handleDeleteLibrary(lib.id)} className="text-[var(--zf-danger)] hover:opacity-70">
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Items Tab */}
      {activeTab === 'items' && (
        <div>
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <select value={selectedLibrary || ''} onChange={e => setSelectedLibrary(e.target.value || null)}
                className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-2 text-sm text-[var(--zf-ink)]">
                <option value="">All Libraries</option>
                {libraries.map(l => <option key={l.id} value={l.id}>{l.name}</option>)}
              </select>
              <span className="text-sm text-[var(--zf-muted)]">{filteredItems.length} items</span>
            </div>
          </div>
          <div className="zf-panel">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Type</th>
                  <th className="p-4">Version</th>
                  <th className="p-4">Size</th>
                  <th className="p-4">Updated</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {filteredItems.length === 0 ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No items found.</td></tr>
                ) : filteredItems.map(item => (
                  <tr key={item.id} className="hover:bg-black/[0.03]">
                    <td className="p-4">
                      <div className="font-medium text-[var(--zf-ink)]">{item.name}</div>
                      {item.description && <div className="text-xs text-[var(--zf-muted)]">{item.description}</div>}
                    </td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded text-xs font-medium border ${getTypeColor()}`}>{item.item_type}</span>
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{item.version}</td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{formatBytes(item.size_bytes)}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{item.updated ? new Date(item.updated).toLocaleDateString() : '-'}</td>
                    <td className="p-4">
                      <button onClick={() => handleDeleteItem(item.library_id, item.id)} className="text-[var(--zf-danger)] hover:opacity-70">
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Guest Customization Specs Tab */}
      {activeTab === 'specs' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateSpec(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Spec
            </button>
          </div>
          <div className="zf-panel">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">OS Type</th>
                  <th className="p-4">Hostname Prefix</th>
                  <th className="p-4">Domain</th>
                  <th className="p-4">DNS Servers</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {specs.length === 0 ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No customization specs.</td></tr>
                ) : specs.map(spec => (
                  <tr key={spec.id} className="hover:bg-black/[0.03]">
                    <td className="p-4 font-medium text-[var(--zf-ink)]">{spec.name}</td>
                    <td className="p-4">
                      <span className="px-2 py-1 rounded text-xs font-medium border text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]">
                        {spec.os_type}
                      </span>
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{spec.hostname_prefix || '-'}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{spec.domain || '-'}</td>
                    <td className="p-4 text-sm font-mono text-[var(--zf-muted)]">{spec.dns_servers?.join(', ') || '-'}</td>
                    <td className="p-4">
                      <button onClick={() => handleDeleteSpec(spec.id)} className="text-[var(--zf-danger)] hover:opacity-70">
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Host Profiles Tab */}
      {activeTab === 'profiles' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateProfile(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Profile
            </button>
          </div>
          <div className="zf-panel">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Compliant</th>
                  <th className="p-4">Non-Compliant</th>
                  <th className="p-4">Status</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {profiles.length === 0 ? (
                  <tr><td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">No host profiles.</td></tr>
                ) : profiles.map(profile => (
                  <tr key={profile.id} className="hover:bg-black/[0.03]">
                    <td className="p-4">
                      <div className="font-medium text-[var(--zf-ink)]">{profile.name}</div>
                      {profile.description && <div className="text-xs text-[var(--zf-muted)]">{profile.description}</div>}
                    </td>
                    <td className="p-4 text-sm text-emerald-700">{profile.compliant_hosts}</td>
                    <td className="p-4 text-sm text-[var(--zf-danger)]">{profile.non_compliant_hosts}</td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{profile.status}</td>
                    <td className="p-4">
                      <div className="flex items-center gap-2">
                        <button onClick={() => setComplianceModalProfile(profile)} className="text-[var(--zf-link)] hover:text-[var(--zf-link-hover)]" title="Check host compliance">
                          <ShieldCheck className="w-4 h-4" />
                        </button>
                        <button onClick={() => handleDeleteProfile(profile.id)} className="text-[var(--zf-danger)] hover:opacity-70">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Modals */}
      {showCreateLibrary && <CreateLibraryModal onClose={() => setShowCreateLibrary(false)} onCreated={() => { setShowCreateLibrary(false); loadData() }} />}
      {showCreateSpec && <CreateSpecModal onClose={() => setShowCreateSpec(false)} onCreated={() => { setShowCreateSpec(false); loadData() }} />}
      {showCreateProfile && <CreateProfileModal onClose={() => setShowCreateProfile(false)} onCreated={() => { setShowCreateProfile(false); loadData() }} />}
      {complianceModalProfile && <HostComplianceModal profile={complianceModalProfile} onClose={() => setComplianceModalProfile(null)} />}
      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel ?? 'Delete'}
          variant={confirmState.variant ?? 'danger'}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}

function CreateLibraryModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [libraryType, setLibraryType] = useState<'local' | 'subscribed'>('local')
  const [storagePath, setStoragePath] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try { await createLibrary({ name, description: description || undefined, library_type: libraryType, storage_path: storagePath }); onCreated() }
    catch { toast.error('Failed to create library') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Library</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)} className="input-field" required /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description</label>
          <input type="text" value={description} onChange={e => setDescription(e.target.value)} className="input-field" /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Type</label>
          <select value={libraryType} onChange={e => setLibraryType(e.target.value as 'local' | 'subscribed')} className="input-field">
            <option value="local">Local</option><option value="subscribed">Subscribed</option>
          </select></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Storage Path</label>
          <input type="text" value={storagePath} onChange={e => setStoragePath(e.target.value)} className="input-field font-mono" required /></div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Create</button>
        </div>
      </form>
    </Modal>
  )
}

function CreateSpecModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [osType, setOsType] = useState<'linux' | 'windows'>('linux')
  const [hostnamePrefix, setHostnamePrefix] = useState('')
  const [domain, setDomain] = useState('')
  const [dnsServers, setDnsServers] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await createCustomizationSpec({ name, os_type: osType, hostname_prefix: hostnamePrefix || undefined, domain: domain || undefined, dns_servers: dnsServers.split(',').map(s => s.trim()).filter(Boolean) })
      onCreated()
    } catch { toast.error('Failed to create spec') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Customization Spec</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)} className="input-field" required /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">OS Type</label>
          <select value={osType} onChange={e => setOsType(e.target.value as 'linux' | 'windows')} className="input-field">
            <option value="linux">Linux</option><option value="windows">Windows</option>
          </select></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Hostname Prefix</label>
          <input type="text" value={hostnamePrefix} onChange={e => setHostnamePrefix(e.target.value)} className="input-field" /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Domain</label>
          <input type="text" value={domain} onChange={e => setDomain(e.target.value)} className="input-field" /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">DNS Servers (comma-separated)</label>
          <input type="text" value={dnsServers} onChange={e => setDnsServers(e.target.value)} className="input-field" placeholder="8.8.8.8, 8.8.4.4" /></div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Create</button>
        </div>
      </form>
    </Modal>
  )
}

function CreateProfileModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try { await createHostProfile({ name, description: description || undefined, settings: {} }); onCreated() }
    catch { toast.error('Failed to create profile') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Host Profile</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)} className="input-field" required /></div>
        <div><label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description</label>
          <input type="text" value={description} onChange={e => setDescription(e.target.value)} className="input-field" /></div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
          <button type="submit" className="flex-1 zf-btn zf-btn-primary">Create</button>
        </div>
      </form>
    </Modal>
  )
}

function HostComplianceModal({ profile, onClose }: { profile: HostProfile; onClose: () => void }) {
  const toast = useToastContext()
  const [hostId, setHostId] = useState('')
  const [configText, setConfigText] = useState(() => JSON.stringify(profile.settings, null, 2))
  const [configError, setConfigError] = useState('')
  const [checking, setChecking] = useState(false)
  const [result, setResult] = useState<{ compliant: boolean; deviations: Array<{ setting: string; expected: string; actual: string }> } | null>(null)

  const handleCheck = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!hostId.trim()) { toast.error('Host ID is required'); return }
    let currentConfig: Record<string, unknown>
    try {
      currentConfig = JSON.parse(configText)
    } catch {
      setConfigError('Current config must be valid JSON')
      return
    }
    setConfigError('')
    setChecking(true)
    setResult(null)
    try {
      const r = await checkHostCompliance(profile.id, hostId.trim(), currentConfig)
      setResult(r)
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Compliance check failed')
    } finally {
      setChecking(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-lg max-h-[85vh] overflow-y-auto">
      <h2 className="text-xl font-bold mb-1 text-[var(--zf-ink)]">Check Host Compliance</h2>
      <p className="text-sm text-[var(--zf-muted)] mb-4">Against profile "{profile.name}"</p>
      <form onSubmit={handleCheck} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Host ID</label>
          <input value={hostId} onChange={e => setHostId(e.target.value)}
            className="input-field" placeholder="host-01" />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Host's current config (JSON)</label>
          <TerminalTextarea
            title="host-config — JSON"
            value={configText}
            onChange={e => setConfigText(e.target.value)}
            rows={8}
          />
          {configError && <p className="text-xs text-[var(--zf-danger)] mt-1">{configError}</p>}
          <p className="text-xs text-[var(--zf-muted)] mt-1">Pre-filled from the profile's reference settings — edit to reflect what's actually configured on this host.</p>
        </div>

        {result && (
          <div className={`rounded-lg border p-3 ${result.compliant ? 'bg-emerald-50 border-emerald-200' : 'bg-red-50 border-red-200'}`}>
            <div className={`flex items-center gap-2 text-sm font-medium ${result.compliant ? 'text-emerald-700' : 'text-red-700'}`}>
              {result.compliant ? <CheckCircle className="w-4 h-4" /> : <XCircle className="w-4 h-4" />}
              {result.compliant ? 'Compliant' : `${result.deviations.length} deviation(s)`}
            </div>
            {!result.compliant && (
              <ul className="mt-2 space-y-1 text-xs text-[var(--zf-ink)]">
                {result.deviations.map((d, i) => (
                  <li key={i}><span className="text-amber-800 font-medium">{d.setting}</span>: expected {d.expected}, got {d.actual}</li>
                ))}
              </ul>
            )}
          </div>
        )}

        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Close</button>
          <button type="submit" disabled={checking} className="flex-1 zf-btn zf-btn-primary">
            {checking ? 'Checking…' : 'Check'}
          </button>
        </div>
      </form>
    </Modal>
  )
}
