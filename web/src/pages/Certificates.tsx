// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react'
import { Plus, Check, Shield, RefreshCw, ClipboardCheck } from 'lucide-react'
import {
  listCas,
  createCa,
  listCertificates,
  issueCertificate,
  revokeCertificate,
  renewCertificate,
  listCertRequests,
  approveCertRequest,
  listAttestations,
  listSecurityBaselines,
  createSecurityBaseline,
  checkVmSecurityCompliance,
  getCertHealthDashboard,
  type CertificateAuthority,
  type Certificate,
  type CertificateRequest,
  type TrustAttestation,
  type VmSecurityBaseline,
  type CertHealthDashboard,
} from '../api/certificates'
import { listVMs, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import RelativeTime from '../components/RelativeTime'

export default function Certificates() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [cas, setCAs] = useState<CertificateAuthority[]>([])
  const [certs, setCerts] = useState<Certificate[]>([])
  const [csrs, setCSRs] = useState<CertificateRequest[]>([])
  const [attestations, setAttestations] = useState<TrustAttestation[]>([])
  const [baselines, setBaselines] = useState<VmSecurityBaseline[]>([])
  const [health, setHealth] = useState<CertHealthDashboard | null>(null)
  const [vms, setVMs] = useState<VM[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load certificates')
  const [activeTab, setActiveTab] = useState<'dashboard' | 'cas' | 'certs' | 'csrs' | 'attestation' | 'baselines'>('dashboard')
  const [showCreateCA, setShowCreateCA] = useState(false)
  const [showCreateBaseline, setShowCreateBaseline] = useState(false)
  const [showIssueCert, setShowIssueCert] = useState(false)
  const [complianceBaseline, setComplianceBaseline] = useState<VmSecurityBaseline | null>(null)

  const loadData = useCallback(() => {
    return run(async () => {
      const [ca, ct, cr, at, bl, hl, vm] = await Promise.all([
        listCas(),
        listCertificates(),
        listCertRequests(),
        listAttestations(),
        listSecurityBaselines(),
        getCertHealthDashboard().catch(() => null),
        listVMs(),
      ])
      setCAs(ca)
      setCerts(ct)
      setCSRs(cr)
      setAttestations(at)
      setBaselines(bl)
      setHealth(hl)
      setVMs(vm)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleRevoke = async (id: string) => {
    const ok = await confirm('Revoke Certificate', 'Revoke this certificate?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return
    try { await revokeCertificate(id); toast.success('Certificate revoked'); loadData() }
    catch { toast.error('Failed to revoke certificate') }
  }

  const handleApproveCSR = async (id: string) => {
    try { await approveCertRequest(id); toast.success('Request approved'); loadData() }
    catch { toast.error('Failed to approve request') }
  }

  const handleRenew = async (id: string, subject: string) => {
    try { await renewCertificate(id); toast.success(`Renewed certificate for '${subject}'`); loadData() }
    catch { toast.error('Failed to renew certificate') }
  }

  const getStatusColor = (status: string) => {
    const m: Record<string, string> = {
      active: 'text-emerald-700 bg-emerald-50 border-emerald-200', expired: 'text-red-700 bg-red-50 border-red-200',
      revoked: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]', expiring_soon: 'text-amber-800 bg-amber-50 border-amber-200',
      pending: 'text-amber-800 bg-amber-50 border-amber-200', approved: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      rejected: 'text-red-700 bg-red-50 border-red-200', trusted: 'text-emerald-700 bg-emerald-50 border-emerald-200',
      untrusted: 'text-red-700 bg-red-50 border-red-200', unknown: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
    }
    return m[status] || 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }

  const daysUntil = (date: string) => {
    const diff = new Date(date).getTime() - Date.now()
    return Math.ceil(diff / (1000 * 60 * 60 * 24))
  }


  return (
    <div className="p-6">
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Certificates & Security"
        description="Certificate authorities, issuance, and host security baselines"
      />

      <PageLoadBanner title="Could not load certificates" headline={loadError} onRetry={() => void loadData()} />

      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-[var(--zf-surface)] rounded-lg p-1 overflow-x-auto">
        {(['dashboard', 'cas', 'certs', 'csrs', 'attestation', 'baselines'] as const).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)}
            className={`px-4 py-2 rounded text-sm font-medium whitespace-nowrap ${activeTab === tab ? 'bg-[var(--zf-link-fill)] text-white' : 'text-[var(--zf-muted)] hover:bg-black/[0.04]'}`}>
            {tab === 'cas' ? 'CAs' : tab === 'certs' ? 'Certificates' : tab === 'csrs' ? 'Requests' :
             tab === 'attestation' ? 'Attestation' : tab === 'baselines' ? 'Security Baselines' : 'Dashboard'}
          </button>
        ))}
      </div>

      {/* Dashboard */}
      {activeTab === 'dashboard' && !health && !loading && (
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg p-8 text-center text-[var(--zf-muted)]">
          No certificate health data available.
        </div>
      )}

      {activeTab === 'dashboard' && health && (
        <div>
          <div className="grid grid-cols-1 md:grid-cols-4 gap-3 mb-4">
            <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Total Certificates</div>
              <div className="text-2xl font-bold">{health.total_certificates}</div>
            </div>
            <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Active</div>
              <div className="text-2xl font-bold text-emerald-600">{health.active}</div>
            </div>
            <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Expiring Soon</div>
              <div className="text-2xl font-bold text-amber-600">{health.expiring_soon}</div>
            </div>
            <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Expired</div>
              <div className="text-2xl font-bold text-red-600">{health.expired}</div>
            </div>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
            <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Certificate Authorities</div>
              <div className="text-2xl font-bold">{health.ca_count}</div>
            </div>
            <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Pending Requests</div>
              <div className="text-2xl font-bold text-[var(--zf-link)]">{health.pending_requests}</div>
            </div>
            <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-4 py-3">
              <div className="text-[var(--zf-muted)] text-sm mb-1">Overall Compliance</div>
              <div className="text-2xl font-bold">{health.overall_compliance_pct.toFixed(0)}%</div>
            </div>
          </div>
          {health.expiring_within_30_days.length > 0 && (
            <div className="bg-amber-50 border border-amber-200 rounded-lg p-4">
              <h3 className="text-lg font-semibold mb-3 text-amber-800">Expiring Within 30 Days</h3>
              <div className="space-y-2">
                {health.expiring_within_30_days.map(c => (
                  <div key={c.cert_id} className="flex items-center justify-between p-2 bg-[var(--zf-surface)] rounded">
                    <span className="font-mono text-sm">{c.subject}</span>
                    <span className="text-sm text-amber-700">{c.days_remaining} days left</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* CAs Tab */}
      {activeTab === 'cas' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateCA(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create CA
            </button>
          </div>
          <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">Type</th>
                  <th className="p-4">Subject</th>
                  <th className="p-4">Valid Until</th>
                  <th className="p-4">Issued</th>
                  <th className="p-4">Status</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {cas.length === 0 ? (
                  <tr><td colSpan={6} className="p-8 text-center text-[var(--zf-muted)]">No Certificate Authorities.</td></tr>
                ) : cas.map(ca => {
                  const days = daysUntil(ca.valid_to)
                  return (
                    <tr key={ca.id} className="hover:bg-[var(--zf-surface)]">
                      <td className="p-4 font-medium">{ca.name}</td>
                      <td className="p-4 text-sm">{ca.ca_type}</td>
                      <td className="p-4 text-sm font-mono text-[var(--zf-muted)]">{ca.subject}</td>
                      <td className="p-4 text-sm">
                        <span className={days < 30 ? 'text-red-600' : days < 90 ? 'text-amber-600' : 'text-[var(--zf-muted)]'}>
                          {new Date(ca.valid_to).toLocaleDateString()} ({days}d)
                        </span>
                      </td>
                      <td className="p-4 text-sm">{ca.issued_count}</td>
                      <td className="p-4"><span className={`px-2 py-1 rounded text-xs font-medium border ${getStatusColor(ca.status)}`}>{ca.status}</span></td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Certificates Tab */}
      {activeTab === 'certs' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowIssueCert(true)} disabled={cas.length === 0}
              title={cas.length === 0 ? 'Create a Certificate Authority first' : undefined}
              className="zf-btn zf-btn-primary disabled:opacity-50 disabled:cursor-not-allowed">
              <Plus className="w-4 h-4" /> Issue Certificate
            </button>
          </div>
          <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Subject</th>
                  <th className="p-4">Type</th>
                  <th className="p-4">Serial</th>
                  <th className="p-4">Expires</th>
                  <th className="p-4">Host/Service</th>
                  <th className="p-4">Status</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {certs.length === 0 ? (
                  <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No certificates.</td></tr>
                ) : certs.map(cert => {
                  const days = daysUntil(cert.valid_to)
                  return (
                    <tr key={cert.id} className="hover:bg-[var(--zf-surface)]">
                      <td className="p-4 font-mono text-sm">{cert.subject}</td>
                      <td className="p-4 text-sm">{cert.cert_type}</td>
                      <td className="p-4 text-xs font-mono text-[var(--zf-muted)]">{cert.serial_number}</td>
                      <td className="p-4 text-sm">
                        <span className={days < 0 ? 'text-red-600' : days < 30 ? 'text-amber-600' : 'text-[var(--zf-muted)]'}>
                          {new Date(cert.valid_to).toLocaleDateString()} ({days}d)
                        </span>
                      </td>
                      <td className="p-4 text-sm text-[var(--zf-muted)]">{cert.host_name || cert.service_name || '-'}</td>
                      <td className="p-4"><span className={`px-2 py-1 rounded text-xs font-medium border ${getStatusColor(cert.status)}`}>{cert.status.replace(/_/g, ' ')}</span></td>
                      <td className="p-4">
                        <div className="flex items-center gap-3">
                          {(cert.status === 'active' || cert.status === 'expiring_soon') && (
                            <button onClick={() => handleRenew(cert.id, cert.subject)} className="flex items-center gap-1 text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm">
                              <RefreshCw className="w-3.5 h-3.5" /> Renew
                            </button>
                          )}
                          {cert.status === 'active' && (
                            <button onClick={() => handleRevoke(cert.id)} className="text-red-600 hover:text-red-800 text-sm">Revoke</button>
                          )}
                        </div>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* CSR Queue Tab */}
      {activeTab === 'csrs' && (
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">Subject</th>
                <th className="p-4">Requestor</th>
                <th className="p-4">Type</th>
                <th className="p-4">Key Size</th>
                <th className="p-4">Submitted</th>
                <th className="p-4">Status</th>
                <th className="p-4">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {csrs.length === 0 ? (
                <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No certificate requests in queue.</td></tr>
              ) : csrs.map(csr => (
                <tr key={csr.id} className="hover:bg-[var(--zf-surface)]">
                  <td className="p-4 font-mono text-sm">{csr.subject}</td>
                  <td className="p-4 text-sm">{csr.requestor}</td>
                  <td className="p-4 text-sm">{csr.cert_type}</td>
                  <td className="p-4 text-sm">{csr.key_size} bits</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]"><RelativeTime date={csr.submitted_at} /></td>
                  <td className="p-4"><span className={`px-2 py-1 rounded text-xs font-medium border ${getStatusColor(csr.status)}`}>{csr.status}</span></td>
                  <td className="p-4">
                    {csr.status === 'pending' && (
                      <div className="flex items-center gap-2">
                        <button onClick={() => handleApproveCSR(csr.id)} className="text-emerald-600 hover:text-emerald-700 p-1">
                          <Check className="w-4 h-4" />
                        </button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Host Attestation Tab */}
      {activeTab === 'attestation' && (
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg">
          <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
            <thead>
              <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                <th className="p-4">Host</th>
                <th className="p-4">TPM</th>
                <th className="p-4">TPM Version</th>
                <th className="p-4">Attestation</th>
                <th className="p-4">Boot Integrity</th>
                <th className="p-4">Secure Boot</th>
                <th className="p-4">Last Check</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]">
              {attestations.length === 0 ? (
                <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No host attestation data.</td></tr>
              ) : attestations.map(att => (
                <tr key={att.id} className="hover:bg-[var(--zf-surface)]">
                  <td className="p-4 font-medium">{att.hostname}</td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded text-xs font-medium border ${att.tpm_present ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-red-700 bg-red-50 border-red-200'}`}>
                      {att.tpm_present ? 'Present' : 'Absent'}
                    </span>
                  </td>
                  <td className="p-4 text-sm">{att.tpm_version || '-'}</td>
                  <td className="p-4"><span className={`px-2 py-1 rounded text-xs font-medium border ${getStatusColor(att.attestation_status)}`}>{att.attestation_status}</span></td>
                  <td className="p-4">
                    <span className={`px-2 py-1 rounded text-xs font-medium border ${att.boot_integrity ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-red-700 bg-red-50 border-red-200'}`}>
                      {att.boot_integrity ? 'Verified' : 'Failed'}
                    </span>
                  </td>
                  <td className="p-4 text-sm">{att.secure_boot_enabled ? 'Enabled' : 'Disabled'}</td>
                  <td className="p-4 text-sm text-[var(--zf-muted)]">{att.last_attestation ? <RelativeTime date={att.last_attestation} /> : 'Never'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Security Baselines Tab */}
      {activeTab === 'baselines' && (
        <div>
          <div className="flex justify-end mb-4">
            <button onClick={() => setShowCreateBaseline(true)}
              className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" /> Create Baseline
            </button>
          </div>
          <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg">
            <table className="min-w-full divide-y divide-[var(--zf-hairline)]">
              <thead>
                <tr className="text-left text-xs text-[var(--zf-muted)] uppercase">
                  <th className="p-4">Name</th>
                  <th className="p-4">VMs</th>
                  <th className="p-4">Compliant</th>
                  <th className="p-4">Checks</th>
                  <th className="p-4">Compliance</th>
                  <th className="p-4">Last Scan</th>
                  <th className="p-4">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--zf-hairline)]">
                {baselines.length === 0 ? (
                  <tr><td colSpan={7} className="p-8 text-center text-[var(--zf-muted)]">No security baselines.</td></tr>
                ) : baselines.map(bl => {
                  const checkPct = bl.vm_count > 0 ? (bl.compliant_count / bl.vm_count * 100) : 0
                  return (
                    <tr key={bl.id} className="hover:bg-[var(--zf-surface)]">
                      <td className="p-4">
                        <div className="flex items-center gap-2">
                          <Shield className="w-4 h-4 text-[var(--zf-link)]" />
                          <div>
                            <div className="font-medium">{bl.name}</div>
                            {bl.description && <div className="text-xs text-[var(--zf-muted)]">{bl.description}</div>}
                          </div>
                        </div>
                      </td>
                      <td className="p-4 text-sm">{bl.vm_count}</td>
                      <td className="p-4 text-sm text-emerald-600">{bl.compliant_count}</td>
                      <td className="p-4 text-sm">{bl.checks.length}</td>
                      <td className="p-4">
                        <div className="flex items-center gap-2">
                          <div className="w-20 bg-[var(--zf-canvas)] rounded-full h-2">
                            <div className={`h-2 rounded-full ${checkPct === 100 ? 'bg-emerald-500' : checkPct > 70 ? 'bg-amber-500' : 'bg-red-500'}`}
                              style={{ width: `${checkPct}%` }} />
                          </div>
                          <span className="text-xs text-[var(--zf-muted)]">{checkPct.toFixed(0)}%</span>
                        </div>
                      </td>
                      <td className="p-4 text-sm text-[var(--zf-muted)]">{bl.last_scan ? new Date(bl.last_scan).toLocaleDateString() : 'Never'}</td>
                      <td className="p-4">
                        <button onClick={() => setComplianceBaseline(bl)} disabled={vms.length === 0}
                          title={vms.length === 0 ? 'No VMs to check' : undefined}
                          className="flex items-center gap-1 text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] text-sm disabled:opacity-50 disabled:cursor-not-allowed">
                          <ClipboardCheck className="w-3.5 h-3.5" /> Check Compliance
                        </button>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Modals */}
      {showCreateCA && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-[var(--zf-surface)] rounded-lg p-6 w-full max-w-md">
            <h2 className="text-xl font-bold mb-4">Create Certificate Authority</h2>
            <CreateCAForm onClose={() => setShowCreateCA(false)} onCreated={() => { setShowCreateCA(false); loadData() }} />
          </div>
        </div>
      )}
      {showCreateBaseline && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-[var(--zf-surface)] rounded-lg p-6 w-full max-w-md">
            <h2 className="text-xl font-bold mb-4">Create Security Baseline</h2>
            <CreateBaselineForm onClose={() => setShowCreateBaseline(false)} onCreated={() => { setShowCreateBaseline(false); loadData() }} />
          </div>
        </div>
      )}
      {showIssueCert && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-[var(--zf-surface)] rounded-lg p-6 w-full max-w-md">
            <h2 className="text-xl font-bold mb-4">Issue Certificate</h2>
            <IssueCertificateForm cas={cas} onClose={() => setShowIssueCert(false)} onIssued={() => { setShowIssueCert(false); loadData() }} />
          </div>
        </div>
      )}
      {complianceBaseline && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-[var(--zf-surface)] rounded-lg p-6 w-full max-w-md">
            <h2 className="text-xl font-bold mb-4">Check Compliance: {complianceBaseline.name}</h2>
            <ComplianceCheckForm baseline={complianceBaseline} vms={vms} onClose={() => setComplianceBaseline(null)} onChecked={loadData} />
          </div>
        </div>
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

function CreateCAForm({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [caType, setCaType] = useState<'root' | 'intermediate' | 'external'>('root')
  const [subject, setSubject] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try { await createCa({ name, ca_type: caType, subject }); onCreated() }
    catch { toast.error('Failed to create CA') }
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div><label className="block text-sm font-medium mb-1">Name</label>
        <input type="text" value={name} onChange={e => setName(e.target.value)} className="input-field" required /></div>
      <div><label className="block text-sm font-medium mb-1">Type</label>
        <select value={caType} onChange={e => setCaType(e.target.value as 'root' | 'intermediate' | 'external')} className="input-field">
          <option value="root">Root</option><option value="intermediate">Intermediate</option><option value="external">External</option>
        </select></div>
      <div><label className="block text-sm font-medium mb-1">Subject</label>
        <input type="text" value={subject} onChange={e => setSubject(e.target.value)} className="input-field" placeholder="CN=My CA, O=My Org" required /></div>
      <div className="flex gap-3">
        <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
        <button type="submit" className="flex-1 zf-btn zf-btn-primary">Create</button>
      </div>
    </form>
  )
}

function CreateBaselineForm({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      await createSecurityBaseline({
        name,
        description: description || undefined,
        checks: [{ check_id: 'default', name: 'Default Check', category: 'general', severity: 'medium' }],
      })
      onCreated()
    } catch { toast.error('Failed to create baseline') }
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div><label className="block text-sm font-medium mb-1">Name</label>
        <input type="text" value={name} onChange={e => setName(e.target.value)} className="input-field" required /></div>
      <div><label className="block text-sm font-medium mb-1">Description</label>
        <input type="text" value={description} onChange={e => setDescription(e.target.value)} className="input-field" /></div>
      <div className="flex gap-3">
        <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
        <button type="submit" className="flex-1 zf-btn zf-btn-primary">Create</button>
      </div>
    </form>
  )
}

function IssueCertificateForm({ cas, onClose, onIssued }: { cas: CertificateAuthority[]; onClose: () => void; onIssued: () => void }) {
  const toast = useToastContext()
  const [caId, setCaId] = useState(cas[0]?.id || '')
  const [subject, setSubject] = useState('')
  const [certType, setCertType] = useState<'server' | 'client' | 'host' | 'service'>('server')
  const [validityDays, setValidityDays] = useState(365)
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSubmitting(true)
    try {
      await issueCertificate({ ca_id: caId, subject, cert_type: certType, validity_days: validityDays })
      toast.success('Certificate issued')
      onIssued()
    } catch { toast.error('Failed to issue certificate') } finally { setSubmitting(false) }
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div><label className="block text-sm font-medium mb-1">Certificate Authority</label>
        <select value={caId} onChange={e => setCaId(e.target.value)} className="input-field">
          {cas.map(ca => <option key={ca.id} value={ca.id}>{ca.name}</option>)}
        </select></div>
      <div><label className="block text-sm font-medium mb-1">Subject</label>
        <input type="text" value={subject} onChange={e => setSubject(e.target.value)} className="input-field" placeholder="CN=vm01.internal" required /></div>
      <div><label className="block text-sm font-medium mb-1">Type</label>
        <select value={certType} onChange={e => setCertType(e.target.value as typeof certType)} className="input-field">
          <option value="server">Server</option><option value="client">Client</option><option value="host">Host</option><option value="service">Service</option>
        </select></div>
      <div><label className="block text-sm font-medium mb-1">Validity (days)</label>
        <input type="number" value={validityDays} onChange={e => setValidityDays(Math.max(1, Number(e.target.value) || 1))} min={1} className="input-field" /></div>
      <div className="flex gap-3">
        <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Cancel</button>
        <button type="submit" disabled={submitting} className="flex-1 zf-btn zf-btn-primary disabled:opacity-50">{submitting ? 'Issuing...' : 'Issue'}</button>
      </div>
    </form>
  )
}

function ComplianceCheckForm({ baseline, vms, onClose, onChecked }: { baseline: VmSecurityBaseline; vms: VM[]; onClose: () => void; onChecked: () => void }) {
  const toast = useToastContext()
  const [vmName, setVmName] = useState(vms[0]?.name || '')
  const [checking, setChecking] = useState(false)
  const [result, setResult] = useState<Awaited<ReturnType<typeof checkVmSecurityCompliance>> | null>(null)

  const handleCheck = async () => {
    if (!vmName) return
    setChecking(true)
    try {
      const res = await checkVmSecurityCompliance(baseline.id, vmName)
      setResult(res)
      toast[res.compliant ? 'success' : 'error'](`'${vmName}' is ${res.compliant ? 'compliant' : 'non-compliant'} with '${baseline.name}'`)
      onChecked()
    } catch { toast.error('Failed to run compliance check') } finally { setChecking(false) }
  }

  return (
    <div className="space-y-4">
      <div><label className="block text-sm font-medium mb-1">VM</label>
        <select value={vmName} onChange={e => { setVmName(e.target.value); setResult(null) }} className="input-field">
          {vms.map(v => <option key={v.name} value={v.name}>{v.name}</option>)}
        </select></div>
      {result && (
        <div className={`rounded-lg p-3 border ${result.compliant ? 'bg-emerald-50 border-emerald-200' : 'bg-red-50 border-red-200'}`}>
          <div className={`text-sm font-semibold mb-2 ${result.compliant ? 'text-emerald-600' : 'text-red-600'}`}>
            {result.compliant ? 'Compliant' : 'Non-compliant'}
          </div>
          <div className="space-y-1">
            {result.checks.map(c => (
              <div key={c.check_id} className="flex items-center justify-between text-xs">
                <span className="text-[var(--zf-ink)]">{c.name}</span>
                <span className={c.status === 'pass' ? 'text-emerald-600' : c.status === 'fail' ? 'text-red-600' : 'text-[var(--zf-muted)]'}>{c.status.replace(/_/g, ' ')}</span>
              </div>
            ))}
          </div>
        </div>
      )}
      <div className="flex gap-3">
        <button type="button" onClick={onClose} className="flex-1 zf-btn zf-btn-ghost">Close</button>
        <button type="button" onClick={handleCheck} disabled={checking || !vmName} className="flex-1 zf-btn zf-btn-primary disabled:opacity-50">{checking ? 'Checking...' : 'Run Check'}</button>
      </div>
    </div>
  )
}
