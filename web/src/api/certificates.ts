// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPostVoid } from './client'

export interface CertificateAuthority {
  id: string
  name: string
  ca_type: 'root' | 'intermediate' | 'external'
  subject: string
  issuer: string
  serial_number: string
  valid_from: string
  valid_to: string
  key_algorithm: string
  key_size: number
  status: 'active' | 'expired' | 'revoked'
  issued_count: number
  crl_url?: string
  ocsp_url?: string
  created: string
  updated?: string
}

export interface Certificate {
  id: string
  subject: string
  issuer: string
  serial_number: string
  ca_id: string
  cert_type: 'server' | 'client' | 'host' | 'service'
  valid_from: string
  valid_to: string
  key_algorithm: string
  key_size: number
  san?: string[]
  status: 'active' | 'expiring_soon' | 'expired' | 'revoked'
  host_name?: string
  service_name?: string
  fingerprint: string
  created: string
  updated?: string
}

export interface CertificateRequest {
  id: string
  subject: string
  requestor: string
  ca_id: string
  cert_type: 'server' | 'client' | 'host' | 'service'
  key_size: number
  san?: string[]
  status: 'pending' | 'approved' | 'rejected'
  submitted_at: string
  reviewed_at?: string
  reviewed_by?: string
  rejection_reason?: string
}

export interface TrustAttestation {
  id: string
  host_id: string
  hostname: string
  tpm_present: boolean
  tpm_version?: string
  attestation_status: 'trusted' | 'untrusted' | 'unknown'
  last_attestation?: string
  boot_integrity: boolean
  secure_boot_enabled: boolean
  measured_boot_log?: string
  created: string
  updated?: string
}

export interface VmSecurityBaseline {
  id: string
  name: string
  description?: string
  checks: Array<{
    check_id: string
    name: string
    category: string
    severity: 'critical' | 'high' | 'medium' | 'low'
    enabled: boolean
  }>
  vm_count: number
  compliant_count: number
  non_compliant_count: number
  last_scan?: string
  created: string
  updated?: string
}

export interface CertHealthDashboard {
  total_certificates: number
  active: number
  expiring_soon: number
  expired: number
  revoked: number
  pending_requests: number
  ca_count: number
  trusted_hosts: number
  untrusted_hosts: number
  security_baselines: number
  overall_compliance_pct: number
  expiring_within_30_days: Array<{
    cert_id: string
    subject: string
    valid_to: string
    days_remaining: number
  }>
}

const API_BASE = '/api/certificates'

export async function listCas(): Promise<CertificateAuthority[]> {
  return apiGet<CertificateAuthority[]>(`${API_BASE}/cas`)
}

export async function createCa(req: {
  name: string
  ca_type: 'root' | 'intermediate' | 'external'
  subject: string
  key_algorithm?: string
  key_size?: number
}): Promise<CertificateAuthority> {
  return apiPost<CertificateAuthority>(`${API_BASE}/cas`, req)
}

export async function listCertificates(_caId?: string): Promise<Certificate[]> {
  return apiGet<Certificate[]>(`${API_BASE}`)
}

export async function issueCertificate(req: {
  ca_id: string
  subject: string
  cert_type: 'server' | 'client' | 'host' | 'service'
  key_size?: number
  san?: string[]
  validity_days?: number
}): Promise<Certificate> {
  return apiPost<Certificate>(`${API_BASE}/issue`, req)
}

export async function revokeCertificate(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/${encodeURIComponent(id)}/revoke`)
}

export async function renewCertificate(id: string): Promise<Certificate> {
  return apiPost<Certificate>(`${API_BASE}/${encodeURIComponent(id)}/renew`)
}

export async function checkExpiring(days?: number): Promise<Certificate[]> {
  const url = days
    ? `${API_BASE}/expiring?days=${days}`
    : `${API_BASE}/expiring`
  return apiGet<Certificate[]>(url)
}

export async function listCertRequests(status?: string): Promise<CertificateRequest[]> {
  const url = status
    ? `${API_BASE}/requests?status=${status}`
    : `${API_BASE}/requests`
  return apiGet<CertificateRequest[]>(url)
}

export async function submitCertRequest(req: {
  subject: string
  ca_id: string
  cert_type: 'server' | 'client' | 'host' | 'service'
  key_size?: number
  san?: string[]
}): Promise<CertificateRequest> {
  return apiPost<CertificateRequest>(`${API_BASE}/requests`, req)
}

export async function approveCertRequest(id: string): Promise<Certificate> {
  return apiPost<Certificate>(`${API_BASE}/requests/${encodeURIComponent(id)}/approve`)
}

export async function listAttestations(): Promise<TrustAttestation[]> {
  return apiGet<TrustAttestation[]>(`${API_BASE}/attestations`)
}

export async function submitAttestation(req: {
  host_id: string
  tpm_present: boolean
  tpm_version?: string
  secure_boot_enabled?: boolean
  measured_boot_log?: string
}): Promise<TrustAttestation> {
  return apiPost<TrustAttestation>(`${API_BASE}/attestations`, req)
}

export async function listSecurityBaselines(): Promise<VmSecurityBaseline[]> {
  return apiGet<VmSecurityBaseline[]>(`${API_BASE}/security-baselines`)
}

export async function createSecurityBaseline(req: {
  name: string
  description?: string
  checks: Array<{
    check_id: string
    name: string
    category: string
    severity: 'critical' | 'high' | 'medium' | 'low'
    enabled?: boolean
  }>
}): Promise<VmSecurityBaseline> {
  return apiPost<VmSecurityBaseline>(`${API_BASE}/security-baselines`, req)
}

export async function checkVmSecurityCompliance(baselineId: string, vmId: string): Promise<{
  compliant: boolean
  checks: Array<{
    check_id: string
    name: string
    status: 'pass' | 'fail' | 'not_applicable'
    details?: string
  }>
  checked_at: string
}> {
  return apiPost<{
    compliant: boolean
    checks: Array<{
      check_id: string
      name: string
      status: 'pass' | 'fail' | 'not_applicable'
      details?: string
    }>
    checked_at: string
  }>(`${API_BASE}/security-baselines/${encodeURIComponent(baselineId)}/compliance`, { vm_id: vmId })
}

export async function getCertHealthDashboard(): Promise<CertHealthDashboard> {
  return apiGet<CertHealthDashboard>(`${API_BASE}/health`)
}
