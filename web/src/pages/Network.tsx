// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback, useMemo, type Dispatch, type SetStateAction } from 'react'
import { RefreshCw, Server, Layers, Cable, Terminal, Link2, Settings, FileText, ArrowRightLeft, Radio, Cpu, Globe, ScanSearch } from 'lucide-react'
import * as api from '../api/networkd'
import * as cloudApi from '../api/network-cloud'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import type {
  BridgeConfig, VlanConfig, MacvtapConfig, TapConfig, LinkInfo,
  BondConfig, NetworkFileConfig, LinkFileConfig, PortForwardConfig,
  VxlanConfig, SriovConfig, ParsedConfigFile,
} from '../api/networkd'
import {
  BridgesTab, CreateBridgeModal, EditBridgeModal, DhcpServerModal,
  BondsTab, CreateBondModal, EditBondModal,
  VlansTab, CreateVlanModal, EditVlanModal,
  MacvtapTab, CreateMacvtapModal,
  TapsTab, CreateTapModal,
  NetfilesTab, CreateNetfileModal,
  LinkfilesTab, CreateLinkfileModal,
  PortForwardsTab, CreatePortForwardModal,
  VxlansTab, CreateVxlanModal,
  SriovTab, CreateSriovModal,
  FloatingIpsTab, CreateFloatingIpModal,
  StatusTab,
} from './network/index'
import type { FloatingIp, DhcpServerConfig } from '../api/network-cloud'
import { ZYVOR_FABRIC_HELP } from '../config/zyvorHelp'

const FABRIC = ZYVOR_FABRIC_HELP.name
import { countNetfileTypes } from './network/NetfilesTab'
import { extractErrorMessage, ModalWrapper } from './network/ModalShared'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { useToastContext } from '../contexts/ToastContext'
import { hintsForError } from '../utils/daemonHints'
import SubsystemBanner from '../components/SubsystemBanner'
import { useSilentPoll } from '../hooks/useSilentPoll'
import { usePermissions } from '../hooks/usePermissions'
import { ReadOnlyProvider } from '../contexts/ReadOnlyContext'
import ReadOnlyNotice from '../components/ReadOnlyNotice'
import { PageHeader } from '../components/ui'

type Tab = 'bridges' | 'bonds' | 'vlans' | 'macvtap' | 'taps' | 'netfiles' | 'linkfiles' | 'portforwards' | 'vxlan' | 'sriov' | 'floatingips' | 'status'
type Modal = 'bridge' | 'bond' | 'vlan' | 'macvtap' | 'tap' | 'netfile' | 'linkfile' | 'portforward' | 'vxlan' | 'sriov' | 'floatingip'
  | 'edit-bridge' | 'edit-bond' | 'edit-vlan' | 'dhcp' | null

export default function Network() {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [activeTab, setActiveTab] = useState<Tab>('bridges')
  const [bridges, setBridges] = useState<BridgeConfig[]>([])
  const [bonds, setBonds] = useState<BondConfig[]>([])
  const [vlans, setVlans] = useState<VlanConfig[]>([])
  const [macvtaps, setMacvtaps] = useState<MacvtapConfig[]>([])
  const [taps, setTaps] = useState<TapConfig[]>([])
  const [netfiles, setNetfiles] = useState<NetworkFileConfig[]>([])
  const [linkfiles, setLinkfiles] = useState<LinkFileConfig[]>([])
  const [portForwards, setPortForwards] = useState<PortForwardConfig[]>([])
  const [vxlans, setVxlans] = useState<VxlanConfig[]>([])
  const [sriov, setSriov] = useState<SriovConfig[]>([])
  const [floatingIps, setFloatingIps] = useState<FloatingIp[]>([])
  const [dhcpServers, setDhcpServers] = useState<DhcpServerConfig[]>([])
  const [links, setLinks] = useState<LinkInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  // Single modal state replaces 8 separate booleans
  const [activeModal, setActiveModal] = useState<Modal>(null)
  const [editingBridge, setEditingBridge] = useState<BridgeConfig | null>(null)
  const [editingBond, setEditingBond] = useState<BondConfig | null>(null)
  const [editingVlan, setEditingVlan] = useState<VlanConfig | null>(null)
  const [dhcpBridge, setDhcpBridge] = useState<BridgeConfig | null>(null)
  const [showScanModal, setShowScanModal] = useState(false)
  const { confirmState, confirm, cancel } = useConfirm()

  const fetchAll = useCallback(async (silent = false) => {
    if (!silent) {
      setLoading(true)
      setError(null)
    }
    try {
      const [b, bo, v, m, t, nf, lf, pf, vx, sr, fip, dhcp, l] = await Promise.all([
        api.listBridges(),
        api.listBonds(),
        api.listVlans(),
        api.listMacvtaps(),
        api.listTaps(),
        api.listNetworkFiles(),
        api.listLinkFiles(),
        api.listPortForwards(),
        api.listVxlans(),
        api.listSriov(),
        cloudApi.listFloatingIps().catch(() => []),
        cloudApi.listDhcpServers().catch(() => []),
        api.listLinks().catch(() => []),
      ])
      setBridges(b)
      setBonds(bo)
      setVlans(v)
      setMacvtaps(m)
      setTaps(t)
      setNetfiles(nf)
      setLinkfiles(lf)
      setPortForwards(pf)
      setVxlans(vx)
      setSriov(sr)
      setFloatingIps(fip)
      setDhcpServers(dhcp)
      setLinks(l)
    } catch (e: unknown) {
      if (!silent) {
        const msg = formatUserError(e)
        setError(msg)
        toastFailure(toast, 'Failed to load network configuration', e)
      }
    } finally {
      if (!silent) setLoading(false)
    }
  }, [toast])

  const failAction = useCallback((label: string, e: unknown) => {
    setError(extractErrorMessage(e))
    toastFailure(toast, label, e)
  }, [toast])

  useEffect(() => { void fetchAll() }, [fetchAll])
  useSilentPoll(() => fetchAll(true), 15000)

  const handleReload = async () => {
    try {
      await api.reloadNetworkd()
      await fetchAll()
    } catch (e: unknown) {
      failAction('Failed to reload networkd', e)
    }
  }

  const handleDeleteBridge = async (id: string) => {
    if (!await confirm('Delete Bridge', 'Delete this bridge and its systemd-networkd config files?')) return
    try {
      await api.deleteBridge(id)
      setBridges(prev => prev.filter(b => b.id !== id))
    } catch (e: unknown) { failAction('Failed to delete bridge', e) }
  }

  const handleAdoptBridge = async (hostId: string) => {
    const name = hostId.replace(/^host:/, '')
    if (!await confirm(
      'Adopt Bridge',
      `Import "${name}" into ${FABRIC}? This writes systemd-networkd config files and reloads networkd.`,
    )) return
    try {
      const adopted = await api.adoptBridge(hostId)
      setBridges(prev => [...prev.filter(b => b.id !== hostId), adopted])
      toast.success(`Bridge "${adopted.name}" is now managed by ${FABRIC}`)
    } catch (e: unknown) { failAction('Failed to adopt bridge', e) }
  }

  const handleDeleteVlan = async (id: string) => {
    if (!await confirm('Delete VLAN', 'Delete this VLAN?')) return
    try {
      await api.deleteVlan(id)
      setVlans(prev => prev.filter(v => v.id !== id))
    } catch (e: unknown) { failAction('Failed to delete VLAN', e) }
  }

  const handleDeleteMacvtap = async (id: string) => {
    if (!await confirm('Delete Macvtap', 'Delete this macvtap device?')) return
    try {
      await api.deleteMacvtap(id)
      setMacvtaps(prev => prev.filter(m => m.id !== id))
    } catch (e: unknown) { failAction('Failed to delete macvtap', e) }
  }

  const handleDeleteTap = async (id: string) => {
    if (!await confirm('Delete Tap', 'Delete this tap device?')) return
    try {
      await api.deleteTap(id)
      setTaps(prev => prev.filter(t => t.id !== id))
    } catch (e: unknown) { failAction('Failed to delete tap', e) }
  }

  const handleDeleteBond = async (id: string) => {
    if (!await confirm('Delete Bond', 'Delete this bond and its systemd-networkd config files?')) return
    try {
      await api.deleteBond(id)
      setBonds(prev => prev.filter(b => b.id !== id))
    } catch (e: unknown) { failAction('Failed to delete bond', e) }
  }

  const handleDeleteNetfile = async (id: string) => {
    if (!await confirm('Delete Network File', 'Delete this network file config?')) return
    try {
      await api.deleteNetworkFile(id)
      setNetfiles(prev => prev.filter(n => n.id !== id))
    } catch (e: unknown) { failAction('Failed to delete network file', e) }
  }

  const handleAdoptNetfile = async (hostId: string) => {
    const name = hostId.replace(/^host:/, '')
    if (!await confirm(
      'Adopt Interface',
      `Import "${name}" into ${FABRIC}? This writes a .network file and reloads networkd.`,
    )) return
    try {
      const adopted = await api.adoptNetworkFile(hostId)
      setNetfiles(prev => [...prev.filter(n => n.id !== hostId), adopted])
      toast.success(`Interface "${adopted.match_name}" is now managed by ${FABRIC}`)
    } catch (e: unknown) { failAction('Failed to adopt interface', e) }
  }

  const adoptHost = async <T extends { id: string; name?: string; match_name?: string }>(
    label: string,
    hostId: string,
    adoptFn: (id: string) => Promise<T>,
    setState: Dispatch<SetStateAction<T[]>>,
  ) => {
    const name = hostId.replace(/^host:/, '')
    if (!await confirm(`Adopt ${label}`, `Import "${name}" into ${FABRIC} and write systemd-networkd config?`)) return
    try {
      const adopted = await adoptFn(hostId)
      setState(prev => [...prev.filter(x => x.id !== hostId), adopted])
      const display = adopted.match_name ?? adopted.name ?? name
      toast.success(`${label} "${display}" is now managed by ${FABRIC}`)
    } catch (e: unknown) {
      failAction(`Failed to adopt ${label.toLowerCase()}`, e)
    }
  }

  const handleDeleteLinkfile = async (id: string) => {
    if (!await confirm('Delete Link File', 'Delete this link file config?')) return
    try {
      await api.deleteLinkFile(id)
      setLinkfiles(prev => prev.filter(l => l.id !== id))
    } catch (e: unknown) { failAction('Failed to delete link file', e) }
  }

  const handleDeletePortForward = async (id: string) => {
    if (!await confirm('Delete Port Forward', 'Delete this port forward rule?')) return
    try {
      await api.deletePortForward(id)
      setPortForwards(prev => prev.filter(p => p.id !== id))
    } catch (e: unknown) { failAction('Failed to delete port forward', e) }
  }

  const handleSyncPortForwards = async () => {
    try {
      await api.syncPortForwards()
      await fetchAll()
    } catch (e: unknown) { failAction('Failed to sync port forwards', e) }
  }

  const handleDeleteVxlan = async (id: string) => {
    if (!await confirm('Delete VXLAN', 'Delete this VXLAN configuration?')) return
    try {
      await api.deleteVxlan(id)
      setVxlans(prev => prev.filter(v => v.id !== id))
    } catch (e: unknown) { failAction('Failed to delete VXLAN', e) }
  }

  const handleDeleteSriov = async (id: string) => {
    if (!await confirm('Delete SR-IOV', 'Delete this SR-IOV configuration?')) return
    try {
      await api.deleteSriov(id)
      setSriov(prev => prev.filter(s => s.id !== id))
    } catch (e: unknown) { failAction('Failed to delete SR-IOV', e) }
  }

  const handleDeleteFloatingIp = async (id: string) => {
    if (!await confirm('Delete Floating IP', `Remove this floating IP from ${FABRIC}?`)) return
    try {
      await cloudApi.deleteFloatingIp(id)
      setFloatingIps(prev => prev.filter(f => f.id !== id))
    } catch (e: unknown) { failAction('Failed to delete floating IP', e) }
  }

  const handleAdoptFloatingIp = async (hostId: string) => {
    if (!await confirm('Adopt Floating IP', `Import this host address into ${FABRIC}?`)) return
    try {
      const adopted = await cloudApi.adoptFloatingIp(hostId)
      setFloatingIps(prev => [...prev.filter(f => f.id !== hostId), adopted])
      toast.success(`Floating IP ${adopted.address} is now managed by ${FABRIC}`)
    } catch (e: unknown) { failAction('Failed to adopt floating IP', e) }
  }

  const handleAssignFloatingIp = async (id: string, vmName: string) => {
    try {
      const updated = await cloudApi.assignFloatingIp(id, vmName)
      setFloatingIps(prev => prev.map(f => f.id === id ? updated : f))
      toast.success(`Assigned ${updated.address} to ${vmName}`)
    } catch (e: unknown) { failAction('Failed to assign floating IP', e) }
  }

  const handleUnassignFloatingIp = async (id: string) => {
    if (!await confirm('Unassign Floating IP', 'Remove this address from the interface?')) return
    try {
      const updated = await cloudApi.unassignFloatingIp(id)
      setFloatingIps(prev => prev.map(f => f.id === id ? updated : f))
    } catch (e: unknown) { failAction('Failed to unassign floating IP', e) }
  }

  const interfaceOptions = useMemo(() => {
    const names = new Set<string>()
    for (const b of bridges) names.add(b.name)
    for (const l of links) {
      if (l.kind === 'bridge' || l.name.match(/^(eno|eth|enp|ens|virbr)/)) names.add(l.name)
    }
    return [...names].sort()
  }, [bridges, links])

  const netfileCounts = useMemo(() => countNetfileTypes(netfiles), [netfiles])

  const closeModal = () => {
    setActiveModal(null)
    setEditingBridge(null)
    setEditingBond(null)
    setEditingVlan(null)
    setDhcpBridge(null)
  }

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'bridges', label: 'Bridges', icon: <Server className="w-4 h-4" /> },
    { key: 'bonds', label: 'Bonds', icon: <Link2 className="w-4 h-4" /> },
    { key: 'vlans', label: 'VLANs', icon: <Layers className="w-4 h-4" /> },
    { key: 'macvtap', label: 'Macvtap', icon: <Cable className="w-4 h-4" /> },
    { key: 'taps', label: 'Tap', icon: <Terminal className="w-4 h-4" /> },
    { key: 'netfiles', label: 'Interfaces', icon: <Settings className="w-4 h-4" /> },
    { key: 'linkfiles', label: 'Link Files', icon: <FileText className="w-4 h-4" /> },
    { key: 'portforwards', label: 'Port Forwards', icon: <ArrowRightLeft className="w-4 h-4" /> },
    { key: 'vxlan', label: 'VXLAN', icon: <Radio className="w-4 h-4" /> },
    { key: 'sriov', label: 'SR-IOV', icon: <Cpu className="w-4 h-4" /> },
    { key: 'floatingips', label: 'Floating IPs', icon: <Globe className="w-4 h-4" /> },
    { key: 'status', label: 'Status', icon: <RefreshCw className="w-4 h-4" /> },
  ]

  return (
    <ReadOnlyProvider readOnly={!canWrite}>
    <div className="space-y-6">
      <SubsystemBanner subsystem="vm_driver" title="Network stack" />
      {!canWrite && <ReadOnlyNotice />}
      <PageHeader
        title="Network"
        description="Bridges, bonds, VLANs, and other systemd-networkd configuration"
        actions={
          <>
            <button onClick={() => setShowScanModal(true)} className="zf-btn zf-btn-ghost zf-btn-sm">
              <ScanSearch className="w-3.5 h-3.5" />
              Scan Configs
            </button>
            {canWrite && (
              <button onClick={handleReload} className="zf-btn zf-btn-ghost zf-btn-sm">
                <RefreshCw className="w-3.5 h-3.5" />
                Reload networkd
              </button>
            )}
          </>
        }
      />

      {error && (
        <ErrorBanner
          title="Could not load network"
          headline={error}
          hints={hintsForError(error, 'network')}
          onRetry={fetchAll}
        />
      )}

      {/* Stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-10 gap-3">
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Bridges</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{bridges.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Bonds</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{bonds.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">VLANs</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{vlans.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Macvtap</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{macvtaps.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Tap</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{taps.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Interfaces</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{netfileCounts.total}</div>
          <div className="text-[10px] text-[var(--zf-muted)] mt-0.5">
            {netfileCounts.physical} phys · {netfileCounts.container} ctr
          </div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Link Files</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{linkfiles.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Port Forwards</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{portForwards.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">VXLAN</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{vxlans.length}</div>
        </div>
        <div className="zf-panel px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">SR-IOV</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{sriov.length}</div>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-[var(--zf-hairline)] overflow-x-auto">
        <div className="flex gap-1 w-max min-w-full">
          {tabs.map(t => (
            <button
              key={t.key}
              onClick={() => setActiveTab(t.key)}
              className={`flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition shrink-0 ${
                activeTab === t.key
                  ? 'border-[var(--zf-link)] text-[var(--zf-link)]'
                  : 'border-transparent text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
              }`}
            >
              {t.icon}
              {t.label}
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <div className="text-center text-[var(--zf-muted)] py-12">Loading...</div>
      ) : (
        <>
          {activeTab === 'bridges' && (
            <BridgesTab
              bridges={bridges}
              dhcpServers={dhcpServers}
              onDelete={handleDeleteBridge}
              onAdopt={handleAdoptBridge}
              onCreate={() => setActiveModal('bridge')}
              onEdit={b => { setEditingBridge(b); setActiveModal('edit-bridge') }}
              onConfigureDhcp={b => { setDhcpBridge(b); setActiveModal('dhcp') }}
            />
          )}
          {activeTab === 'bonds' && (
            <BondsTab
              bonds={bonds}
              onDelete={handleDeleteBond}
              onAdopt={id => adoptHost('Bond', id, api.adoptBond, setBonds)}
              onCreate={() => setActiveModal('bond')}
              onEdit={b => { setEditingBond(b); setActiveModal('edit-bond') }}
            />
          )}
          {activeTab === 'vlans' && (
            <VlansTab
              vlans={vlans}
              onDelete={handleDeleteVlan}
              onAdopt={id => adoptHost('VLAN', id, api.adoptVlan, setVlans)}
              onCreate={() => setActiveModal('vlan')}
              onEdit={v => { setEditingVlan(v); setActiveModal('edit-vlan') }}
            />
          )}
          {activeTab === 'macvtap' && (
            <MacvtapTab macvtaps={macvtaps} onDelete={handleDeleteMacvtap} onAdopt={id => adoptHost('Macvtap', id, api.adoptMacvtap, setMacvtaps)} onCreate={() => setActiveModal('macvtap')} />
          )}
          {activeTab === 'taps' && (
            <TapsTab taps={taps} onDelete={handleDeleteTap} onAdopt={id => adoptHost('Tap', id, api.adoptTap, setTaps)} onCreate={() => setActiveModal('tap')} />
          )}
          {activeTab === 'netfiles' && (
            <NetfilesTab netfiles={netfiles} onDelete={handleDeleteNetfile} onAdopt={handleAdoptNetfile} onCreate={() => setActiveModal('netfile')} />
          )}
          {activeTab === 'linkfiles' && (
            <LinkfilesTab linkfiles={linkfiles} onDelete={handleDeleteLinkfile} onCreate={() => setActiveModal('linkfile')} />
          )}
          {activeTab === 'portforwards' && (
            <PortForwardsTab
              portForwards={portForwards}
              onDelete={handleDeletePortForward}
              onAdopt={id => adoptHost('Port forward', id, api.adoptPortForward, setPortForwards)}
              onCreate={() => setActiveModal('portforward')}
              onSync={handleSyncPortForwards}
            />
          )}
          {activeTab === 'vxlan' && (
            <VxlansTab
              vxlans={vxlans}
              onDelete={handleDeleteVxlan}
              onAdopt={id => adoptHost('VXLAN', id, api.adoptVxlan, setVxlans)}
              onCreate={() => setActiveModal('vxlan')}
            />
          )}
          {activeTab === 'sriov' && (
            <SriovTab
              sriov={sriov}
              onDelete={handleDeleteSriov}
              onAdopt={id => adoptHost('SR-IOV', id, api.adoptSriov, setSriov)}
              onCreate={() => setActiveModal('sriov')}
            />
          )}
          {activeTab === 'floatingips' && (
            <FloatingIpsTab
              floatingIps={floatingIps}
              onDelete={handleDeleteFloatingIp}
              onAdopt={handleAdoptFloatingIp}
              onAssign={handleAssignFloatingIp}
              onUnassign={handleUnassignFloatingIp}
              onCreate={() => setActiveModal('floatingip')}
            />
          )}
          {activeTab === 'status' && <StatusTab links={links} onRefresh={fetchAll} />}
        </>
      )}

      {/* Modals */}
      {canWrite && activeModal === 'bridge' && <CreateBridgeModal onClose={closeModal} onCreated={(b) => { setBridges(prev => [...prev, b]); closeModal() }} />}
      {canWrite && activeModal === 'bond' && <CreateBondModal onClose={closeModal} onCreated={(b) => { setBonds(prev => [...prev, b]); closeModal() }} />}
      {canWrite && activeModal === 'vlan' && <CreateVlanModal onClose={closeModal} onCreated={(v) => { setVlans(prev => [...prev, v]); closeModal() }} />}
      {canWrite && activeModal === 'edit-bridge' && editingBridge && (
        <EditBridgeModal bridge={editingBridge} onClose={closeModal} onUpdated={(b) => { setBridges(prev => prev.map(x => x.id === b.id ? b : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'dhcp' && dhcpBridge && (
        <DhcpServerModal
          bridge={dhcpBridge}
          existing={dhcpServers.find(d => d.bridge === dhcpBridge.name) ?? null}
          onClose={closeModal}
          onCreated={(d) => { setDhcpServers(prev => prev.some(x => x.id === d.id) ? prev.map(x => x.id === d.id ? d : x) : [...prev, d]); closeModal() }}
          onDeleted={(id) => { setDhcpServers(prev => prev.filter(d => d.id !== id)); closeModal() }}
        />
      )}
      {canWrite && activeModal === 'edit-bond' && editingBond && (
        <EditBondModal bond={editingBond} onClose={closeModal} onUpdated={(b) => { setBonds(prev => prev.map(x => x.id === b.id ? b : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-vlan' && editingVlan && (
        <EditVlanModal vlan={editingVlan} onClose={closeModal} onUpdated={(v) => { setVlans(prev => prev.map(x => x.id === v.id ? v : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'macvtap' && <CreateMacvtapModal onClose={closeModal} onCreated={(m) => { setMacvtaps(prev => [...prev, m]); closeModal() }} />}
      {canWrite && activeModal === 'tap' && <CreateTapModal onClose={closeModal} onCreated={(t) => { setTaps(prev => [...prev, t]); closeModal() }} />}
      {canWrite && activeModal === 'netfile' && <CreateNetfileModal onClose={closeModal} onCreated={(n) => { setNetfiles(prev => [...prev, n]); closeModal() }} />}
      {canWrite && activeModal === 'linkfile' && <CreateLinkfileModal onClose={closeModal} onCreated={(l) => { setLinkfiles(prev => [...prev, l]); closeModal() }} />}
      {canWrite && activeModal === 'portforward' && <CreatePortForwardModal onClose={closeModal} onCreated={(pf) => { setPortForwards(prev => [...prev, pf]); closeModal() }} />}
      {canWrite && activeModal === 'vxlan' && <CreateVxlanModal onClose={closeModal} onCreated={(v) => { setVxlans(prev => [...prev, v]); closeModal() }} />}
      {canWrite && activeModal === 'sriov' && <CreateSriovModal onClose={closeModal} onCreated={(s) => { setSriov(prev => [...prev, s]); closeModal() }} />}
      {canWrite && activeModal === 'floatingip' && <CreateFloatingIpModal interfaceOptions={interfaceOptions} onClose={closeModal} onCreated={(f) => { setFloatingIps(prev => [...prev, f]); closeModal() }} />}
      {showScanModal && <ScanConfigsModal onClose={() => setShowScanModal(false)} />}
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
    </ReadOnlyProvider>
  )
}

function ScanConfigsModal({ onClose }: { onClose: () => void }) {
  const [data, setData] = useState<ParsedConfigFile[] | null>(null)
  const [loading, setLoading] = useState(true)
  const [err, setErr] = useState('')
  const [expanded, setExpanded] = useState<Set<string>>(new Set())

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const result = await api.scanConfigs()
        if (!cancelled) setData(result)
      } catch (e: unknown) {
        if (!cancelled) setErr(extractErrorMessage(e))
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => { cancelled = true }
  }, [])

  const toggle = (filename: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(filename)) next.delete(filename)
      else next.add(filename)
      return next
    })
  }

  return (
    <ModalWrapper title="Scanned Host Configs" onClose={onClose}>
      <div className="space-y-3">
        {loading && <p className="text-[var(--zf-muted)] text-sm">Scanning host configuration files…</p>}
        {err && <p className="text-[var(--zf-danger)] text-sm">{err}</p>}
        {data && data.length === 0 && <p className="text-[var(--zf-muted)] text-sm">No existing config files found on host.</p>}
        {data && data.map(f => (
          <div key={f.filename} className="border border-[var(--zf-hairline)] rounded-lg">
            <button
              type="button"
              onClick={() => toggle(f.filename)}
              className="w-full flex items-center justify-between p-3 text-left hover:bg-black/[0.04] transition"
            >
              <span className="font-mono text-sm text-[var(--zf-ink)]">{f.filename}</span>
              <span className="text-xs text-[var(--zf-muted)]">{f.file_type}</span>
            </button>
            {expanded.has(f.filename) && (
              <div className="px-3 pb-3 space-y-2">
                {f.sections.map((s, i) => (
                  <div key={i} className="text-xs">
                    <div className="text-[var(--zf-muted)] font-medium mb-1">[{s.name}]</div>
                    <div className="pl-3 space-y-0.5 font-mono text-[var(--zf-muted)]">
                      {s.entries.map(([k, v], j) => <div key={j}>{k}={v}</div>)}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </ModalWrapper>
  )
}
