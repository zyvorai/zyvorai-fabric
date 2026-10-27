// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Trash2, RefreshCw, Key, Shield, Lock, Unlock } from 'lucide-react'
import {
  listProviders,
  registerProvider,
  removeProvider,
  listEncryptionPolicies,
  createEncryptionPolicy,
  listEncryptedVms,
  rotateVmKey,
  encryptVm,
  decryptVm,
  type KeyProvider,
  type EncryptionPolicy,
  type VmEncryptionStatus,
} from '../api/encryption'
import { listVMs, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader, Modal } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'

export default function Encryption() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [providers, setProviders] = useState<KeyProvider[]>([])
  const [policies, setPolicies] = useState<EncryptionPolicy[]>([])
  const [encryptedVMs, setEncryptedVMs] = useState<VmEncryptionStatus[]>([])
  const [allVMs, setAllVMs] = useState<VM[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load encryption data')
  const [activeTab, setActiveTab] = useState<'providers' | 'policies' | 'vms'>('providers')
  const [showCreateProvider, setShowCreateProvider] = useState(false)
  const [showCreatePolicy, setShowCreatePolicy] = useState(false)
  const [showEncryptVM, setShowEncryptVM] = useState(false)

  const loadData = useCallback(() => {
    return run(async () => {
      const [prov, pol, vms, allVms] = await Promise.all([
        listProviders(),
        listEncryptionPolicies(),
        listEncryptedVms(),
        listVMs(),
      ])
      setProviders(prov)
      setPolicies(pol)
      setEncryptedVMs(vms)
      setAllVMs(allVms)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleRemoveProvider = async (id: string) => {
    const ok = await confirm('Remove Key Provider', 'Remove this key provider?', { variant: 'danger', confirmLabel: 'Remove' })
    if (!ok) return
    try {
      await removeProvider(id)
      toast.success('Key provider removed')
      loadData()
    } catch { toast.error('Failed to remove key provider') }
  }

  const handleRotateKey = async (vmId: string) => {
    const ok = await confirm('Rotate Encryption Key', 'Rotate encryption key for this VM?', { variant: 'danger', confirmLabel: 'Rotate' })
    if (!ok) return
    try {
      await rotateVmKey(vmId)
      toast.success('Key rotation initiated')
      loadData()
    } catch { toast.error('Failed to rotate key') }
  }

  const handleDecrypt = async (vmName: string) => {
    const ok = await confirm('Decrypt VM', `Decrypt '${vmName}'? The VM's disk will no longer be protected by the encryption policy.`, { variant: 'danger', confirmLabel: 'Decrypt' })
    if (!ok) return
    try {
      await decryptVm(vmName)
      toast.success(`'${vmName}' decrypted`)
      loadData()
    } catch (err) { toastFailure(toast, 'Failed to decrypt VM', err) }
  }

  const getStatusColor = (status: string) => {
    const colors: Record<string, string> = {
      connected: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      disconnected: 'text-red-700 bg-red-50 border-red-200',
      error: 'text-red-700 bg-red-50 border-red-200',
      encrypted: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      encrypting: 'text-amber-800 bg-amber-50 border-amber-200',
      decrypting: 'text-amber-800 bg-amber-50 border-amber-200',
    }
    return colors[status] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }


  return (
    <div className="p-6">
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Encryption"
      />

      <PageLoadBanner title="Could not load encryption data" headline={loadError} onRetry={() => void loadData()} />

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
        <div className="zf-panel-muted px-4 py-3">
          <div className="flex items-center gap-2 text-[var(--zf-muted)] text-sm mb-1">
            <Key className="w-4 h-4" /> Key Providers
          </div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{providers.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="flex items-center gap-2 text-[var(--zf-muted)] text-sm mb-1">
            <Shield className="w-4 h-4" /> Policies
          </div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{policies.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Encrypted VMs</div>
          <div className="text-2xl font-bold text-emerald-700">
            {encryptedVMs.filter(v => v.encrypted).length}
          </div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-sm mb-1">Connected Providers</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">
            {providers.filter(p => p.status === 'connected').length}
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex gap-1 mb-4 zf-panel-muted p-1">
        {(['providers', 'policies', 'vms'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`flex-1 px-4 py-2 rounded text-sm font-medium capitalize transition-colors ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'text-[var(--zf-ink)] hover:bg-black/[0.04]'}`}>
            {tab === 'providers' ? 'Key Providers' : tab === 'vms' ? 'Encrypted VMs' : 'Policies'}
          </button>
        ))}
      </div>

      {/* Key Providers Tab */}
      {activeTab === 'providers' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateProvider(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Add Provider
            </button>
          </div>
          <div className="zf-panel">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Type</th>
                  <th className="p-4">Endpoint</th>
                  <th className="p-4">Status</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {providers.length === 0 ? (
                  <tr><td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">No key providers registered.</td></tr>
                ) : providers.map(prov => (
                  <tr key={prov.id} className="hover:bg-black/[0.02]">
                    <td className="p-4 font-medium text-[var(--zf-ink)]">{prov.name}</td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{prov.provider_type}</td>
                    <td className="p-4 text-sm font-mono text-[var(--zf-muted)]">{prov.endpoint}</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded-full text-xs font-medium border ${getStatusColor(prov.status)}`}>
                        {prov.status}
                      </span>
                    </td>
                    <td className="p-4">
                      <button onClick={() => handleRemoveProvider(prov.id)} className="text-[var(--zf-danger)] hover:opacity-70">
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

      {/* Policies Tab */}
      {activeTab === 'policies' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreatePolicy(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Policy
            </button>
          </div>
          <div className="zf-panel">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Provider</th>
                  <th className="p-4">Algorithm</th>
                  <th className="p-4">Encrypt vMotion</th>
                  <th className="p-4">Auto Rotate</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {policies.length === 0 ? (
                  <tr><td colSpan={5} className="p-8 text-center text-[var(--zf-muted)]">No encryption policies.</td></tr>
                ) : policies.map(pol => (
                  <tr key={pol.id} className="hover:bg-black/[0.02]">
                    <td className="p-4">
                      <div className="font-medium text-[var(--zf-ink)]">{pol.name}</div>
                      {pol.description && <div className="text-xs text-[var(--zf-muted)]">{pol.description}</div>}
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{providers.find(p => p.id === pol.key_provider_id)?.name || pol.key_provider_id}</td>
                    <td className="p-4 text-sm font-mono text-[var(--zf-ink)]">{pol.algorithm}</td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{pol.encrypt_vmotion ? 'Yes' : 'No'}</td>
                    <td className="p-4 text-sm text-[var(--zf-ink)]">{pol.auto_rotate_days ? `Every ${pol.auto_rotate_days} days` : 'No'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Encrypted VMs Tab */}
      {activeTab === 'vms' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowEncryptVM(true)} disabled={policies.length === 0}
              title={policies.length === 0 ? 'Create an encryption policy first' : undefined}
              className="zf-btn zf-btn-primary">
              <Lock className="w-4 h-4" /> Encrypt VM
            </button>
          </div>
          <div className="zf-panel">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">VM</th>
                  <th className="p-4">Encrypted</th>
                  <th className="p-4">Policy</th>
                  <th className="p-4">Algorithm</th>
                  <th className="p-4">Last Key Rotation</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {encryptedVMs.length === 0 ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No encrypted VMs.</td></tr>
                ) : encryptedVMs.map(vm => (
                  <tr key={vm.vm_name} className="hover:bg-black/[0.02]">
                    <td className="p-4 font-medium text-[var(--zf-ink)]">{vm.vm_name}</td>
                    <td className="p-4">
                      <span className={`px-2 py-1 rounded-full text-xs font-medium border ${vm.encrypted ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'}`}>
                        {vm.encrypted ? 'Yes' : 'No'}
                      </span>
                    </td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">{policies.find(p => p.id === vm.policy_id)?.name || vm.policy_id || '-'}</td>
                    <td className="p-4 text-sm font-mono text-[var(--zf-ink)]">{vm.algorithm || '-'}</td>
                    <td className="p-4 text-sm text-[var(--zf-muted)]">
                      {vm.last_key_rotation ? new Date(vm.last_key_rotation).toLocaleDateString() : 'Never'}
                    </td>
                    <td className="p-4">
                      {vm.encrypted && (
                        <div className="flex items-center gap-3">
                          <button onClick={() => handleRotateKey(vm.vm_name)}
                            className="flex items-center gap-1 text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm">
                            <RefreshCw className="w-3.5 h-3.5" /> Rotate Key
                          </button>
                          <button onClick={() => handleDecrypt(vm.vm_name)}
                            className="flex items-center gap-1 text-[var(--zf-danger)] hover:opacity-70 text-sm">
                            <Unlock className="w-3.5 h-3.5" /> Decrypt
                          </button>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Create Provider Modal */}
      {showCreateProvider && (
        <CreateProviderModal onClose={() => setShowCreateProvider(false)}
          onCreated={() => { setShowCreateProvider(false); loadData() }} />
      )}

      {/* Create Policy Modal */}
      {showCreatePolicy && (
        <CreatePolicyModal providers={providers} onClose={() => setShowCreatePolicy(false)}
          onCreated={() => { setShowCreatePolicy(false); loadData() }} />
      )}

      {/* Encrypt VM Modal */}
      {showEncryptVM && (
        <EncryptVMModal
          vms={allVMs.filter(v => !encryptedVMs.some(e => e.vm_name === v.name && e.encrypted))}
          policies={policies}
          onClose={() => setShowEncryptVM(false)}
          onEncrypted={() => { setShowEncryptVM(false); loadData() }}
        />
      )}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel}
          variant={confirmState.variant}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}

function CreateProviderModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [providerType, setProviderType] = useState('kmip')
  const [endpoint, setEndpoint] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await registerProvider({ name, provider_type: providerType, endpoint })
      onCreated()
    } catch { toast.error('Failed to register provider') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Register Key Provider</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)}
            className="input-field" required />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Type</label>
          <select value={providerType} onChange={e => setProviderType(e.target.value)}
            className="input-field">
            <option value="kmip">KMIP</option>
            <option value="local">Local (software-based)</option>
            <option value="vault_transit">HashiCorp Vault (Transit)</option>
          </select>
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Endpoint</label>
          <input type="text" value={endpoint} onChange={e => setEndpoint(e.target.value)}
            className="input-field"
            placeholder="https://kms.example.com:5696" required />
        </div>
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button type="submit" className="zf-btn zf-btn-primary flex-1">Register</button>
        </div>
      </form>
    </Modal>
  )
}

function CreatePolicyModal({ providers, onClose, onCreated }: { providers: KeyProvider[]; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [keyProviderId, setKeyProviderId] = useState(providers[0]?.id || '')
  const [algorithm, setAlgorithm] = useState('aes256_xts')
  const [encryptVmotion, setEncryptVmotion] = useState(false)
  const [autoRotate, setAutoRotate] = useState(false)
  const [rotationDays, setRotationDays] = useState(90)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await createEncryptionPolicy({
        name, description: description || undefined, key_provider_id: keyProviderId,
        algorithm, encrypt_vmotion: encryptVmotion,
        auto_rotate_days: autoRotate ? rotationDays : undefined,
      })
      onCreated()
    } catch { toast.error('Failed to create policy') }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Encryption Policy</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)}
            className="input-field" required />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description</label>
          <input type="text" value={description} onChange={e => setDescription(e.target.value)}
            className="input-field" />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Key Provider</label>
          <select value={keyProviderId} onChange={e => setKeyProviderId(e.target.value)}
            className="input-field">
            {providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Algorithm</label>
          <select value={algorithm} onChange={e => setAlgorithm(e.target.value)}
            className="input-field">
            <option value="aes256_xts">AES-256-XTS</option>
            <option value="aes256_cbc">AES-256-CBC</option>
            <option value="cha_cha20_poly1305">ChaCha20-Poly1305</option>
          </select>
        </div>
        <label className="flex items-center gap-2 text-[var(--zf-ink)]">
          <input type="checkbox" checked={encryptVmotion} onChange={e => setEncryptVmotion(e.target.checked)} />
          <span className="text-sm">Encrypt vMotion traffic</span>
        </label>
        <label className="flex items-center gap-2 text-[var(--zf-ink)]">
          <input type="checkbox" checked={autoRotate} onChange={e => setAutoRotate(e.target.checked)} />
          <span className="text-sm">Auto-rotate keys</span>
        </label>
        {autoRotate && (
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Rotation Interval (days)</label>
            <input type="number" value={rotationDays} onChange={e => setRotationDays(Number(e.target.value))}
              className="input-field" min={1} />
          </div>
        )}
        <div className="flex gap-3">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
          <button type="submit" className="zf-btn zf-btn-primary flex-1">Create</button>
        </div>
      </form>
    </Modal>
  )
}

function EncryptVMModal({ vms, policies, onClose, onEncrypted }: { vms: VM[]; policies: EncryptionPolicy[]; onClose: () => void; onEncrypted: () => void }) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState(vms[0]?.name || '')
  const [policyId, setPolicyId] = useState(policies[0]?.id || '')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!vmName || !policyId) return
    setSubmitting(true)
    try {
      await encryptVm(vmName, policyId)
      toast.success(`Encrypting '${vmName}'`)
      onEncrypted()
    } catch (err) { toastFailure(toast, 'Failed to encrypt VM', err) } finally { setSubmitting(false) }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Encrypt Virtual Machine</h2>
      {vms.length === 0 ? (
        <>
          <p className="text-sm text-[var(--zf-muted)] mb-4">All VMs are already encrypted, or no VMs exist yet.</p>
          <button onClick={onClose} className="zf-btn zf-btn-ghost w-full">Close</button>
        </>
      ) : (
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">VM</label>
            <select value={vmName} onChange={e => setVmName(e.target.value)}
              className="input-field">
              {vms.map(v => <option key={v.name} value={v.name}>{v.name}</option>)}
            </select>
          </div>
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Encryption Policy</label>
            <select value={policyId} onChange={e => setPolicyId(e.target.value)}
              className="input-field">
              {policies.map(p => <option key={p.id} value={p.id}>{p.name} ({p.algorithm})</option>)}
            </select>
          </div>
          <p className="text-xs text-[var(--zf-muted)]">The VM's disk will be encrypted using the selected policy's key provider and algorithm.</p>
          <div className="flex gap-3">
            <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost flex-1">Cancel</button>
            <button type="submit" disabled={submitting} className="zf-btn zf-btn-primary flex-1">{submitting ? 'Encrypting...' : 'Encrypt'}</button>
          </div>
        </form>
      )}
    </Modal>
  )
}
