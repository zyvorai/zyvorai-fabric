// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { describe, it, expect, vi, beforeEach } from 'vitest'

// ─── Mock client helpers ────────────────────────────────────────────────────────

vi.mock('../client', () => ({
  apiGet: vi.fn().mockResolvedValue({}),
  apiPost: vi.fn().mockResolvedValue({}),
  apiPostVoid: vi.fn().mockResolvedValue(undefined),
  apiPut: vi.fn().mockResolvedValue({}),
  apiPutVoid: vi.fn().mockResolvedValue(undefined),
  apiDelete: vi.fn().mockResolvedValue(undefined),
  apiFetch: vi.fn().mockResolvedValue({
    ok: true,
    json: () => Promise.resolve({}),
    blob: () => Promise.resolve(new Blob()),
  }),
  getToken: vi.fn().mockReturnValue('test-token'),
  setToken: vi.fn(),
  clearToken: vi.fn(),
}))

// Mock global fetch for auth.ts login()
const fetchMock = vi.fn()
globalThis.fetch = fetchMock

import { apiGet, apiPost, apiPostVoid, apiPut, apiPutVoid, apiDelete, apiFetch } from '../client'

const mockApiGet = vi.mocked(apiGet)
const mockApiPost = vi.mocked(apiPost)
const mockApiPostVoid = vi.mocked(apiPostVoid)
const mockApiPut = vi.mocked(apiPut)
const mockApiPutVoid = vi.mocked(apiPutVoid)
const mockApiDelete = vi.mocked(apiDelete)
const mockApiFetch = vi.mocked(apiFetch)

beforeEach(() => {
  vi.clearAllMocks()
  fetchMock.mockReset()
})

// ─── auth.ts ──────────────────────────────────────────────────────────────────

describe('auth', () => {
  it('login sends POST with credentials via raw fetch', async () => {
    const { login } = await import('../auth')
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ token: 't', user_id: 'u', role: 'admin', username: 'admin' }),
    })

    const result = await login({ username: 'admin', password: 'pass' })

    expect(fetchMock).toHaveBeenCalledWith('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username: 'admin', password: 'pass' }),
    })
    expect(result.token).toBe('t')
  })

  it('login throws on 401', async () => {
    const { login } = await import('../auth')
    fetchMock.mockResolvedValue({ ok: false, status: 401 })
    await expect(login({ username: 'x', password: 'y' })).rejects.toThrow('Invalid username or password')
  })

  it('login throws generic error on other failures', async () => {
    const { login } = await import('../auth')
    fetchMock.mockResolvedValue({ ok: false, status: 500 })
    await expect(login({ username: 'x', password: 'y' })).rejects.toThrow('Login failed')
  })

  it('getMe calls apiGet', async () => {
    const { getMe } = await import('../auth')
    await getMe()
    expect(mockApiGet).toHaveBeenCalledWith('/api/auth/me')
  })
})

// ─── analytics.ts ─────────────────────────────────────────────────────────────

describe('analytics', () => {
  it('getVMPerformance calls apiGet with vm name and range', async () => {
    const { getVMPerformance } = await import('../analytics')
    await getVMPerformance('vm1', '1h')
    expect(mockApiGet).toHaveBeenCalledWith('/api/analytics/vms/vm1?range=1h')
  })

  it('getVMPerformance defaults to 24h range', async () => {
    const { getVMPerformance } = await import('../analytics')
    await getVMPerformance('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/analytics/vms/vm1?range=24h')
  })

  it('getSystemPerformance calls apiGet', async () => {
    const { getSystemPerformance } = await import('../analytics')
    await getSystemPerformance('6h')
    expect(mockApiGet).toHaveBeenCalledWith('/api/analytics/system?range=6h')
  })

  it('getPerformanceInsights calls apiGet', async () => {
    const { getPerformanceInsights } = await import('../analytics')
    await getPerformanceInsights()
    expect(mockApiGet).toHaveBeenCalledWith('/api/analytics/insights')
  })

  it('getTopVMsByResource calls apiGet with resource and limit', async () => {
    const { getTopVMsByResource } = await import('../analytics')
    await getTopVMsByResource('cpu', 5)
    expect(mockApiGet).toHaveBeenCalledWith('/api/analytics/top?resource=cpu&limit=5')
  })

  it('exportPerformanceReport uses apiFetch and returns blob', async () => {
    const { exportPerformanceReport } = await import('../analytics')
    const blob = new Blob(['data'])
    mockApiFetch.mockResolvedValue({ ok: true, blob: () => Promise.resolve(blob) } as Response)

    const result = await exportPerformanceReport('7d', 'csv')
    expect(mockApiFetch).toHaveBeenCalledWith('/api/analytics/export?range=7d&format=csv')
    expect(result).toBe(blob)
  })

  it('exportPerformanceReport throws on failure', async () => {
    const { exportPerformanceReport } = await import('../analytics')
    mockApiFetch.mockResolvedValue({ ok: false } as Response)
    await expect(exportPerformanceReport('24h')).rejects.toThrow('Failed to export performance report')
  })

  it('getResourceUtilization calls apiGet', async () => {
    const { getResourceUtilization } = await import('../analytics')
    await getResourceUtilization()
    expect(mockApiGet).toHaveBeenCalledWith('/api/analytics/utilization')
  })
})

// ─── audit.ts ─────────────────────────────────────────────────────────────────

describe('audit', () => {
  it('listAuditLogs calls apiGet without filters', async () => {
    const { listAuditLogs } = await import('../audit')
    await listAuditLogs()
    expect(mockApiGet).toHaveBeenCalledWith('/api/audit/logs')
  })

  it('listAuditLogs calls apiGet with filters', async () => {
    const { listAuditLogs } = await import('../audit')
    await listAuditLogs({ action: 'create', user: 'admin' })
    expect(mockApiGet).toHaveBeenCalledWith(expect.stringContaining('/api/audit/logs?'))
  })

  it('getAuditLog calls apiGet', async () => {
    const { getAuditLog } = await import('../audit')
    await getAuditLog('log1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/audit/logs/log1')
  })

  it('exportAuditLogs uses apiFetch and returns blob', async () => {
    const { exportAuditLogs } = await import('../audit')
    const blob = new Blob(['csv'])
    mockApiFetch.mockResolvedValue({ ok: true, blob: () => Promise.resolve(blob) } as Response)

    const result = await exportAuditLogs(undefined, 'csv')
    expect(mockApiFetch).toHaveBeenCalledWith(expect.stringContaining('/api/audit/logs/export?'))
    expect(result).toBe(blob)
  })

  it('exportAuditLogs throws on failure', async () => {
    const { exportAuditLogs } = await import('../audit')
    mockApiFetch.mockResolvedValue({ ok: false } as Response)
    await expect(exportAuditLogs()).rejects.toThrow('Failed to export audit logs')
  })

  it('getAuditStats calls apiGet', async () => {
    const { getAuditStats } = await import('../audit')
    await getAuditStats()
    expect(mockApiGet).toHaveBeenCalledWith('/api/audit/stats')
  })
})

// ─── autoscale.ts ─────────────────────────────────────────────────────────────

describe('autoscale', () => {
  it('listPolicies calls apiGet', async () => {
    const { listPolicies } = await import('../autoscale')
    await listPolicies()
    expect(mockApiGet).toHaveBeenCalledWith('/api/autoscale')
  })

  it('createPolicy calls apiPost', async () => {
    const { createPolicy } = await import('../autoscale')
    const policy = { vm_name: 'vm1', min_cpus: 1, max_cpus: 4, min_memory_mb: 512, max_memory_mb: 4096, cooldown_secs: 300 }
    await createPolicy(policy)
    expect(mockApiPost).toHaveBeenCalledWith('/api/autoscale', policy)
  })

  it('deletePolicy calls apiDelete', async () => {
    const { deletePolicy } = await import('../autoscale')
    await deletePolicy('vm1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/autoscale/vm1')
  })

  it('listScaleEvents calls apiGet', async () => {
    const { listScaleEvents } = await import('../autoscale')
    await listScaleEvents()
    expect(mockApiGet).toHaveBeenCalledWith('/api/autoscale/events')
  })
})

// ─── backup.ts ────────────────────────────────────────────────────────────────

describe('backup', () => {
  it('listBackups calls apiGet without vmName', async () => {
    const { listBackups } = await import('../backup')
    await listBackups()
    expect(mockApiGet).toHaveBeenCalledWith('/api/backups')
  })

  it('listBackups calls apiGet with vmName', async () => {
    const { listBackups } = await import('../backup')
    await listBackups('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/backups?vm=vm1')
  })

  it('getBackup calls apiGet', async () => {
    const { getBackup } = await import('../backup')
    await getBackup('b1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/backups/b1')
  })

  it('createBackup calls apiPost', async () => {
    const { createBackup } = await import('../backup')
    const req = { vm_name: 'vm1', backup_type: 'full' as const }
    await createBackup(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/backups', req)
  })

  it('deleteBackup calls apiDelete', async () => {
    const { deleteBackup } = await import('../backup')
    await deleteBackup('b1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/backups/b1')
  })

  it('restoreBackup calls apiPost', async () => {
    const { restoreBackup } = await import('../backup')
    const opts = { backup_id: 'b1' }
    await restoreBackup(opts)
    expect(mockApiPost).toHaveBeenCalledWith('/api/backups/restore', opts)
  })

  it('getBackupJobs calls apiGet', async () => {
    const { getBackupJobs } = await import('../backup')
    await getBackupJobs()
    expect(mockApiGet).toHaveBeenCalledWith('/api/backups/jobs')
  })

  it('getBackupJob calls apiGet', async () => {
    const { getBackupJob } = await import('../backup')
    await getBackupJob('j1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/backups/jobs/j1')
  })

  it('listBackupPolicies calls apiGet', async () => {
    const { listBackupPolicies } = await import('../backup')
    await listBackupPolicies()
    expect(mockApiGet).toHaveBeenCalledWith('/api/backups/policies')
  })

  it('createBackupPolicy calls apiPost', async () => {
    const { createBackupPolicy } = await import('../backup')
    const policy = { name: 'daily', schedule_type: 'daily' as const, backup_type: 'full' as const, retention_days: 30, enabled: true }
    await createBackupPolicy(policy)
    expect(mockApiPost).toHaveBeenCalledWith('/api/backups/policies', policy)
  })

  it('deleteBackupPolicy calls apiDelete', async () => {
    const { deleteBackupPolicy } = await import('../backup')
    await deleteBackupPolicy('p1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/backups/policies/p1')
  })

  it('enableBackupPolicy calls apiPostVoid', async () => {
    const { enableBackupPolicy } = await import('../backup')
    await enableBackupPolicy('p1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/backups/policies/p1/enable')
  })

  it('disableBackupPolicy calls apiPostVoid', async () => {
    const { disableBackupPolicy } = await import('../backup')
    await disableBackupPolicy('p1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/backups/policies/p1/disable')
  })

  it('getBackupStats calls apiGet', async () => {
    const { getBackupStats } = await import('../backup')
    await getBackupStats()
    expect(mockApiGet).toHaveBeenCalledWith('/api/backups/stats')
  })
})

// ─── certificates.ts ─────────────────────────────────────────────────────────

describe('certificates', () => {
  it('listCas calls apiGet', async () => {
    const { listCas } = await import('../certificates')
    await listCas()
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates/cas')
  })

  it('createCa calls apiPost', async () => {
    const { createCa } = await import('../certificates')
    const req = { name: 'root', ca_type: 'root' as const, subject: 'CN=Root' }
    await createCa(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/cas', req)
  })

  it('listCertificates calls apiGet without caId', async () => {
    const { listCertificates } = await import('../certificates')
    await listCertificates()
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates')
  })

  it('listCertificates calls apiGet with caId (ignored)', async () => {
    const { listCertificates } = await import('../certificates')
    await listCertificates('ca1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates')
  })

  it('issueCertificate calls apiPost', async () => {
    const { issueCertificate } = await import('../certificates')
    const req = { ca_id: 'ca1', subject: 'CN=test', cert_type: 'server' as const }
    await issueCertificate(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/issue', req)
  })

  it('revokeCertificate calls apiPostVoid', async () => {
    const { revokeCertificate } = await import('../certificates')
    await revokeCertificate('cert1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/certificates/cert1/revoke')
  })

  it('renewCertificate calls apiPost', async () => {
    const { renewCertificate } = await import('../certificates')
    await renewCertificate('cert1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/cert1/renew')
  })

  it('checkExpiring calls apiGet', async () => {
    const { checkExpiring } = await import('../certificates')
    await checkExpiring(30)
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates/expiring?days=30')
  })

  it('checkExpiring without days', async () => {
    const { checkExpiring } = await import('../certificates')
    await checkExpiring()
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates/expiring')
  })

  it('listCertRequests calls apiGet', async () => {
    const { listCertRequests } = await import('../certificates')
    await listCertRequests('pending')
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates/requests?status=pending')
  })

  it('submitCertRequest calls apiPost', async () => {
    const { submitCertRequest } = await import('../certificates')
    const req = { subject: 'CN=x', ca_id: 'ca1', cert_type: 'server' as const }
    await submitCertRequest(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/requests', req)
  })

  it('approveCertRequest calls apiPost', async () => {
    const { approveCertRequest } = await import('../certificates')
    await approveCertRequest('req1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/requests/req1/approve')
  })

  it('listAttestations calls apiGet', async () => {
    const { listAttestations } = await import('../certificates')
    await listAttestations()
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates/attestations')
  })

  it('submitAttestation calls apiPost', async () => {
    const { submitAttestation } = await import('../certificates')
    const req = { host_id: 'h1', tpm_present: true }
    await submitAttestation(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/attestations', req)
  })

  it('listSecurityBaselines calls apiGet', async () => {
    const { listSecurityBaselines } = await import('../certificates')
    await listSecurityBaselines()
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates/security-baselines')
  })

  it('createSecurityBaseline calls apiPost', async () => {
    const { createSecurityBaseline } = await import('../certificates')
    const req = { name: 'base1', checks: [{ check_id: 'c1', name: 'check', category: 'cat', severity: 'high' as const }] }
    await createSecurityBaseline(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/security-baselines', req)
  })

  it('checkVmSecurityCompliance calls apiPost', async () => {
    const { checkVmSecurityCompliance } = await import('../certificates')
    await checkVmSecurityCompliance('b1', 'vm1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/certificates/security-baselines/b1/compliance', { vm_id: 'vm1' })
  })

  it('getCertHealthDashboard calls apiGet', async () => {
    const { getCertHealthDashboard } = await import('../certificates')
    await getCertHealthDashboard()
    expect(mockApiGet).toHaveBeenCalledWith('/api/certificates/health')
  })
})

// ─── contentLibrary.ts ────────────────────────────────────────────────────────

describe('contentLibrary', () => {
  it('listLibraries calls apiGet', async () => {
    const { listLibraries } = await import('../contentLibrary')
    await listLibraries()
    expect(mockApiGet).toHaveBeenCalledWith('/api/content-library/libraries')
  })

  it('createLibrary calls apiPost', async () => {
    const { createLibrary } = await import('../contentLibrary')
    const req = { name: 'lib1', library_type: 'local' as const, storage_path: '/data' }
    await createLibrary(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/content-library/libraries', req)
  })

  it('deleteLibrary calls apiDelete', async () => {
    const { deleteLibrary } = await import('../contentLibrary')
    await deleteLibrary('lib1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/content-library/libraries/lib1')
  })

  it('syncLibrary calls apiPostVoid', async () => {
    const { syncLibrary } = await import('../contentLibrary')
    await syncLibrary('lib1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/content-library/libraries/lib1/sync')
  })

  it('listLibraryItems calls apiGet', async () => {
    const { listLibraryItems } = await import('../contentLibrary')
    await listLibraryItems('lib1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/content-library/libraries/lib1/items')
  })

  it('addLibraryItem calls apiPost', async () => {
    const { addLibraryItem } = await import('../contentLibrary')
    const req = { name: 'item1', item_type: 'iso' as const }
    await addLibraryItem('lib1', req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/content-library/libraries/lib1/items', req)
  })

  it('deleteLibraryItem calls apiDelete', async () => {
    const { deleteLibraryItem } = await import('../contentLibrary')
    await deleteLibraryItem('lib1', 'item1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/content-library/items/item1')
  })

  it('searchItems calls apiGet', async () => {
    const { searchItems } = await import('../contentLibrary')
    await searchItems('ubuntu')
    expect(mockApiGet).toHaveBeenCalledWith(expect.stringContaining('/api/content-library/items/search?'))
  })

  it('listCustomizationSpecs calls apiGet', async () => {
    const { listCustomizationSpecs } = await import('../contentLibrary')
    await listCustomizationSpecs()
    expect(mockApiGet).toHaveBeenCalledWith('/api/content-library/customization-specs')
  })

  it('createCustomizationSpec calls apiPost', async () => {
    const { createCustomizationSpec } = await import('../contentLibrary')
    const req = { name: 'spec1', os_type: 'linux' as const }
    await createCustomizationSpec(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/content-library/customization-specs', req)
  })

  it('deleteCustomizationSpec calls apiDelete', async () => {
    const { deleteCustomizationSpec } = await import('../contentLibrary')
    await deleteCustomizationSpec('spec1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/content-library/customization-specs/spec1')
  })

  it('listHostProfiles calls apiGet', async () => {
    const { listHostProfiles } = await import('../contentLibrary')
    await listHostProfiles()
    expect(mockApiGet).toHaveBeenCalledWith('/api/content-library/host-profiles')
  })

  it('createHostProfile calls apiPost', async () => {
    const { createHostProfile } = await import('../contentLibrary')
    const req = { name: 'prof1', settings: {} }
    await createHostProfile(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/content-library/host-profiles', req)
  })

  it('deleteHostProfile calls apiDelete', async () => {
    const { deleteHostProfile } = await import('../contentLibrary')
    await deleteHostProfile('prof1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/content-library/host-profiles/prof1')
  })

  it('checkHostCompliance calls apiPost', async () => {
    const { checkHostCompliance } = await import('../contentLibrary')
    const currentConfig = { bridge: 'br0' }
    await checkHostCompliance('prof1', 'host1', currentConfig)
    expect(mockApiPost).toHaveBeenCalledWith('/api/content-library/host-profiles/prof1/compliance', { host_id: 'host1', current_config: currentConfig })
  })
})

// ─── containerGroups.ts ───────────────────────────────────────────────────────

describe('containerGroups', () => {
  it('listContainerGroups calls apiGet', async () => {
    const { listContainerGroups } = await import('../containerGroups')
    await listContainerGroups()
    expect(mockApiGet).toHaveBeenCalledWith('/api/container-groups')
  })

  it('getContainerGroup calls apiGet with the encoded name', async () => {
    const { getContainerGroup } = await import('../containerGroups')
    await getContainerGroup('web app')
    expect(mockApiGet).toHaveBeenCalledWith('/api/container-groups/web%20app/spec')
  })

  it('applyContainerGroup calls apiPost with the spec', async () => {
    const { applyContainerGroup } = await import('../containerGroups')
    const spec = { name: 'web', containers: [{ name: 'app', image: 'nginx', resources: { cpus: 1, memory: '512M' } }] }
    await applyContainerGroup(spec)
    expect(mockApiPost).toHaveBeenCalledWith('/api/container-groups/apply', spec)
  })

  it('deleteContainerGroup calls apiDelete with the encoded name', async () => {
    const { deleteContainerGroup } = await import('../containerGroups')
    await deleteContainerGroup('web')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/container-groups/web')
  })

  it('listContainerGroupEvents calls apiGet', async () => {
    const { listContainerGroupEvents } = await import('../containerGroups')
    await listContainerGroupEvents()
    expect(mockApiGet).toHaveBeenCalledWith('/api/container-group-events')
  })

  it('listContainerGroupBackups calls apiGet', async () => {
    const { listContainerGroupBackups } = await import('../containerGroups')
    await listContainerGroupBackups()
    expect(mockApiGet).toHaveBeenCalledWith('/api/container-group-backups')
  })

  it('createContainerGroupBackup calls apiPost with a default retention of 30 days', async () => {
    const { createContainerGroupBackup } = await import('../containerGroups')
    await createContainerGroupBackup('web')
    expect(mockApiPost).toHaveBeenCalledWith('/api/container-group-backups', {
      container_group_name: 'web',
      retention_days: 30,
    })
  })

  it('createContainerGroupBackup respects an explicit retention', async () => {
    const { createContainerGroupBackup } = await import('../containerGroups')
    await createContainerGroupBackup('web', 7)
    expect(mockApiPost).toHaveBeenCalledWith('/api/container-group-backups', {
      container_group_name: 'web',
      retention_days: 7,
    })
  })

  it('getContainerGroupStatus calls apiGet with encoded name', async () => {
    const { getContainerGroupStatus } = await import('../containerGroups')
    await getContainerGroupStatus('web app')
    expect(mockApiGet).toHaveBeenCalledWith('/api/container-groups/web%20app/status')
  })

  it('deleteContainerGroupBackup calls apiDelete with the encoded id', async () => {
    const { deleteContainerGroupBackup } = await import('../containerGroups')
    await deleteContainerGroupBackup('backup-1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/container-group-backups/backup-1')
  })

  it('restoreContainerGroupBackup calls apiPost with no body', async () => {
    const { restoreContainerGroupBackup } = await import('../containerGroups')
    await restoreContainerGroupBackup('backup-1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/container-group-backups/backup-1/restore')
  })
})

// ─── datacenter.ts ────────────────────────────────────────────────────────────

describe('datacenter', () => {
  it('listDatacenters calls apiGet', async () => {
    const { listDatacenters } = await import('../datacenter')
    await listDatacenters()
    expect(mockApiGet).toHaveBeenCalledWith('/api/datacenters')
  })

  it('createDatacenter calls apiPost', async () => {
    const { createDatacenter } = await import('../datacenter')
    await createDatacenter({ name: 'dc1' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/datacenters', { name: 'dc1' })
  })

  it('getDatacenter calls apiGet', async () => {
    const { getDatacenter } = await import('../datacenter')
    await getDatacenter('dc1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/datacenters/dc1')
  })

  it('updateDatacenter calls apiPut', async () => {
    const { updateDatacenter } = await import('../datacenter')
    await updateDatacenter('dc1', { name: 'dc1-updated' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/datacenters/dc1', { name: 'dc1-updated' })
  })

  it('deleteDatacenter calls apiDelete', async () => {
    const { deleteDatacenter } = await import('../datacenter')
    await deleteDatacenter('dc1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/datacenters/dc1')
  })

  it('getDatacenterSummary calls apiGet', async () => {
    const { getDatacenterSummary } = await import('../datacenter')
    await getDatacenterSummary('dc1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/datacenters/dc1/summary')
  })

  it('listClusters calls apiGet without datacenterId', async () => {
    const { listClusters } = await import('../datacenter')
    await listClusters()
    expect(mockApiGet).toHaveBeenCalledWith('/api/clusters')
  })

  it('listClusters calls apiGet with datacenterId', async () => {
    const { listClusters } = await import('../datacenter')
    await listClusters('dc1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/clusters?datacenter_id=dc1')
  })

  it('createCluster calls apiPost', async () => {
    const { createCluster } = await import('../datacenter')
    await createCluster({ name: 'c1', datacenter_id: 'dc1' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/clusters', { name: 'c1', datacenter_id: 'dc1' })
  })

  it('getCluster calls apiGet', async () => {
    const { getCluster } = await import('../datacenter')
    await getCluster('c1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/clusters/c1')
  })

  it('updateCluster calls apiPut', async () => {
    const { updateCluster } = await import('../datacenter')
    await updateCluster('c1', { name: 'c1-new' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/clusters/c1', { name: 'c1-new' })
  })

  it('deleteCluster calls apiDelete', async () => {
    const { deleteCluster } = await import('../datacenter')
    await deleteCluster('c1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/clusters/c1')
  })

  it('listHosts calls apiGet', async () => {
    const { listHosts } = await import('../datacenter')
    await listHosts('c1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/hosts?cluster_id=c1')
  })

  it('registerHost calls apiPost', async () => {
    const { registerHost } = await import('../datacenter')
    const req = { hostname: 'h1', address: '10.0.0.1', cluster_id: 'c1', cpus: 8, memory_mb: 16384 }
    await registerHost(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/hosts', req)
  })

  it('getHost calls apiGet', async () => {
    const { getHost } = await import('../datacenter')
    await getHost('h1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/hosts/h1')
  })

  it('removeHost calls apiDelete', async () => {
    const { removeHost } = await import('../datacenter')
    await removeHost('h1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/hosts/h1')
  })

  it('hostEnterMaintenance calls apiPostVoid', async () => {
    const { hostEnterMaintenance } = await import('../datacenter')
    await hostEnterMaintenance('h1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/hosts/h1/maintenance/enter')
  })

  it('hostExitMaintenance calls apiPostVoid', async () => {
    const { hostExitMaintenance } = await import('../datacenter')
    await hostExitMaintenance('h1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/hosts/h1/maintenance/exit')
  })
})

// ─── distributedStorage.ts ────────────────────────────────────────────────────

describe('distributedStorage', () => {
  it('listDistributedPools calls apiGet', async () => {
    const { listDistributedPools } = await import('../distributedStorage')
    await listDistributedPools()
    expect(mockApiGet).toHaveBeenCalledWith('/api/distributed-storage/pools')
  })

  it('createDistributedPool calls apiPost', async () => {
    const { createDistributedPool } = await import('../distributedStorage')
    const req = {
      name: 'pool1',
      cluster_id: 'cl1',
      hosts: [{ host_id: 'h1', hostname: 'host1', disks: [] }],
      replication_factor: 3,
      erasure_coding: false,
      fault_domains: [],
    }
    await createDistributedPool(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/distributed-storage/pools', req)
  })

  it('deleteDistributedPool calls apiDelete', async () => {
    const { deleteDistributedPool } = await import('../distributedStorage')
    await deleteDistributedPool('pool1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/distributed-storage/pools/pool1')
  })

  it('startStorageMigration calls apiPost', async () => {
    const { startStorageMigration } = await import('../distributedStorage')
    const req = { vm_id: 'vm1', source_pool_id: 'p1', target_pool_id: 'p2' }
    await startStorageMigration(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/distributed-storage/migrations', req)
  })

  it('listStorageMigrations calls apiGet', async () => {
    const { listStorageMigrations } = await import('../distributedStorage')
    await listStorageMigrations('in_progress')
    expect(mockApiGet).toHaveBeenCalledWith('/api/distributed-storage/migrations?status=in_progress')
  })

  it('listStoragePolicies calls apiGet', async () => {
    const { listStoragePolicies } = await import('../distributedStorage')
    await listStoragePolicies()
    expect(mockApiGet).toHaveBeenCalledWith('/api/distributed-storage/policies')
  })

  it('createStoragePolicy calls apiPost', async () => {
    const { createStoragePolicy } = await import('../distributedStorage')
    const req = { name: 'pol1', description: 'test policy', replication_factor: 3, encryption_required: false, tier: 'gold' as const }
    await createStoragePolicy(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/distributed-storage/policies', expect.objectContaining(req))
  })

  it('deleteStoragePolicy calls apiDelete', async () => {
    const { deleteStoragePolicy } = await import('../distributedStorage')
    await deleteStoragePolicy('pol1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/distributed-storage/policies/pol1')
  })

  it('checkCompliance calls apiPost', async () => {
    const { checkCompliance } = await import('../distributedStorage')
    await checkCompliance('pol1', 'vm1', 'pool1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/distributed-storage/policies/pol1/compliance', { vm_name: 'vm1', pool_id: 'pool1' })
  })

  it('listDatastoreClusters calls apiGet', async () => {
    const { listDatastoreClusters } = await import('../distributedStorage')
    await listDatastoreClusters()
    expect(mockApiGet).toHaveBeenCalledWith('/api/distributed-storage/datastore-clusters')
  })

  it('createDatastoreCluster calls apiPost', async () => {
    const { createDatastoreCluster } = await import('../distributedStorage')
    const req = {
      name: 'dsc1',
      cluster_id: 'cl1',
      datastore_ids: ['p1'],
      storage_drs_enabled: true,
      space_threshold_pct: 80,
      automation_level: 'manual' as const,
    }
    await createDatastoreCluster(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/distributed-storage/datastore-clusters', req)
  })
})

// ─── drs.ts ───────────────────────────────────────────────────────────────────

describe('drs', () => {
  it('configureDrs calls apiPost', async () => {
    const { configureDrs } = await import('../drs')
    const req = { cluster_id: 'c1', enabled: true }
    await configureDrs(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/drs/config', req)
  })

  it('getDrsConfig calls apiGet', async () => {
    const { getDrsConfig } = await import('../drs')
    await getDrsConfig('c1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/drs/config/c1')
  })

  it('computePlacement calls apiPost', async () => {
    const { computePlacement } = await import('../drs')
    const req = { cluster_id: 'c1', vm_cpus: 4, vm_memory_mb: 8192 }
    await computePlacement(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/drs/placement', req)
  })

  it('analyzeBalance calls apiGet', async () => {
    const { analyzeBalance } = await import('../drs')
    await analyzeBalance('c1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/drs/balance/c1')
  })

  it('generateRecommendations calls apiPost', async () => {
    const { generateRecommendations } = await import('../drs')
    await generateRecommendations('c1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/drs/recommendations', { cluster_id: 'c1' })
  })

  it('listRecommendations calls apiGet', async () => {
    const { listRecommendations } = await import('../drs')
    await listRecommendations('c1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/drs/recommendations/c1')
  })

  it('approveRecommendation calls apiPostVoid', async () => {
    const { approveRecommendation } = await import('../drs')
    await approveRecommendation('r1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/drs/recommendations/r1/approve')
  })

  it('rejectRecommendation calls apiPostVoid', async () => {
    const { rejectRecommendation } = await import('../drs')
    await rejectRecommendation('r1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/drs/recommendations/r1/reject')
  })

  it('listAffinityRules calls apiGet', async () => {
    const { listAffinityRules } = await import('../drs')
    await listAffinityRules('c1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/drs/affinity-rules?cluster_id=c1')
  })

  it('createAffinityRule calls apiPost', async () => {
    const { createAffinityRule } = await import('../drs')
    const req = { cluster_id: 'c1', name: 'rule1', rule_type: 'affinity' as const, mandatory: true, vm_ids: ['vm1'] }
    await createAffinityRule(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/drs/affinity-rules', req)
  })

  it('updateAffinityRule calls apiPut', async () => {
    const { updateAffinityRule } = await import('../drs')
    await updateAffinityRule('r1', { name: 'updated' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/drs/affinity-rules/r1', { name: 'updated' })
  })

  it('deleteAffinityRule calls apiDelete', async () => {
    const { deleteAffinityRule } = await import('../drs')
    await deleteAffinityRule('r1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/drs/affinity-rules/r1')
  })
})

// ─── encryption.ts ────────────────────────────────────────────────────────────

describe('encryption', () => {
  it('listProviders calls apiGet', async () => {
    const { listProviders } = await import('../encryption')
    await listProviders()
    expect(mockApiGet).toHaveBeenCalledWith('/api/encryption/providers')
  })

  it('registerProvider calls apiPost', async () => {
    const { registerProvider } = await import('../encryption')
    const req = { name: 'kms', provider_type: 'vault', endpoint: 'https://vault:8200' }
    await registerProvider(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/encryption/providers', { ...req, status: 'connected' })
  })

  it('removeProvider calls apiDelete', async () => {
    const { removeProvider } = await import('../encryption')
    await removeProvider('kms1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/encryption/providers/kms1')
  })

  it('listEncryptionPolicies calls apiGet', async () => {
    const { listEncryptionPolicies } = await import('../encryption')
    await listEncryptionPolicies()
    expect(mockApiGet).toHaveBeenCalledWith('/api/encryption/policies')
  })

  it('createEncryptionPolicy calls apiPost', async () => {
    const { createEncryptionPolicy } = await import('../encryption')
    const req = { name: 'aes256', key_provider_id: 'kms1', algorithm: 'aes256_xts' }
    await createEncryptionPolicy(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/encryption/policies', { ...req, encrypt_vmotion: false })
  })

  it('encryptVm calls apiPostVoid', async () => {
    const { encryptVm } = await import('../encryption')
    await encryptVm('vm1', 'pol1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/encryption/vms/vm1/encrypt', { vm_name: 'vm1', policy_id: 'pol1' })
  })

  it('decryptVm calls apiPostVoid', async () => {
    const { decryptVm } = await import('../encryption')
    await decryptVm('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/encryption/vms/vm1/decrypt', { vm_name: 'vm1' })
  })

  it('getVmEncryptionStatus calls apiGet', async () => {
    const { getVmEncryptionStatus } = await import('../encryption')
    await getVmEncryptionStatus('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/encryption/vms/vm1/status')
  })

  it('listEncryptedVms calls apiGet', async () => {
    const { listEncryptedVms } = await import('../encryption')
    await listEncryptedVms()
    expect(mockApiGet).toHaveBeenCalledWith('/api/encryption/vms')
  })

  it('rotateVmKey calls apiPostVoid', async () => {
    const { rotateVmKey } = await import('../encryption')
    await rotateVmKey('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/encryption/vms/vm1/rotate-key')
  })
})

// ─── faultTolerance.ts ────────────────────────────────────────────────────────

describe('faultTolerance', () => {
  it('enableFt calls apiPost', async () => {
    const { enableFt } = await import('../faultTolerance')
    const req = { vm_name: 'vm1', primary_host_id: 'h1', secondary_host_id: 'h2' }
    await enableFt(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/ft/enable', req)
  })

  it('disableFt calls apiDelete', async () => {
    const { disableFt } = await import('../faultTolerance')
    await disableFt('vm1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/ft/vms/vm1')
  })

  it('getFtConfig calls apiGet', async () => {
    const { getFtConfig } = await import('../faultTolerance')
    await getFtConfig('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/ft/vms/vm1')
  })

  it('listFtVms calls apiGet', async () => {
    const { listFtVms } = await import('../faultTolerance')
    await listFtVms()
    expect(mockApiGet).toHaveBeenCalledWith('/api/ft/vms')
  })

  it('checkFtCompatibility calls apiGet', async () => {
    const { checkFtCompatibility } = await import('../faultTolerance')
    await checkFtCompatibility('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/ft/vms/vm1/compatibility')
  })

  it('triggerFailover calls apiPost', async () => {
    const { triggerFailover } = await import('../faultTolerance')
    await triggerFailover('vm1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/ft/vms/vm1/failover', {})
  })

  it('testFailover calls apiPost', async () => {
    const { testFailover } = await import('../faultTolerance')
    await testFailover('vm1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/ft/vms/vm1/test-failover', {})
  })

  it('getFtMetrics calls apiGet', async () => {
    const { getFtMetrics } = await import('../faultTolerance')
    await getFtMetrics('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/ft/vms/vm1/metrics')
  })

  it('getFtEvents calls apiGet with vmId', async () => {
    const { getFtEvents } = await import('../faultTolerance')
    await getFtEvents('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/ft/events?vm_name=vm1')
  })

  it('getFtEvents calls apiGet without vmId', async () => {
    const { getFtEvents } = await import('../faultTolerance')
    await getFtEvents()
    expect(mockApiGet).toHaveBeenCalledWith('/api/ft/events')
  })
})

// ─── firmware.ts ──────────────────────────────────────────────────────────────

describe('firmware', () => {
  it('getFirmwareStatus calls apiGet', async () => {
    const { getFirmwareStatus } = await import('../firmware')
    await getFirmwareStatus('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/firmware/status')
  })

  it('enableUefi calls apiPostVoid', async () => {
    const { enableUefi } = await import('../firmware')
    await enableUefi('vm1', { secure_boot: true })
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/firmware/uefi', { secure_boot: true })
  })

  it('enableSecureBoot calls apiPostVoid', async () => {
    const { enableSecureBoot } = await import('../firmware')
    await enableSecureBoot('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/firmware/secureboot')
  })

  it('disableSecureBoot calls apiDelete', async () => {
    const { disableSecureBoot } = await import('../firmware')
    await disableSecureBoot('vm1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/vms/vm1/firmware/secureboot')
  })

  it('resetNvram calls apiPostVoid', async () => {
    const { resetNvram } = await import('../firmware')
    await resetNvram('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/firmware/reset')
  })

  it('getFirmwareCapabilities calls apiGet', async () => {
    const { getFirmwareCapabilities } = await import('../firmware')
    await getFirmwareCapabilities()
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/firmware/capabilities')
  })
})

// ─── hotplug.ts ───────────────────────────────────────────────────────────────

describe('hotplug', () => {
  it('hotplugCpu calls apiPost', async () => {
    const { hotplugCpu } = await import('../hotplug')
    await hotplugCpu('vm1', { count: 4 })
    expect(mockApiPost).toHaveBeenCalledWith('/api/vms/vm1/hotplug/cpu', { count: 4 })
  })

  it('hotplugMemory calls apiPost', async () => {
    const { hotplugMemory } = await import('../hotplug')
    await hotplugMemory('vm1', { size_mb: 2048 })
    expect(mockApiPost).toHaveBeenCalledWith('/api/vms/vm1/hotplug/memory', { size_mb: 2048 })
  })

  it('hotplugDisk calls apiPost', async () => {
    const { hotplugDisk } = await import('../hotplug')
    await hotplugDisk('vm1', { path: '/dev/sdb' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/vms/vm1/hotplug/disk', { path: '/dev/sdb' })
  })

  it('hotremoveDisk uses apiFetch with DELETE and returns JSON', async () => {
    const { hotremoveDisk } = await import('../hotplug')
    const data = { status: 'removed' }
    mockApiFetch.mockResolvedValue({ ok: true, json: () => Promise.resolve(data) } as Response)

    const result = await hotremoveDisk('vm1', 'disk1')
    expect(mockApiFetch).toHaveBeenCalledWith('/api/vms/vm1/hotplug/disk/disk1', { method: 'DELETE' })
    expect(result).toEqual(data)
  })

  it('hotremoveDisk throws on failure', async () => {
    const { hotremoveDisk } = await import('../hotplug')
    mockApiFetch.mockResolvedValue({ ok: false } as Response)
    await expect(hotremoveDisk('vm1', 'disk1')).rejects.toThrow('Failed to hot-remove disk')
  })

  it('hotplugNic calls apiPost', async () => {
    const { hotplugNic } = await import('../hotplug')
    await hotplugNic('vm1', { bridge: 'br0' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/vms/vm1/hotplug/nic', { bridge: 'br0' })
  })

  it('hotremoveNic uses apiFetch with DELETE and returns JSON', async () => {
    const { hotremoveNic } = await import('../hotplug')
    const data = { status: 'removed' }
    mockApiFetch.mockResolvedValue({ ok: true, json: () => Promise.resolve(data) } as Response)

    const result = await hotremoveNic('vm1', 'nic1')
    expect(mockApiFetch).toHaveBeenCalledWith('/api/vms/vm1/hotplug/nic/nic1', { method: 'DELETE' })
    expect(result).toEqual(data)
  })

  it('hotremoveNic throws on failure', async () => {
    const { hotremoveNic } = await import('../hotplug')
    mockApiFetch.mockResolvedValue({ ok: false } as Response)
    await expect(hotremoveNic('vm1', 'nic1')).rejects.toThrow('Failed to hot-remove NIC')
  })
})

// ─── images.ts ────────────────────────────────────────────────────────────────

describe('images', () => {
  it('listImages calls apiGet', async () => {
    const { listImages } = await import('../images')
    await listImages()
    expect(mockApiGet).toHaveBeenCalledWith('/api/images')
  })
})

// ─── lifecycle.ts ─────────────────────────────────────────────────────────────

describe('lifecycle', () => {
  it('listBaselines calls apiGet', async () => {
    const { listBaselines } = await import('../lifecycle')
    await listBaselines()
    expect(mockApiGet).toHaveBeenCalledWith('/api/lifecycle/baselines')
  })

  it('createBaseline calls apiPost', async () => {
    const { createBaseline } = await import('../lifecycle')
    const req = { name: 'b1', baseline_type: 'patch' as const, severity: 'critical' as const }
    await createBaseline(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/lifecycle/baselines', req)
  })

  it('deleteBaseline calls apiDelete', async () => {
    const { deleteBaseline } = await import('../lifecycle')
    await deleteBaseline('b1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/lifecycle/baselines/b1')
  })

  it('scanHostCompliance calls apiPostVoid', async () => {
    const { scanHostCompliance } = await import('../lifecycle')
    await scanHostCompliance('b1', ['h1', 'h2'])
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/lifecycle/compliance/scan', { baseline_id: 'b1', host_ids: ['h1', 'h2'] })
  })

  it('getComplianceStatus calls apiGet', async () => {
    const { getComplianceStatus } = await import('../lifecycle')
    await getComplianceStatus('b1', 'h1')
    expect(mockApiGet).toHaveBeenCalledWith(expect.stringContaining('/api/lifecycle/compliance'))
  })

  it('listRemediations calls apiGet', async () => {
    const { listRemediations } = await import('../lifecycle')
    await listRemediations()
    expect(mockApiGet).toHaveBeenCalledWith('/api/lifecycle/remediations')
  })

  it('createRemediation calls apiPost', async () => {
    const { createRemediation } = await import('../lifecycle')
    await createRemediation({ host_id: 'h1', baseline_id: 'b1' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/lifecycle/remediations', { host_id: 'h1', baseline_id: 'b1' })
  })

  it('listRollingUpdates calls apiGet', async () => {
    const { listRollingUpdates } = await import('../lifecycle')
    await listRollingUpdates()
    expect(mockApiGet).toHaveBeenCalledWith('/api/lifecycle/rolling-updates')
  })

  it('createRollingUpdate calls apiPost', async () => {
    const { createRollingUpdate } = await import('../lifecycle')
    const req = { name: 'ru1', baseline_id: 'b1', host_ids: ['h1'] }
    await createRollingUpdate(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/lifecycle/rolling-updates', req)
  })

  it('startRollingUpdate calls apiPostVoid', async () => {
    const { startRollingUpdate } = await import('../lifecycle')
    await startRollingUpdate('ru1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/lifecycle/rolling-updates/ru1/start')
  })

  it('pauseRollingUpdate calls apiPostVoid', async () => {
    const { pauseRollingUpdate } = await import('../lifecycle')
    await pauseRollingUpdate('ru1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/lifecycle/rolling-updates/ru1/pause')
  })

  it('advanceRollingUpdate calls apiPostVoid', async () => {
    const { advanceRollingUpdate } = await import('../lifecycle')
    await advanceRollingUpdate('ru1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/lifecycle/rolling-updates/ru1/advance')
  })
})

// ─── migrations.ts ────────────────────────────────────────────────────────────

describe('migrations', () => {
  it('startMigration calls apiPost', async () => {
    const { startMigration } = await import('../migrations')
    const req = { vm_name: 'vm1', target_host: 'h2', migration_type: 'live' as const }
    await startMigration(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/migrations', req)
  })

  it('listMigrations calls apiGet', async () => {
    const { listMigrations } = await import('../migrations')
    await listMigrations()
    expect(mockApiGet).toHaveBeenCalledWith('/api/migrations')
  })

  it('getMigration calls apiGet', async () => {
    const { getMigration } = await import('../migrations')
    await getMigration('m1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/migrations/m1')
  })

  it('cancelMigration calls apiPost', async () => {
    const { cancelMigration } = await import('../migrations')
    await cancelMigration('m1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/migrations/m1/cancel')
  })
})

// ─── networkd.ts ──────────────────────────────────────────────────────────────

describe('networkd', () => {
  // Bridges
  it('listBridges calls apiGet', async () => {
    const { listBridges } = await import('../networkd')
    await listBridges()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/bridges')
  })

  it('createBridge calls apiPost', async () => {
    const { createBridge } = await import('../networkd')
    await createBridge({ name: 'br0' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/bridges', { name: 'br0' })
  })

  it('getBridge calls apiGet', async () => {
    const { getBridge } = await import('../networkd')
    await getBridge('br0')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/bridges/br0')
  })

  it('updateBridge calls apiPut', async () => {
    const { updateBridge } = await import('../networkd')
    await updateBridge('br0', { name: 'br0', stp: true })
    expect(mockApiPut).toHaveBeenCalledWith('/api/networkd/bridges/br0', { name: 'br0', stp: true })
  })

  it('deleteBridge calls apiDelete', async () => {
    const { deleteBridge } = await import('../networkd')
    await deleteBridge('br0')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/bridges/br0')
  })

  // VLANs
  it('listVlans calls apiGet', async () => {
    const { listVlans } = await import('../networkd')
    await listVlans()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/vlans')
  })

  it('createVlan calls apiPost', async () => {
    const { createVlan } = await import('../networkd')
    await createVlan({ name: 'vlan10', vlan_id: 10, parent_interface: 'eth0' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/vlans', { name: 'vlan10', vlan_id: 10, parent_interface: 'eth0' })
  })

  it('getVlan calls apiGet', async () => {
    const { getVlan } = await import('../networkd')
    await getVlan('v1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/vlans/v1')
  })

  it('updateVlan calls apiPut', async () => {
    const { updateVlan } = await import('../networkd')
    await updateVlan('v1', { name: 'vlan10', vlan_id: 10, parent_interface: 'eth0' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/networkd/vlans/v1', { name: 'vlan10', vlan_id: 10, parent_interface: 'eth0' })
  })

  it('deleteVlan calls apiDelete', async () => {
    const { deleteVlan } = await import('../networkd')
    await deleteVlan('v1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/vlans/v1')
  })

  // Macvtap
  it('listMacvtaps calls apiGet', async () => {
    const { listMacvtaps } = await import('../networkd')
    await listMacvtaps()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/macvtaps')
  })

  it('createMacvtap calls apiPost', async () => {
    const { createMacvtap } = await import('../networkd')
    await createMacvtap({ name: 'macvtap0', parent_interface: 'eth0' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/macvtaps', { name: 'macvtap0', parent_interface: 'eth0' })
  })

  it('getMacvtap calls apiGet', async () => {
    const { getMacvtap } = await import('../networkd')
    await getMacvtap('m1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/macvtaps/m1')
  })

  it('deleteMacvtap calls apiDelete', async () => {
    const { deleteMacvtap } = await import('../networkd')
    await deleteMacvtap('m1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/macvtaps/m1')
  })

  // Taps
  it('listTaps calls apiGet', async () => {
    const { listTaps } = await import('../networkd')
    await listTaps()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/taps')
  })

  it('createTap calls apiPost', async () => {
    const { createTap } = await import('../networkd')
    await createTap({ name: 'tap0' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/taps', { name: 'tap0' })
  })

  it('getTap calls apiGet', async () => {
    const { getTap } = await import('../networkd')
    await getTap('t1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/taps/t1')
  })

  it('deleteTap calls apiDelete', async () => {
    const { deleteTap } = await import('../networkd')
    await deleteTap('t1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/taps/t1')
  })

  // Bonds
  it('listBonds calls apiGet', async () => {
    const { listBonds } = await import('../networkd')
    await listBonds()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/bonds')
  })

  it('createBond calls apiPost', async () => {
    const { createBond } = await import('../networkd')
    await createBond({ name: 'bond0' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/bonds', { name: 'bond0' })
  })

  it('getBond calls apiGet', async () => {
    const { getBond } = await import('../networkd')
    await getBond('b1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/bonds/b1')
  })

  it('updateBond calls apiPut', async () => {
    const { updateBond } = await import('../networkd')
    await updateBond('b1', { name: 'bond0' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/networkd/bonds/b1', { name: 'bond0' })
  })

  it('deleteBond calls apiDelete', async () => {
    const { deleteBond } = await import('../networkd')
    await deleteBond('b1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/bonds/b1')
  })

  // Network files
  it('listNetworkFiles calls apiGet', async () => {
    const { listNetworkFiles } = await import('../networkd')
    await listNetworkFiles()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/network-files')
  })

  it('createNetworkFile calls apiPost', async () => {
    const { createNetworkFile } = await import('../networkd')
    await createNetworkFile({ match_name: 'eth0' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/network-files', { match_name: 'eth0' })
  })

  it('getNetworkFile calls apiGet', async () => {
    const { getNetworkFile } = await import('../networkd')
    await getNetworkFile('nf1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/network-files/nf1')
  })

  it('deleteNetworkFile calls apiDelete', async () => {
    const { deleteNetworkFile } = await import('../networkd')
    await deleteNetworkFile('nf1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/network-files/nf1')
  })

  // Link files
  it('listLinkFiles calls apiGet', async () => {
    const { listLinkFiles } = await import('../networkd')
    await listLinkFiles()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/link-files')
  })

  it('createLinkFile calls apiPost', async () => {
    const { createLinkFile } = await import('../networkd')
    await createLinkFile({ match_mac: '00:11:22:33:44:55' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/link-files', { match_mac: '00:11:22:33:44:55' })
  })

  it('deleteLinkFile calls apiDelete', async () => {
    const { deleteLinkFile } = await import('../networkd')
    await deleteLinkFile('lf1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/link-files/lf1')
  })

  // Port forwards
  it('listPortForwards calls apiGet', async () => {
    const { listPortForwards } = await import('../networkd')
    await listPortForwards()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/port-forwards')
  })

  it('createPortForward calls apiPost', async () => {
    const { createPortForward } = await import('../networkd')
    const req = { name: 'pf1', host_port: 8080, guest_ip: '10.0.0.1', guest_port: 80 }
    await createPortForward(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/port-forwards', req)
  })

  it('getPortForward calls apiGet', async () => {
    const { getPortForward } = await import('../networkd')
    await getPortForward('pf1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/port-forwards/pf1')
  })

  it('deletePortForward calls apiDelete', async () => {
    const { deletePortForward } = await import('../networkd')
    await deletePortForward('pf1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/networkd/port-forwards/pf1')
  })

  it('syncPortForwards calls apiPost', async () => {
    const { syncPortForwards } = await import('../networkd')
    await syncPortForwards()
    expect(mockApiPost).toHaveBeenCalledWith('/api/networkd/port-forwards/sync')
  })

  // Scan & status
  it('scanConfigs calls apiGet', async () => {
    const { scanConfigs } = await import('../networkd')
    await scanConfigs()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/scan')
  })

  it('listLinks calls apiGet', async () => {
    const { listLinks } = await import('../networkd')
    await listLinks()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/links')
  })

  it('getDeviceStatus calls apiGet', async () => {
    const { getDeviceStatus } = await import('../networkd')
    await getDeviceStatus('eth0')
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/links/eth0/status')
  })

  it('reloadNetworkd calls apiPostVoid', async () => {
    const { reloadNetworkd } = await import('../networkd')
    await reloadNetworkd()
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/networkd/reload')
  })

  it('listManagedFiles calls apiGet', async () => {
    const { listManagedFiles } = await import('../networkd')
    await listManagedFiles()
    expect(mockApiGet).toHaveBeenCalledWith('/api/networkd/files')
  })
})

// ─── notifications.ts ─────────────────────────────────────────────────────────

describe('notifications', () => {
  it('listChannels calls apiGet', async () => {
    const { listChannels } = await import('../notifications')
    await listChannels()
    expect(mockApiGet).toHaveBeenCalledWith('/api/notifications/channels')
  })

  it('createChannel calls apiPost', async () => {
    const { createChannel } = await import('../notifications')
    const req = { name: 'slack', type: 'slack' as const, config: { webhook: 'https://...' } }
    await createChannel(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/notifications/channels', req)
  })

  it('updateChannel calls apiPut', async () => {
    const { updateChannel } = await import('../notifications')
    await updateChannel('ch1', { name: 'updated' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/notifications/channels/ch1', { name: 'updated' })
  })

  it('deleteChannel calls apiDelete', async () => {
    const { deleteChannel } = await import('../notifications')
    await deleteChannel('ch1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/notifications/channels/ch1')
  })

  it('testChannel calls apiPostVoid', async () => {
    const { testChannel } = await import('../notifications')
    await testChannel('ch1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/notifications/channels/ch1/test')
  })

  it('listRules calls apiGet', async () => {
    const { listRules } = await import('../notifications')
    await listRules()
    expect(mockApiGet).toHaveBeenCalledWith('/api/notifications/rules')
  })

  it('createRule calls apiPost', async () => {
    const { createRule } = await import('../notifications')
    const req = { name: 'rule1', event_types: ['vm.created'], severity_levels: ['info' as const], channels: ['ch1'] }
    await createRule(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/notifications/rules', req)
  })

  it('updateRule calls apiPut', async () => {
    const { updateRule } = await import('../notifications')
    await updateRule('r1', { name: 'updated' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/notifications/rules/r1', { name: 'updated' })
  })

  it('deleteRule calls apiDelete', async () => {
    const { deleteRule } = await import('../notifications')
    await deleteRule('r1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/notifications/rules/r1')
  })

  it('enableRule calls apiPostVoid', async () => {
    const { enableRule } = await import('../notifications')
    await enableRule('r1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/notifications/rules/r1/enable')
  })

  it('disableRule calls apiPostVoid', async () => {
    const { disableRule } = await import('../notifications')
    await disableRule('r1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/notifications/rules/r1/disable')
  })

  it('getHistory calls apiGet', async () => {
    const { getHistory } = await import('../notifications')
    await getHistory(100)
    expect(mockApiGet).toHaveBeenCalledWith('/api/notifications/history?limit=100')
  })
})

// ─── plugins.ts ───────────────────────────────────────────────────────────────

describe('plugins', () => {
  it('listPlugins calls apiGet', async () => {
    const { listPlugins } = await import('../plugins')
    await listPlugins()
    expect(mockApiGet).toHaveBeenCalledWith('/api/plugins')
  })
})

// ─── profiles.ts ──────────────────────────────────────────────────────────────

describe('profiles', () => {
  it('listProfiles calls apiGet', async () => {
    const { listProfiles } = await import('../profiles')
    await listProfiles()
    expect(mockApiGet).toHaveBeenCalledWith('/api/profiles')
  })

  it('createProfile calls apiPost', async () => {
    const { createProfile } = await import('../profiles')
    const req = { name: 'small', description: 'Small VM', cpus: 1, memory: 512, disk: 10, category: 'general' as const }
    await createProfile(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/profiles', req)
  })

  it('deleteProfile calls apiDelete', async () => {
    const { deleteProfile } = await import('../profiles')
    await deleteProfile('small')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/profiles/small')
  })
})

// ─── quota.ts ─────────────────────────────────────────────────────────────────

describe('quota', () => {
  it('listQuotas calls apiGet', async () => {
    const { listQuotas } = await import('../quota')
    await listQuotas()
    expect(mockApiGet).toHaveBeenCalledWith('/api/quotas')
  })

  it('getQuota calls apiGet', async () => {
    const { getQuota } = await import('../quota')
    await getQuota('q1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/quotas/q1')
  })

  it('createQuota calls apiPost', async () => {
    const { createQuota } = await import('../quota')
    const req = { name: 'q1', max_cpus: 16, max_memory: 32768, max_disk: 500, max_vms: 10 }
    await createQuota(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/quotas', req)
  })

  it('updateQuota calls apiPut', async () => {
    const { updateQuota } = await import('../quota')
    await updateQuota('q1', { max_cpus: 32 })
    expect(mockApiPut).toHaveBeenCalledWith('/api/quotas/q1', { max_cpus: 32 })
  })

  it('deleteQuota calls apiDelete', async () => {
    const { deleteQuota } = await import('../quota')
    await deleteQuota('q1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/quotas/q1')
  })

  it('enableQuota calls apiPostVoid', async () => {
    const { enableQuota } = await import('../quota')
    await enableQuota('q1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/quotas/q1/enable')
  })

  it('disableQuota calls apiPostVoid', async () => {
    const { disableQuota } = await import('../quota')
    await disableQuota('q1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/quotas/q1/disable')
  })

  it('getQuotaUsage calls apiGet', async () => {
    const { getQuotaUsage } = await import('../quota')
    await getQuotaUsage('q1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/quotas/q1/usage')
  })

  it('getAllQuotaUsage calls apiGet', async () => {
    const { getAllQuotaUsage } = await import('../quota')
    await getAllQuotaUsage()
    expect(mockApiGet).toHaveBeenCalledWith('/api/quotas/usage')
  })
})

// ─── replication.ts ───────────────────────────────────────────────────────────

describe('replication', () => {
  it('listSites calls apiGet', async () => {
    const { listSites } = await import('../replication')
    await listSites()
    expect(mockApiGet).toHaveBeenCalledWith('/api/replication/sites')
  })

  it('registerSite calls apiPost', async () => {
    const { registerSite } = await import('../replication')
    const req = { name: 'site1', endpoint: 'https://site1', site_type: 'primary' as const }
    await registerSite(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/replication/sites', req)
  })

  it('removeSite calls apiDelete', async () => {
    const { removeSite } = await import('../replication')
    await removeSite('s1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/replication/sites/s1')
  })

  it('listReplications calls apiGet without siteId', async () => {
    const { listReplications } = await import('../replication')
    await listReplications()
    expect(mockApiGet).toHaveBeenCalledWith('/api/replication/configs')
  })

  it('listReplications calls apiGet with siteId', async () => {
    const { listReplications } = await import('../replication')
    await listReplications('s1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/replication/configs?site_id=s1')
  })

  it('configureReplication calls apiPost', async () => {
    const { configureReplication } = await import('../replication')
    const req = { vm_id: 'vm1', source_site_id: 's1', target_site_id: 's2', rpo_minutes: 15 }
    await configureReplication(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/replication/configs', req)
  })

  it('pauseReplication calls apiPostVoid', async () => {
    const { pauseReplication } = await import('../replication')
    await pauseReplication('r1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/replication/configs/r1/pause')
  })

  it('resumeReplication calls apiPostVoid', async () => {
    const { resumeReplication } = await import('../replication')
    await resumeReplication('r1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/replication/configs/r1/resume')
  })

  it('getReplicationMetrics calls apiGet', async () => {
    const { getReplicationMetrics } = await import('../replication')
    await getReplicationMetrics('r1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/replication/configs/r1/metrics')
  })

  it('checkRpoViolations calls apiGet', async () => {
    const { checkRpoViolations } = await import('../replication')
    await checkRpoViolations()
    expect(mockApiGet).toHaveBeenCalledWith('/api/replication/rpo-violations')
  })

  it('getReplicationHealth calls apiGet', async () => {
    const { getReplicationHealth } = await import('../replication')
    await getReplicationHealth()
    expect(mockApiGet).toHaveBeenCalledWith('/api/replication/health')
  })
})

// ─── resourcePools.ts ─────────────────────────────────────────────────────────

describe('resourcePools', () => {
  it('listPools calls apiGet without clusterId', async () => {
    const { listPools } = await import('../resourcePools')
    await listPools()
    expect(mockApiGet).toHaveBeenCalledWith('/api/resource-pools')
  })

  it('listPools calls apiGet with clusterId', async () => {
    const { listPools } = await import('../resourcePools')
    await listPools('c1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/resource-pools?cluster_id=c1')
  })

  it('createPool calls apiPost', async () => {
    const { createPool } = await import('../resourcePools')
    const req = { name: 'pool1', cluster_id: 'c1', cpu_shares: 'normal' as const, memory_shares: 'normal' as const }
    await createPool(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/resource-pools', {
      cpu_reservation_mhz: 0,
      cpu_expandable_reservation: false,
      memory_reservation_mb: 0,
      memory_expandable_reservation: false,
      ...req,
    })
  })

  it('getPool calls apiGet', async () => {
    const { getPool } = await import('../resourcePools')
    await getPool('p1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/resource-pools/p1')
  })

  it('updatePool calls apiPut', async () => {
    const { updatePool } = await import('../resourcePools')
    await updatePool('p1', { name: 'updated' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/resource-pools/p1', { name: 'updated' })
  })

  it('deletePool calls apiDelete', async () => {
    const { deletePool } = await import('../resourcePools')
    await deletePool('p1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/resource-pools/p1')
  })

  it('getPoolSummary calls apiGet', async () => {
    const { getPoolSummary } = await import('../resourcePools')
    await getPoolSummary('p1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/resource-pools/p1/summary')
  })

  it('assignVm calls apiPostVoid', async () => {
    const { assignVm } = await import('../resourcePools')
    await assignVm('p1', 'vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/resource-pools/p1/vms', { vm_name: 'vm1' })
  })

  it('unassignVm calls apiDelete', async () => {
    const { unassignVm } = await import('../resourcePools')
    await unassignVm('p1', 'vm1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/resource-pools/p1/vms/vm1')
  })

  it('moveVm calls apiPostVoid', async () => {
    const { moveVm } = await import('../resourcePools')
    await moveVm('vm1', 'p1', 'p2')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/resource-pools/p1/vms/move', { vm_name: 'vm1', target_pool_id: 'p2' })
  })

  it('checkAdmission calls apiPost', async () => {
    const { checkAdmission } = await import('../resourcePools')
    await checkAdmission('p1', { cpu: 4, memory_mb: 8192 })
    expect(mockApiPost).toHaveBeenCalledWith('/api/resource-pools/p1/admission', { cpu_mhz: 4, memory_mb: 8192 })
  })
})

// ─── schedule.ts ──────────────────────────────────────────────────────────────

describe('schedule', () => {
  it('listSchedules calls apiGet', async () => {
    const { listSchedules } = await import('../schedule')
    await listSchedules()
    expect(mockApiGet).toHaveBeenCalledWith('/api/schedules')
  })

  it('getSchedule calls apiGet', async () => {
    const { getSchedule } = await import('../schedule')
    await getSchedule('s1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/schedules/s1')
  })

  it('createSchedule calls apiPost', async () => {
    const { createSchedule } = await import('../schedule')
    const req = { name: 'daily-start', vm_name: 'vm1', action: 'start' as const, schedule_type: 'daily' as const, time: '08:00' }
    await createSchedule(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/schedules', req)
  })

  it('updateSchedule calls apiPut', async () => {
    const { updateSchedule } = await import('../schedule')
    await updateSchedule('s1', { time: '09:00' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/schedules/s1', { time: '09:00' })
  })

  it('deleteSchedule calls apiDelete', async () => {
    const { deleteSchedule } = await import('../schedule')
    await deleteSchedule('s1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/schedules/s1')
  })

  it('enableSchedule calls apiPostVoid', async () => {
    const { enableSchedule } = await import('../schedule')
    await enableSchedule('s1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/schedules/s1/enable')
  })

  it('disableSchedule calls apiPostVoid', async () => {
    const { disableSchedule } = await import('../schedule')
    await disableSchedule('s1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/schedules/s1/disable')
  })

  it('getScheduleHistory calls apiGet with scheduleId', async () => {
    const { getScheduleHistory } = await import('../schedule')
    await getScheduleHistory('s1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/schedules/s1/history')
  })

  it('getScheduleHistory calls apiGet without scheduleId', async () => {
    const { getScheduleHistory } = await import('../schedule')
    await getScheduleHistory()
    expect(mockApiGet).toHaveBeenCalledWith('/api/schedules/history')
  })

  it('runScheduleNow calls apiPostVoid', async () => {
    const { runScheduleNow } = await import('../schedule')
    await runScheduleNow('s1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/schedules/s1/run')
  })
})

// ─── siteRecovery.ts ──────────────────────────────────────────────────────────

describe('siteRecovery', () => {
  it('listPlans calls apiGet', async () => {
    const { listPlans } = await import('../siteRecovery')
    await listPlans()
    expect(mockApiGet).toHaveBeenCalledWith('/api/site-recovery/plans')
  })

  it('createPlan calls apiPost', async () => {
    const { createPlan } = await import('../siteRecovery')
    const req = { name: 'plan1', source_site_id: 's1', target_site_id: 's2', vm_groups: [{ name: 'g1', vm_ids: ['vm1'], boot_order: 1 }] }
    await createPlan(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/site-recovery/plans', req)
  })

  it('getPlan calls apiGet', async () => {
    const { getPlan } = await import('../siteRecovery')
    await getPlan('p1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/site-recovery/plans/p1')
  })

  it('updatePlan calls apiPut', async () => {
    const { updatePlan } = await import('../siteRecovery')
    await updatePlan('p1', { name: 'updated' })
    expect(mockApiPut).toHaveBeenCalledWith('/api/site-recovery/plans/p1', { name: 'updated' })
  })

  it('deletePlan calls apiDelete', async () => {
    const { deletePlan } = await import('../siteRecovery')
    await deletePlan('p1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/site-recovery/plans/p1')
  })

  it('executePlannedMigration calls apiPost', async () => {
    const { executePlannedMigration } = await import('../siteRecovery')
    await executePlannedMigration('p1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/site-recovery/plans/p1/planned-migration')
  })

  it('executeDisasterRecovery calls apiPost', async () => {
    const { executeDisasterRecovery } = await import('../siteRecovery')
    await executeDisasterRecovery('p1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/site-recovery/plans/p1/disaster-recovery')
  })

  it('executeTestFailover calls apiPost', async () => {
    const { executeTestFailover } = await import('../siteRecovery')
    await executeTestFailover('p1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/site-recovery/plans/p1/test-failover')
  })

  it('listExecutions calls apiGet', async () => {
    const { listExecutions } = await import('../siteRecovery')
    await listExecutions('p1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/site-recovery/executions?plan_id=p1')
  })

  it('getExecution calls apiGet', async () => {
    const { getExecution } = await import('../siteRecovery')
    await getExecution('e1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/site-recovery/executions/e1')
  })

  it('cancelExecution calls apiPostVoid', async () => {
    const { cancelExecution } = await import('../siteRecovery')
    await cancelExecution('e1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/site-recovery/executions/e1/cancel')
  })

  it('getDrDashboard calls apiGet', async () => {
    const { getDrDashboard } = await import('../siteRecovery')
    await getDrDashboard()
    expect(mockApiGet).toHaveBeenCalledWith('/api/site-recovery/dashboard')
  })
})

// ─── snapshots.ts ─────────────────────────────────────────────────────────────

describe('snapshots', () => {
  it('listSnapshots calls apiGet', async () => {
    const { listSnapshots } = await import('../snapshots')
    await listSnapshots('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/snapshots')
  })

  it('createSnapshot calls apiPost', async () => {
    const { createSnapshot } = await import('../snapshots')
    const req = { name: 'snap1' }
    await createSnapshot('vm1', req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/vms/vm1/snapshots', req)
  })

  it('createSnapshotWithRetry succeeds on first try', async () => {
    const { createSnapshotWithRetry } = await import('../snapshots')
    const body = JSON.stringify({
      id: '1',
      vm_name: 'vm1',
      name: 'snap1',
      description: null,
      snapshot_type: 'Disk',
      parent_id: null,
      size_bytes: 0,
      created: '2026-01-01T00:00:00Z',
    })
    mockApiFetch.mockResolvedValueOnce({
      ok: true,
      status: 201,
      headers: { get: () => 'application/json' },
      text: async () => body,
    } as unknown as Response)
    const result = await createSnapshotWithRetry('vm1', { name: 'snap1' })
    expect(result.name).toBe('snap1')
  })

  it('createSnapshotWithRetry retries once on 409 then succeeds', async () => {
    const { createSnapshotWithRetry } = await import('../snapshots')
    const body = JSON.stringify({
      id: '1',
      vm_name: 'vm1',
      name: 'snap1',
      description: null,
      snapshot_type: 'Disk',
      parent_id: null,
      size_bytes: 0,
      created: '2026-01-01T00:00:00Z',
    })
    mockApiFetch
      .mockResolvedValueOnce({
        ok: false,
        status: 409,
        statusText: 'Conflict',
        text: async () => JSON.stringify({ error: 'could not reach the VM monitor yet' }),
      } as unknown as Response)
      .mockResolvedValueOnce({
        ok: true,
        status: 201,
        headers: { get: () => 'application/json' },
        text: async () => body,
      } as unknown as Response)
    const result = await createSnapshotWithRetry('vm1', { name: 'snap1' }, { delayMs: 1 })
    expect(result.name).toBe('snap1')
    expect(mockApiFetch).toHaveBeenCalledTimes(2)
  })

  it('getSnapshot calls apiGet', async () => {
    const { getSnapshot } = await import('../snapshots')
    await getSnapshot('vm1', 's1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/snapshots/s1')
  })

  it('deleteSnapshot calls apiDelete', async () => {
    const { deleteSnapshot } = await import('../snapshots')
    await deleteSnapshot('vm1', 's1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/vms/vm1/snapshots/s1')
  })

  it('revertSnapshot calls apiPostVoid', async () => {
    const { revertSnapshot } = await import('../snapshots')
    await revertSnapshot('vm1', 's1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/snapshots/s1/revert')
  })

  it('getSnapshotTree calls apiGet', async () => {
    const { getSnapshotTree } = await import('../snapshots')
    await getSnapshotTree('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/snapshots/tree')
  })
})

// ─── storage.ts ───────────────────────────────────────────────────────────────

describe('storage', () => {
  it('listStoragePools calls apiGet', async () => {
    const { listStoragePools } = await import('../storage')
    await listStoragePools()
    expect(mockApiGet).toHaveBeenCalledWith('/api/storage/pools')
  })

  it('getStoragePool calls apiGet', async () => {
    const { getStoragePool } = await import('../storage')
    await getStoragePool('pool1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/storage/pools/pool1')
  })

  it('createLocalPool calls apiPost', async () => {
    const { createLocalPool } = await import('../storage')
    const req = { name: 'local1', path: '/data', auto_start: true }
    await createLocalPool(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/local', req)
  })

  it('createNfsPool calls apiPost', async () => {
    const { createNfsPool } = await import('../storage')
    const req = { name: 'nfs1', config: { server: '10.0.0.1', export_path: '/export', mount_path: '/mnt', mount_options: [], auto_start: true, nfs_version: 'V4' as const } }
    await createNfsPool(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/nfs', req)
  })

  it('createLvmPool calls apiPost', async () => {
    const { createLvmPool } = await import('../storage')
    await createLvmPool({ name: 'lvm1', volume_group: 'vg0', auto_start: true })
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/lvm', { name: 'lvm1', volume_group: 'vg0', auto_start: true })
  })

  it('createLvmThinPool calls apiPost', async () => {
    const { createLvmThinPool } = await import('../storage')
    await createLvmThinPool({ name: 'thin1', volume_group: 'vg0', thin_pool: 'tp0', auto_start: true })
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/lvm-thin', { name: 'thin1', volume_group: 'vg0', thin_pool: 'tp0', auto_start: true })
  })

  it('createZfsPool calls apiPost', async () => {
    const { createZfsPool } = await import('../storage')
    await createZfsPool({ name: 'zfs1', zpool: 'rpool', auto_start: true })
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/zfs', { name: 'zfs1', zpool: 'rpool', auto_start: true })
  })

  it('deleteStoragePool calls apiDelete', async () => {
    const { deleteStoragePool } = await import('../storage')
    await deleteStoragePool('pool1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/storage/pools/pool1')
  })

  it('startStoragePool calls apiPostVoid', async () => {
    const { startStoragePool } = await import('../storage')
    await startStoragePool('pool1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/storage/pools/pool1/start')
  })

  it('stopStoragePool calls apiPostVoid', async () => {
    const { stopStoragePool } = await import('../storage')
    await stopStoragePool('pool1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/storage/pools/pool1/stop')
  })

  it('getNfsHealth calls apiGet', async () => {
    const { getNfsHealth } = await import('../storage')
    await getNfsHealth('nfs1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/storage/pools/nfs1/health')
  })

  it('getNfsStats calls apiGet', async () => {
    const { getNfsStats } = await import('../storage')
    await getNfsStats('nfs1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/storage/pools/nfs1/stats')
  })

  it('refreshPoolStats calls apiPostVoid', async () => {
    const { refreshPoolStats } = await import('../storage')
    await refreshPoolStats('pool1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/storage/pools/pool1/refresh')
  })
})

// ─── templates.ts ─────────────────────────────────────────────────────────────

describe('templates', () => {
  it('listTemplates calls apiGet', async () => {
    const { listTemplates } = await import('../templates')
    await listTemplates()
    expect(mockApiGet).toHaveBeenCalledWith('/api/templates')
  })

  it('getTemplate calls apiGet', async () => {
    const { getTemplate } = await import('../templates')
    await getTemplate('t1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/templates/t1')
  })

  it('createTemplate calls apiPost', async () => {
    const { createTemplate } = await import('../templates')
    const req = { name: 't1', cpus: 2, memory: 2048, disk: 20, image: 'ubuntu.img' }
    await createTemplate(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/templates', req)
  })

  it('updateTemplate calls apiPut', async () => {
    const { updateTemplate } = await import('../templates')
    await updateTemplate('t1', { cpus: 4 })
    expect(mockApiPut).toHaveBeenCalledWith('/api/templates/t1', { cpus: 4 })
  })

  it('deleteTemplate calls apiDelete', async () => {
    const { deleteTemplate } = await import('../templates')
    await deleteTemplate('t1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/templates/t1')
  })

  it('deployTemplate calls apiPostVoid', async () => {
    const { deployTemplate } = await import('../templates')
    await deployTemplate('t1', 'new-vm')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/templates/t1/deploy', { vm_name: 'new-vm' })
  })
})

// ─── volumes.ts ───────────────────────────────────────────────────────────────

describe('volumes', () => {
  it('listVolumes calls apiGet', async () => {
    const { listVolumes } = await import('../volumes')
    await listVolumes('pool1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/storage/pools/pool1/volumes')
  })

  it('createVolume calls apiPost', async () => {
    const { createVolume } = await import('../volumes')
    await createVolume('pool1', { name: 'vol1', size: '10G' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/pool1/volumes', { name: 'vol1', size: '10G' })
  })

  it('getVolume calls apiGet', async () => {
    const { getVolume } = await import('../volumes')
    await getVolume('pool1', 'v1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/storage/pools/pool1/volumes/v1')
  })

  it('deleteVolume calls apiDelete', async () => {
    const { deleteVolume } = await import('../volumes')
    await deleteVolume('pool1', 'v1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/storage/pools/pool1/volumes/v1')
  })

  it('resizeVolume calls apiPost', async () => {
    const { resizeVolume } = await import('../volumes')
    await resizeVolume('pool1', 'v1', { size: '20G' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/pool1/volumes/v1/resize', { size: '20G' })
  })

  it('attachVolume calls apiPost', async () => {
    const { attachVolume } = await import('../volumes')
    await attachVolume('pool1', 'v1', { vm_name: 'vm1' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/pool1/volumes/v1/attach', { vm_name: 'vm1' })
  })

  it('detachVolume calls apiPost', async () => {
    const { detachVolume } = await import('../volumes')
    await detachVolume('pool1', 'v1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/storage/pools/pool1/volumes/v1/detach')
  })
})

// ─── zones.ts ─────────────────────────────────────────────────────────────────

describe('zones', () => {
  it('listZones calls apiGet', async () => {
    const { listZones } = await import('../zones')
    await listZones()
    expect(mockApiGet).toHaveBeenCalledWith('/api/zones')
  })

  it('createZone calls apiPost', async () => {
    const { createZone } = await import('../zones')
    await createZone({ name: 'zone1' })
    expect(mockApiPost).toHaveBeenCalledWith('/api/zones', { name: 'zone1' })
  })

  it('deleteZone calls apiDelete', async () => {
    const { deleteZone } = await import('../zones')
    await deleteZone('z1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/zones/z1')
  })

  it('listSpotInstances calls apiGet', async () => {
    const { listSpotInstances } = await import('../zones')
    await listSpotInstances()
    expect(mockApiGet).toHaveBeenCalledWith('/api/spot-instances')
  })

  it('evictSpotInstance calls apiPost', async () => {
    const { evictSpotInstance } = await import('../zones')
    await evictSpotInstance('si1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/spot-instances/si1/evict')
  })
})

// ─── vm.ts ────────────────────────────────────────────────────────────────────

describe('vm', () => {
  it('listVMs calls apiGet', async () => {
    const { listVMs } = await import('../vm')
    await listVMs()
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms')
  })

  it('getVM calls apiGet', async () => {
    const { getVM } = await import('../vm')
    await getVM('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1')
  })

  it('createVM calls apiPost', async () => {
    const { createVM } = await import('../vm')
    const req = { name: 'vm1', image: 'ubuntu.img', cpus: 2, memory: 2048 }
    await createVM(req)
    expect(mockApiPost).toHaveBeenCalledWith('/api/vms', req)
  })

  it('deleteVM calls apiDelete', async () => {
    const { deleteVM } = await import('../vm')
    await deleteVM('vm1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/vms/vm1')
  })

  it('startVM calls apiPostVoid', async () => {
    const { startVM } = await import('../vm')
    await startVM('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/start')
  })

  it('stopVM calls apiPostVoid', async () => {
    const { stopVM } = await import('../vm')
    await stopVM('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/stop')
  })

  it('restartVM calls apiPostVoid', async () => {
    const { restartVM } = await import('../vm')
    await restartVM('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/restart')
  })

  it('pauseVM calls apiPostVoid', async () => {
    const { pauseVM } = await import('../vm')
    await pauseVM('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/pause')
  })

  it('resumeVM calls apiPostVoid', async () => {
    const { resumeVM } = await import('../vm')
    await resumeVM('vm1')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/resume')
  })

  it('getMetrics calls apiGet', async () => {
    const { getMetrics } = await import('../vm')
    await getMetrics('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/metrics')
  })

  it('cloneVM calls apiPostVoid with defaults', async () => {
    const { cloneVM } = await import('../vm')
    await cloneVM('vm1', 'vm1-clone')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/clone', {
      target_name: 'vm1-clone',
      include_snapshots: false,
      linked_clone: false,
    })
  })

  it('cloneVM calls apiPostVoid with options', async () => {
    const { cloneVM } = await import('../vm')
    await cloneVM('vm1', 'vm1-clone', { includeSnapshots: true, linkedClone: true })
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/clone', {
      target_name: 'vm1-clone',
      include_snapshots: true,
      linked_clone: true,
    })
  })

  it('addTag calls apiPostVoid', async () => {
    const { addTag } = await import('../vm')
    await addTag('vm1', 'prod')
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/tags', { tag: 'prod' })
  })

  it('removeTag calls apiDelete', async () => {
    const { removeTag } = await import('../vm')
    await removeTag('vm1', 'prod')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/vms/vm1/tags/prod')
  })

  it('updateTags calls apiPutVoid', async () => {
    const { updateTags } = await import('../vm')
    await updateTags('vm1', ['prod', 'web'])
    expect(mockApiPutVoid).toHaveBeenCalledWith('/api/vms/vm1/tags', { tags: ['prod', 'web'] })
  })
})

// ─── system.ts ────────────────────────────────────────────────────────────────

describe('system', () => {
  it('getCpuTopology calls apiGet', async () => {
    const { getCpuTopology } = await import('../system')
    await getCpuTopology()
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/cpu/topology')
  })

  it('getNumaTopology calls apiGet', async () => {
    const { getNumaTopology } = await import('../system')
    await getNumaTopology()
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/numa/topology')
  })

  it('getNumaNode calls apiGet', async () => {
    const { getNumaNode } = await import('../system')
    await getNumaNode(0)
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/numa/nodes/0')
  })

  it('getNumaPlacement calls apiGet', async () => {
    const { getNumaPlacement } = await import('../system')
    await getNumaPlacement(4096, 4)
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/numa/placement?memory_mb=4096&cpus=4')
  })

  it('setCpuPinning calls apiPostVoid', async () => {
    const { setCpuPinning } = await import('../system')
    await setCpuPinning('vm1', { pinning: { type: 'Auto' } })
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/cpu/pin', { pinning: { type: 'Auto' } })
  })

  it('removeCpuPinning calls apiDelete', async () => {
    const { removeCpuPinning } = await import('../system')
    await removeCpuPinning('vm1')
    expect(mockApiDelete).toHaveBeenCalledWith('/api/vms/vm1/cpu/pin')
  })

  it('getCpuAffinity calls apiGet', async () => {
    const { getCpuAffinity } = await import('../system')
    await getCpuAffinity('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/cpu/affinity')
  })

  it('setMemoryLimit calls apiPutVoid', async () => {
    const { setMemoryLimit } = await import('../system')
    await setMemoryLimit('vm1', { limit_bytes: 4294967296 })
    expect(mockApiPutVoid).toHaveBeenCalledWith('/api/vms/vm1/memory/limit', { limit_bytes: 4294967296 })
  })

  it('getMemoryUsage calls apiGet', async () => {
    const { getMemoryUsage } = await import('../system')
    await getMemoryUsage('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/memory/usage')
  })

  it('setMemoryBallooning calls apiPostVoid', async () => {
    const { setMemoryBallooning } = await import('../system')
    await setMemoryBallooning('vm1', true)
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/vms/vm1/memory/balloon', { enabled: true })
  })

  it('getHugepageStats calls apiGet', async () => {
    const { getHugepageStats } = await import('../system')
    await getHugepageStats('Size2MB')
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/memory/hugepages?size=Size2MB')
  })

  it('allocateHugepages calls apiPostVoid', async () => {
    const { allocateHugepages } = await import('../system')
    await allocateHugepages({ size: 'Size1GB', count: 4 })
    expect(mockApiPostVoid).toHaveBeenCalledWith('/api/system/memory/hugepages', { size: 'Size1GB', count: 4 })
  })

  it('getSystemMemory calls apiGet', async () => {
    const { getSystemMemory } = await import('../system')
    await getSystemMemory()
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/memory')
  })

  it('getOptimizationRecommendations calls apiGet', async () => {
    const { getOptimizationRecommendations } = await import('../system')
    await getOptimizationRecommendations()
    expect(mockApiGet).toHaveBeenCalledWith('/api/system/optimization/recommendations')
  })

  it('optimizeVM calls apiPost', async () => {
    const { optimizeVM } = await import('../system')
    await optimizeVM('vm1')
    expect(mockApiPost).toHaveBeenCalledWith('/api/vms/vm1/optimize')
  })
})

// ─── agents.ts (catch-up) ─────────────────────────────────────────────────────

describe('agents', () => {
  it('listAgents calls apiGet', async () => {
    const { listAgents } = await import('../agents')
    await listAgents()
    expect(mockApiGet).toHaveBeenCalledWith('/api/agents')
  })

  it('listSessions calls apiGet', async () => {
    const { listSessions } = await import('../agents')
    await listSessions()
    expect(mockApiGet).toHaveBeenCalledWith('/api/sessions')
  })

  it('sessionAction posts to the action path', async () => {
    const { sessionAction } = await import('../agents')
    await sessionAction('sess-1', 'cancel')
    expect(mockApiPost).toHaveBeenCalledWith('/api/sessions/sess-1/cancel', {})
  })
})

// ─── dataplane.ts (catch-up) ──────────────────────────────────────────────────

describe('dataplane catch-up paths', () => {
  it('getDataplaneStatus calls apiGet', async () => {
    const { getDataplaneStatus } = await import('../dataplane')
    await getDataplaneStatus('vm 1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm%201/dataplane/status')
  })

  it('getDataplaneStats calls apiGet', async () => {
    const { getDataplaneStats } = await import('../dataplane')
    await getDataplaneStats('vm1')
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/dataplane/stats')
  })

  it('getDataplaneDropReasons calls apiGet with limit', async () => {
    const { getDataplaneDropReasons } = await import('../dataplane')
    await getDataplaneDropReasons('vm1', 50)
    expect(mockApiGet).toHaveBeenCalledWith('/api/vms/vm1/dataplane/drop-reasons?limit=50')
  })
})
