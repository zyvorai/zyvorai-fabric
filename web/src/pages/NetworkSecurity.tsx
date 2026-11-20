// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useState, useEffect, useCallback } from 'react'
import {
  Shield, ShieldCheck, Globe, Server, Gauge,
  Wifi, Eye, ArrowUpDown, Activity,
} from 'lucide-react'
import * as api from '../api/network-security'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import type {
  NetworkPolicy, FirewallProfile, FirewallZone, VMFirewallAssignment,
  Service, QoSPolicy, DnsZone, DnsPolicy, VpnTunnel, VpnNetwork,
  MirrorSession, NatRule, NatPool, NatGatewayConfig,
  MonitorPolicy, NetworkMetrics, BandwidthAlert, SecurityIdentity,
} from '../api/network-security'
import {
  PoliciesTab, CreatePolicyModal, EditPolicyModal,
  FirewallTab, CreateFirewallProfileModal, EditFirewallProfileModal, CreateFirewallZoneModal, CreateVMFirewallAssignmentModal,
  ServicesTab, CreateServiceModal, EditServiceModal,
  QosTab, CreateQosModal, EditQosModal,
  DnsTab, CreateDnsZoneModal, CreateDnsPolicyModal, EditDnsZoneModal, EditDnsPolicyModal,
  VpnTab, CreateVpnTunnelModal, CreateVpnNetworkModal, EditVpnTunnelModal,
  MirrorTab, CreateMirrorModal, EditMirrorModal,
  NatTab, CreateNatRuleModal, CreateNatPoolModal, CreateNatGatewayModal, EditNatRuleModal,
  MonitorTab, CreateMonitorPolicyModal, EditMonitorPolicyModal,
} from './network-security'
import { extractErrorMessage } from './network/ModalShared'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { useToastContext } from '../contexts/ToastContext'
import { useSilentPoll } from '../hooks/useSilentPoll'
import { usePermissions } from '../hooks/usePermissions'
import { ReadOnlyProvider } from '../contexts/ReadOnlyContext'
import ReadOnlyNotice from '../components/ReadOnlyNotice'

type Tab = 'policies' | 'firewall' | 'services' | 'qos' | 'dns' | 'vpn' | 'mirror' | 'nat' | 'monitor'
type Modal =
  | 'policy' | 'firewall-profile' | 'firewall-zone' | 'firewall-assignment' | 'service' | 'qos'
  | 'dns-zone' | 'dns-policy' | 'vpn-tunnel' | 'vpn-network'
  | 'mirror' | 'nat-rule' | 'nat-pool' | 'nat-gateway' | 'monitor'
  | 'edit-policy' | 'edit-firewall-profile' | 'edit-service' | 'edit-qos'
  | 'edit-dns-zone' | 'edit-dns-policy' | 'edit-vpn-tunnel'
  | 'edit-mirror' | 'edit-nat-rule' | 'edit-monitor'
  | null

export default function NetworkSecurity() {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [activeTab, setActiveTab] = useState<Tab>('policies')

  // Data state
  const [policies, setPolicies] = useState<NetworkPolicy[]>([])
  const [identities, setIdentities] = useState<SecurityIdentity[]>([])
  const [fwProfiles, setFwProfiles] = useState<FirewallProfile[]>([])
  const [fwZones, setFwZones] = useState<FirewallZone[]>([])
  const [fwAssignments, setFwAssignments] = useState<VMFirewallAssignment[]>([])
  const [services, setServices] = useState<Service[]>([])
  const [qosPolicies, setQosPolicies] = useState<QoSPolicy[]>([])
  const [dnsZones, setDnsZones] = useState<DnsZone[]>([])
  const [dnsPolicies, setDnsPolicies] = useState<DnsPolicy[]>([])
  const [vpnTunnels, setVpnTunnels] = useState<VpnTunnel[]>([])
  const [vpnNetworks, setVpnNetworks] = useState<VpnNetwork[]>([])
  const [mirrorSessions, setMirrorSessions] = useState<MirrorSession[]>([])
  const [natRules, setNatRules] = useState<NatRule[]>([])
  const [natPools, setNatPools] = useState<NatPool[]>([])
  const [natGateways, setNatGateways] = useState<NatGatewayConfig[]>([])
  const [monitorPolicies, setMonitorPolicies] = useState<MonitorPolicy[]>([])
  const [metrics, setMetrics] = useState<NetworkMetrics[]>([])
  const [alerts, setAlerts] = useState<BandwidthAlert[]>([])

  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [activeModal, setActiveModal] = useState<Modal>(null)
  const [editingId, setEditingId] = useState<string | null>(null)
  const { confirmState, confirm, cancel } = useConfirm()

  const openEdit = (modal: Modal, id: string) => { setEditingId(id); setActiveModal(modal) }

  const fetchAll = useCallback(async (silent = false) => {
    if (!silent) {
      setLoading(true)
      setError(null)
    }
    try {
      const [
        pol, ids, fwp, fwz, fwa, svc, qos, dz, dp,
        vt, vn, ms, nr, np, ng, mp, met, al,
      ] = await Promise.all([
        api.listNetworkPolicies().catch(() => []),
        api.listIdentities().catch(() => []),
        api.listFirewallProfiles().catch(() => []),
        api.listFirewallZones().catch(() => []),
        api.listVMFirewallAssignments().catch(() => []),
        api.listServices().catch(() => []),
        api.listQosPolicies().catch(() => []),
        api.listDnsZones().catch(() => []),
        api.listDnsPolicies().catch(() => []),
        api.listVpnTunnels().catch(() => []),
        api.listVpnNetworks().catch(() => []),
        api.listMirrorSessions().catch(() => []),
        api.listNatRules().catch(() => []),
        api.listNatPools().catch(() => []),
        api.listNatGateways().catch(() => []),
        api.listMonitorPolicies().catch(() => []),
        api.listNetworkMetrics().catch(() => []),
        api.listBandwidthAlerts().catch(() => []),
      ])
      setPolicies(pol)
      setIdentities(ids)
      setFwProfiles(fwp)
      setFwZones(fwz)
      setFwAssignments(fwa)
      setServices(svc)
      setQosPolicies(qos)
      setDnsZones(dz)
      setDnsPolicies(dp)
      setVpnTunnels(vt)
      setVpnNetworks(vn)
      setMirrorSessions(ms)
      setNatRules(nr)
      setNatPools(np)
      setNatGateways(ng)
      setMonitorPolicies(mp)
      setMetrics(met)
      setAlerts(al)
    } catch (e: unknown) {
      if (!silent) {
        const msg = formatUserError(e)
        setError(msg)
        toastFailure(toast, 'Failed to load network security', e)
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

  // ─── Delete handlers ─────────────────────────────────────────────────────

  const handleDeletePolicy = async (id: string) => {
    if (!await confirm('Delete Policy', 'Delete this network policy?')) return
    try { await api.deleteNetworkPolicy(id); setPolicies(prev => prev.filter(p => p.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete network policy', e) }
  }

  const handleDeleteFwProfile = async (id: string) => {
    if (!await confirm('Delete Profile', 'Delete this firewall profile?')) return
    try { await api.deleteFirewallProfile(id); setFwProfiles(prev => prev.filter(p => p.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete firewall profile', e) }
  }

  const handleDeleteFwZone = async (id: string) => {
    if (!await confirm('Delete Zone', 'Delete this firewall zone?')) return
    try { await api.deleteFirewallZone(id); setFwZones(prev => prev.filter(z => z.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete firewall zone', e) }
  }

  const handleDeleteFwAssignment = async (vmName: string) => {
    if (!await confirm('Delete Assignment', 'Remove this VM firewall assignment?')) return
    try { await api.deleteVMFirewallAssignment(vmName); setFwAssignments(prev => prev.filter(a => a.vm_name !== vmName)) }
    catch (e: unknown) { failAction('Failed to remove firewall assignment', e) }
  }

  const handleAdoptNatRule = async (id: string) => {
    try {
      const adopted = await api.adoptNatRule(id)
      setNatRules(prev => [...prev.filter(r => r.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt NAT rule', e) }
  }

  const handleAdoptService = async (id: string) => {
    try {
      const adopted = await api.adoptService(id)
      setServices(prev => [...prev.filter(s => s.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt host listener', e) }
  }

  const handleAdoptFwZone = async (id: string) => {
    try {
      const adopted = await api.adoptFirewallZone(id)
      setFwZones(prev => [...prev.filter(z => z.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt firewalld zone', e) }
  }

  const handleAdoptFwProfile = async (id: string) => {
    try {
      const adopted = await api.adoptFirewallProfile(id)
      setFwProfiles(prev => [...prev.filter(p => p.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt firewall profile', e) }
  }

  const handleAdoptQos = async (id: string) => {
    try {
      const adopted = await api.adoptQosPolicy(id)
      setQosPolicies(prev => [...prev.filter(p => p.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt QoS policy', e) }
  }

  const handleAdoptVpnTunnel = async (id: string) => {
    try {
      const adopted = await api.adoptVpnTunnel(id)
      setVpnTunnels(prev => [...prev.filter(t => t.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt WireGuard tunnel', e) }
  }

  const handleAdoptDnsZone = async (id: string) => {
    try {
      const adopted = await api.adoptDnsZone(id)
      setDnsZones(prev => [...prev.filter(z => z.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt DNS zone', e) }
  }

  const handleAdoptDnsPolicy = async (id: string) => {
    try {
      const adopted = await api.adoptDnsPolicy(id)
      setDnsPolicies(prev => [...prev.filter(p => p.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt DNS policy', e) }
  }

  const handleAdoptMirrorSession = async (id: string) => {
    try {
      const adopted = await api.adoptMirrorSession(id)
      setMirrorSessions(prev => [...prev.filter(s => s.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt mirror session', e) }
  }

  const handleAdoptMonitorPolicy = async (id: string) => {
    try {
      const adopted = await api.adoptMonitorPolicy(id)
      setMonitorPolicies(prev => [...prev.filter(p => p.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt monitor policy', e) }
  }

  const handleAdoptNetworkPolicy = async (id: string) => {
    try {
      const adopted = await api.adoptNetworkPolicy(id)
      setPolicies(prev => [...prev.filter(p => p.id !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt network policy', e) }
  }

  const handleAdoptIdentity = async (id: string) => {
    try {
      const adopted = await api.adoptIdentity(id)
      setIdentities(prev => [...prev.filter(i => String(i.id) !== id), adopted])
    } catch (e: unknown) { failAction('Failed to adopt identity', e) }
  }

  const handleDeleteService = async (id: string) => {
    if (!await confirm('Delete Service', 'Delete this service?')) return
    try { await api.deleteService(id); setServices(prev => prev.filter(s => s.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete service', e) }
  }

  const handleDeleteQos = async (id: string) => {
    if (!await confirm('Delete QoS Policy', 'Delete this QoS policy?')) return
    try { await api.deleteQosPolicy(id); setQosPolicies(prev => prev.filter(p => p.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete QoS policy', e) }
  }

  const handleDeleteDnsZone = async (id: string) => {
    if (!await confirm('Delete DNS Zone', 'Delete this DNS zone?')) return
    try { await api.deleteDnsZone(id); setDnsZones(prev => prev.filter(z => z.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete DNS zone', e) }
  }

  const handleDeleteDnsPolicy = async (id: string) => {
    if (!await confirm('Delete DNS Policy', 'Delete this DNS policy?')) return
    try { await api.deleteDnsPolicy(id); setDnsPolicies(prev => prev.filter(p => p.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete DNS policy', e) }
  }

  const handleDeleteVpnTunnel = async (id: string) => {
    if (!await confirm('Delete Tunnel', 'Delete this VPN tunnel?')) return
    try { await api.deleteVpnTunnel(id); setVpnTunnels(prev => prev.filter(t => t.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete VPN tunnel', e) }
  }

  const handleDeleteVpnNetwork = async (id: string) => {
    if (!await confirm('Delete Network', 'Delete this VPN network?')) return
    try { await api.deleteVpnNetwork(id); setVpnNetworks(prev => prev.filter(n => n.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete VPN network', e) }
  }

  const handleDeleteMirror = async (id: string) => {
    if (!await confirm('Delete Session', 'Delete this mirror session?')) return
    try { await api.deleteMirrorSession(id); setMirrorSessions(prev => prev.filter(s => s.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete mirror session', e) }
  }

  const handleDeleteNatRule = async (id: string) => {
    if (!await confirm('Delete NAT Rule', 'Delete this NAT rule?')) return
    try { await api.deleteNatRule(id); setNatRules(prev => prev.filter(r => r.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete NAT rule', e) }
  }

  const handleDeleteNatPool = async (id: string) => {
    if (!await confirm('Delete NAT Pool', 'Delete this NAT pool?')) return
    try { await api.deleteNatPool(id); setNatPools(prev => prev.filter(p => p.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete NAT pool', e) }
  }

  const handleDeleteNatGateway = async (id: string) => {
    if (!await confirm('Delete NAT Gateway', 'Delete this NAT gateway?')) return
    try { await api.deleteNatGateway(id); setNatGateways(prev => prev.filter(g => g.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete NAT gateway', e) }
  }

  const handleDeleteMonitorPolicy = async (id: string) => {
    if (!await confirm('Delete Monitor Policy', 'Delete this monitor policy?')) return
    try { await api.deleteMonitorPolicy(id); setMonitorPolicies(prev => prev.filter(p => p.id !== id)) }
    catch (e: unknown) { failAction('Failed to delete monitor policy', e) }
  }

  const handleAcknowledgeAlert = async (id: string) => {
    try {
      await api.acknowledgeBandwidthAlert(id)
      setAlerts(prev => prev.map(a => a.id === id ? { ...a, acknowledged: true } : a))
    } catch (e: unknown) { failAction('Failed to acknowledge alert', e) }
  }

  // ─── Sync handlers ───────────────────────────────────────────────────────

  const handleSync = async (syncFn: () => Promise<unknown>) => {
    try { await syncFn(); await fetchAll() }
    catch (e: unknown) { failAction('Sync failed', e) }
  }

  const closeModal = () => { setActiveModal(null); setEditingId(null) }

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: 'policies', label: 'Policies', icon: <ShieldCheck className="w-4 h-4" /> },
    { key: 'firewall', label: 'Firewall', icon: <Shield className="w-4 h-4" /> },
    { key: 'services', label: 'Services', icon: <Server className="w-4 h-4" /> },
    { key: 'qos', label: 'QoS', icon: <Gauge className="w-4 h-4" /> },
    { key: 'dns', label: 'DNS', icon: <Globe className="w-4 h-4" /> },
    { key: 'vpn', label: 'VPN', icon: <Wifi className="w-4 h-4" /> },
    { key: 'mirror', label: 'Mirror', icon: <Eye className="w-4 h-4" /> },
    { key: 'nat', label: 'NAT', icon: <ArrowUpDown className="w-4 h-4" /> },
    { key: 'monitor', label: 'Monitor', icon: <Activity className="w-4 h-4" /> },
  ]

  return (
    <ReadOnlyProvider readOnly={!canWrite}>
    <div className="space-y-6">
      {!canWrite && <ReadOnlyNotice />}
      <PageHeader
        title="Net Security"
        description="Policies, firewall, services, QoS, DNS, VPN, mirroring, NAT, and monitoring"
        onRefresh={() => fetchAll()}
        refreshing={loading}
      />

      {error && (
        <ErrorBanner
          title="Could not load network security"
          headline={error}
          onRetry={fetchAll}
        />
      )}

      {/* Stats */}
      <div className="grid grid-cols-3 md:grid-cols-5 lg:grid-cols-9 gap-3">
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Policies</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{policies.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Firewall</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{fwProfiles.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Services</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{services.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">QoS</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{qosPolicies.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">DNS</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{dnsZones.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">VPN</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{vpnTunnels.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Mirror</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{mirrorSessions.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">NAT</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{natRules.length}</div>
        </div>
        <div className="zf-panel-muted px-4 py-3">
          <div className="text-[var(--zf-muted)] text-xs mb-1">Monitor</div>
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{monitorPolicies.length}</div>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-[var(--zf-hairline)]">
        <div className="flex gap-1 overflow-x-auto">
          {tabs.map(t => (
            <button
              key={t.key}
              onClick={() => setActiveTab(t.key)}
              className={`flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition whitespace-nowrap ${
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
          {activeTab === 'policies' && (
            <PoliciesTab
              policies={policies}
              identities={identities}
              onDelete={handleDeletePolicy}
              onAdopt={handleAdoptNetworkPolicy}
              onAdoptIdentity={handleAdoptIdentity}
              onEdit={(id) => openEdit('edit-policy', id)}
              onCreate={() => setActiveModal('policy')}
              onSync={() => handleSync(api.syncNetworkPolicies)}
            />
          )}
          {activeTab === 'firewall' && (
            <FirewallTab
              profiles={fwProfiles} zones={fwZones} assignments={fwAssignments}
              onDeleteProfile={handleDeleteFwProfile} onDeleteZone={handleDeleteFwZone} onDeleteAssignment={handleDeleteFwAssignment}
              onAdoptProfile={handleAdoptFwProfile}
              onAdoptZone={handleAdoptFwZone}
              onEditProfile={(id) => openEdit('edit-firewall-profile', id)}
              onCreate={() => setActiveModal('firewall-profile')}
              onCreateZone={() => setActiveModal('firewall-zone')}
              onCreateAssignment={() => setActiveModal('firewall-assignment')}
              onSync={() => handleSync(api.syncFirewall)}
            />
          )}
          {activeTab === 'services' && (
            <ServicesTab services={services} onDelete={handleDeleteService} onAdopt={handleAdoptService} onEdit={(id) => openEdit('edit-service', id)} onCreate={() => setActiveModal('service')} onSync={() => handleSync(api.syncServices)} />
          )}
          {activeTab === 'qos' && (
            <QosTab policies={qosPolicies} onDelete={handleDeleteQos} onAdopt={handleAdoptQos} onEdit={(id) => openEdit('edit-qos', id)} onCreate={() => setActiveModal('qos')} onSync={() => handleSync(api.syncQos)} />
          )}
          {activeTab === 'dns' && (
            <DnsTab
              zones={dnsZones} policies={dnsPolicies}
              onDeleteZone={handleDeleteDnsZone} onDeletePolicy={handleDeleteDnsPolicy}
              onAdoptZone={handleAdoptDnsZone} onAdoptPolicy={handleAdoptDnsPolicy}
              onEditZone={(id) => openEdit('edit-dns-zone', id)}
              onEditPolicy={(id) => openEdit('edit-dns-policy', id)}
              onCreate={() => setActiveModal('dns-zone')} onSync={() => handleSync(api.syncDns)}
            />
          )}
          {activeTab === 'vpn' && (
            <VpnTab
              tunnels={vpnTunnels} networks={vpnNetworks}
              onDeleteTunnel={handleDeleteVpnTunnel} onDeleteNetwork={handleDeleteVpnNetwork}
              onAdoptTunnel={handleAdoptVpnTunnel}
              onEditTunnel={(id) => openEdit('edit-vpn-tunnel', id)}
              onCreate={() => setActiveModal('vpn-tunnel')} onSync={() => handleSync(api.syncVpn)}
            />
          )}
          {activeTab === 'mirror' && (
            <MirrorTab
              sessions={mirrorSessions}
              onDelete={handleDeleteMirror}
              onAdopt={handleAdoptMirrorSession}
              onEdit={(id) => openEdit('edit-mirror', id)}
              onCreate={() => setActiveModal('mirror')}
              onSync={() => handleSync(api.syncMirror)}
            />
          )}
          {activeTab === 'nat' && (
            <NatTab
              rules={natRules} pools={natPools} gateways={natGateways}
              onDeleteRule={handleDeleteNatRule} onDeletePool={handleDeleteNatPool} onDeleteGateway={handleDeleteNatGateway}
              onAdoptRule={handleAdoptNatRule}
              onEditRule={(id) => openEdit('edit-nat-rule', id)}
              onCreate={() => setActiveModal('nat-rule')} onSync={() => handleSync(api.syncNat)}
            />
          )}
          {activeTab === 'monitor' && (
            <MonitorTab
              policies={monitorPolicies} metrics={metrics} alerts={alerts}
              onDelete={handleDeleteMonitorPolicy} onAdopt={handleAdoptMonitorPolicy}
              onEdit={(id) => openEdit('edit-monitor', id)}
              onAcknowledge={handleAcknowledgeAlert}
              onCreate={() => setActiveModal('monitor')} onSync={() => handleSync(api.syncMonitor)}
            />
          )}
        </>
      )}

      {/* Modals */}
      {canWrite && activeModal === 'policy' && <CreatePolicyModal onClose={closeModal} onCreated={(p) => { setPolicies(prev => [...prev, p]); closeModal() }} />}
      {canWrite && activeModal === 'firewall-profile' && <CreateFirewallProfileModal onClose={closeModal} onCreated={(p) => { setFwProfiles(prev => [...prev, p]); closeModal() }} />}
      {canWrite && activeModal === 'firewall-zone' && <CreateFirewallZoneModal profiles={fwProfiles} onClose={closeModal} onCreated={(z) => { setFwZones(prev => [...prev, z]); closeModal() }} />}
      {canWrite && activeModal === 'firewall-assignment' && <CreateVMFirewallAssignmentModal profiles={fwProfiles} zones={fwZones} onClose={closeModal} onCreated={(a) => { setFwAssignments(prev => [...prev.filter(x => x.vm_name !== a.vm_name), a]); closeModal() }} />}
      {canWrite && activeModal === 'service' && <CreateServiceModal onClose={closeModal} onCreated={(s) => { setServices(prev => [...prev, s]); closeModal() }} />}
      {canWrite && activeModal === 'qos' && <CreateQosModal onClose={closeModal} onCreated={(p) => { setQosPolicies(prev => [...prev, p]); closeModal() }} />}
      {canWrite && activeModal === 'dns-zone' && <CreateDnsZoneModal onClose={closeModal} onCreated={(z) => { setDnsZones(prev => [...prev, z]); closeModal() }} />}
      {canWrite && activeModal === 'dns-policy' && <CreateDnsPolicyModal zones={dnsZones} onClose={closeModal} onCreated={(p) => { setDnsPolicies(prev => [...prev, p]); closeModal() }} />}
      {canWrite && activeModal === 'vpn-tunnel' && <CreateVpnTunnelModal onClose={closeModal} onCreated={(t) => { setVpnTunnels(prev => [...prev, t]); closeModal() }} />}
      {canWrite && activeModal === 'vpn-network' && <CreateVpnNetworkModal onClose={closeModal} onCreated={(n) => { setVpnNetworks(prev => [...prev, n]); closeModal() }} />}
      {canWrite && activeModal === 'mirror' && <CreateMirrorModal onClose={closeModal} onCreated={(s) => { setMirrorSessions(prev => [...prev, s]); closeModal() }} />}
      {canWrite && activeModal === 'nat-rule' && <CreateNatRuleModal onClose={closeModal} onCreated={(r) => { setNatRules(prev => [...prev, r]); closeModal() }} />}
      {canWrite && activeModal === 'nat-pool' && <CreateNatPoolModal onClose={closeModal} onCreated={(p) => { setNatPools(prev => [...prev, p]); closeModal() }} />}
      {canWrite && activeModal === 'nat-gateway' && <CreateNatGatewayModal onClose={closeModal} onCreated={(g) => { setNatGateways(prev => [...prev, g]); closeModal() }} />}
      {canWrite && activeModal === 'monitor' && <CreateMonitorPolicyModal onClose={closeModal} onCreated={(p) => { setMonitorPolicies(prev => [...prev, p]); closeModal() }} />}
      {canWrite && activeModal === 'edit-policy' && editingId && (
        <EditPolicyModal id={editingId} onClose={closeModal} onUpdated={(p) => { setPolicies(prev => prev.map(x => x.id === p.id ? p : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-firewall-profile' && editingId && (
        <EditFirewallProfileModal id={editingId} onClose={closeModal} onUpdated={(p) => { setFwProfiles(prev => prev.map(x => x.id === p.id ? p : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-service' && editingId && (
        <EditServiceModal id={editingId} onClose={closeModal} onUpdated={(s) => { setServices(prev => prev.map(x => x.id === s.id ? s : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-qos' && editingId && (
        <EditQosModal id={editingId} onClose={closeModal} onUpdated={(p) => { setQosPolicies(prev => prev.map(x => x.id === p.id ? p : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-dns-zone' && editingId && (
        <EditDnsZoneModal id={editingId} onClose={closeModal} onUpdated={(z) => { setDnsZones(prev => prev.map(x => x.id === z.id ? z : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-dns-policy' && editingId && (
        <EditDnsPolicyModal id={editingId} zones={dnsZones} onClose={closeModal} onUpdated={(p) => { setDnsPolicies(prev => prev.map(x => x.id === p.id ? p : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-vpn-tunnel' && editingId && (
        <EditVpnTunnelModal id={editingId} onClose={closeModal} onUpdated={(t) => { setVpnTunnels(prev => prev.map(x => x.id === t.id ? t : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-mirror' && editingId && (
        <EditMirrorModal id={editingId} onClose={closeModal} onUpdated={(s) => { setMirrorSessions(prev => prev.map(x => x.id === s.id ? s : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-nat-rule' && editingId && (
        <EditNatRuleModal id={editingId} onClose={closeModal} onUpdated={(r) => { setNatRules(prev => prev.map(x => x.id === r.id ? r : x)); closeModal() }} />
      )}
      {canWrite && activeModal === 'edit-monitor' && editingId && (
        <EditMonitorPolicyModal id={editingId} onClose={closeModal} onUpdated={(p) => { setMonitorPolicies(prev => prev.map(x => x.id === p.id ? p : x)); closeModal() }} />
      )}
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
