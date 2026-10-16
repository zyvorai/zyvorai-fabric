// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { BrowserRouter, Routes, Route, Navigate, useLocation } from 'react-router'
import { Suspense, lazy, ReactNode } from 'react'
import { ThemeProvider } from './contexts/ThemeContext'
import { ToastProvider } from './contexts/ToastContext'
import { WebSocketProvider } from './contexts/WebSocketContext'
import { AuthProvider, useAuth } from './contexts/AuthContext'
import { PlatformInfoProvider } from './contexts/PlatformInfoContext'
import PageSkeleton from './components/PageSkeleton'
import { PageErrorBoundary } from './components/ErrorBoundary'
import ConsoleLayout from './components/ConsoleLayout'
import SignIn from './pages/SignIn'
import NotFound from './pages/NotFound'
import Home from './pages/marketing/Home'
import Product from './pages/marketing/Product'
import Platform from './pages/marketing/Platform'
import Keep from './pages/marketing/Keep'
import SecurityPage from './pages/marketing/Security'

const Dashboard = lazy(() => import('./pages/Dashboard'))
const VMList = lazy(() => import('./pages/VMList'))
const VMDetails = lazy(() => import('./pages/VMDetails'))
const CreateVM = lazy(() => import('./pages/CreateVM'))
const Console = lazy(() => import('./pages/Console'))
const Logs = lazy(() => import('./pages/Logs'))
const Network = lazy(() => import('./pages/Network'))
const Storage = lazy(() => import('./pages/Storage'))
const Zones = lazy(() => import('./pages/Zones'))
const Settings = lazy(() => import('./pages/Settings'))
const Templates = lazy(() => import('./pages/Templates'))
const Quotas = lazy(() => import('./pages/Quotas'))
const ContainerGroups = lazy(() => import('./pages/ContainerGroups'))
const Agents = lazy(() => import('./pages/Agents'))
const AiWorkloads = lazy(() => import('./pages/AiWorkloads'))
const Sessions = lazy(() => import('./pages/Sessions'))
const KeepHome = lazy(() => import('./pages/KeepHome'))
const KeepSession = lazy(() => import('./pages/KeepSession'))
const KeepHistory = lazy(() => import('./pages/KeepHistory'))
const Schedules = lazy(() => import('./pages/Schedules'))
const AuditLogs = lazy(() => import('./pages/AuditLogs'))
const Analytics = lazy(() => import('./pages/Analytics'))
const Backups = lazy(() => import('./pages/Backups'))
const Notifications = lazy(() => import('./pages/Notifications'))
const StoragePools = lazy(() => import('./pages/StoragePools'))
const SystemResources = lazy(() => import('./pages/SystemResources'))
const Datacenters = lazy(() => import('./pages/Datacenters'))
const ResourcePools = lazy(() => import('./pages/ResourcePools'))
const WarmPools = lazy(() => import('./pages/WarmPools'))
const DRS = lazy(() => import('./pages/DRS'))
const DistributedStorage = lazy(() => import('./pages/DistributedStorage'))
const Encryption = lazy(() => import('./pages/Encryption'))
const FaultTolerance = lazy(() => import('./pages/FaultTolerance'))
const Replication = lazy(() => import('./pages/Replication'))
const SiteRecovery = lazy(() => import('./pages/SiteRecovery'))
const ContentLibrary = lazy(() => import('./pages/ContentLibrary'))
const LifecycleManager = lazy(() => import('./pages/LifecycleManager'))
const Certificates = lazy(() => import('./pages/Certificates'))
const Migrations = lazy(() => import('./pages/Migrations'))
const Profiles = lazy(() => import('./pages/Profiles'))
const Snapshots = lazy(() => import('./pages/Snapshots'))
const NetworkSecurity = lazy(() => import('./pages/NetworkSecurity'))
const EdgeDataplane = lazy(() => import('./pages/EdgeDataplane'))
const Processes = lazy(() => import('./pages/Processes'))
const SecurityDashboard = lazy(() => import('./pages/SecurityDashboard'))
const Kernel = lazy(() => import('./pages/Kernel'))
const Alerts = lazy(() => import('./pages/Alerts'))
const Debug = lazy(() => import('./pages/Debug'))
const Explain = lazy(() => import('./pages/Explain'))
const Timeline = lazy(() => import('./pages/Timeline'))
const Webhooks = lazy(() => import('./pages/Webhooks'))
const APIPlayground = lazy(() => import('./pages/APIPlayground'))
const CostEstimator = lazy(() => import('./pages/CostEstimator'))
const SystemHealth = lazy(() => import('./pages/SystemHealth'))
const ContainersPage = lazy(() => import('./pages/Containers'))
const DiskConverter = lazy(() => import('./pages/DiskConverter'))
const VMComparePage = lazy(() => import('./pages/VMCompare'))
const VMHealthCheckPage = lazy(() => import('./pages/VMHealthCheck'))
const NotificationCenterPage = lazy(() => import('./pages/NotificationCenter'))
const FavoriteVMs = lazy(() => import('./pages/FavoriteVMs'))
const BulkOperationsPage = lazy(() => import('./pages/BulkOperations'))
const ISOImages = lazy(() => import('./pages/ISOImages'))
const DownloadDisk = lazy(() => import('./pages/DownloadDisk'))
const UploadDiskPage = lazy(() => import('./pages/UploadDisk'))
const PipelineMonitorPage = lazy(() => import('./pages/PipelineMonitor'))
const MigrationReadinessPage = lazy(() => import('./pages/MigrationReadiness'))
const MigrationHistoryPage = lazy(() => import('./pages/MigrationHistory'))
const MigrationReportPage = lazy(() => import('./pages/MigrationReport'))
const NetworkTopologyPage = lazy(() => import('./pages/NetworkTopology'))
const BackupSchedulerPage = lazy(() => import('./pages/BackupScheduler'))
const BatchImportPage = lazy(() => import('./pages/BatchImport'))
const ResourceOptimizerPage = lazy(() => import('./pages/ResourceOptimizer'))
const CapacityPlanningPage = lazy(() => import('./pages/CapacityPlanning'))
const ComplianceDashboardPage = lazy(() => import('./pages/ComplianceDashboard'))
const LiveMetricsPage = lazy(() => import('./pages/LiveMetrics'))
const AccessControlPage = lazy(() => import('./pages/AccessControl'))
const PluginManagerPage = lazy(() => import('./pages/PluginManager'))
const ServiceMapPage = lazy(() => import('./pages/ServiceMap'))
const EventStreamPage = lazy(() => import('./pages/EventStream'))
const JobMonitorPage = lazy(() => import('./pages/JobMonitor'))
const ManifestBuilderPage = lazy(() => import('./pages/ManifestBuilder'))
const DiskImagesPage = lazy(() => import('./pages/DiskImages'))
const VMBrowserPage = lazy(() => import('./pages/VMBrowser'))
const MigrationWizardPage = lazy(() => import('./pages/MigrationWizard'))
const BatchMigrationBuilderPage = lazy(() => import('./pages/BatchMigrationBuilder'))
const MigrationTemplatesPage = lazy(() => import('./pages/MigrationTemplates'))
const SnapshotManagerPage = lazy(() => import('./pages/SnapshotManager'))
const StorageManagerPage = lazy(() => import('./pages/StorageManager'))
const AutoscalePage = lazy(() => import('./pages/Autoscale'))

function ProtectedRoute({ children }: { children: ReactNode }) {
  const { isAuthenticated, loading } = useAuth()

  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-[var(--zf-canvas)]">
        <div className="flex flex-col items-center gap-3">
          <div className="w-7 h-7 border-2 border-[var(--zf-ink)] border-t-transparent rounded-full animate-spin" />
          <span className="text-sm font-medium text-[var(--zf-muted)] tracking-wide">Loading…</span>
        </div>
      </div>
    )
  }

  if (!isAuthenticated) {
    return <Navigate to="/sign-in" replace />
  }

  return <>{children}</>
}

function ConsoleRoutes() {
  return (
    <ConsoleLayout>
      <PageErrorBoundary>
        <Suspense fallback={<PageSkeleton />}>
          <Routes>
            <Route index element={<Dashboard />} />
            <Route path="vms" element={<VMList />} />
            <Route path="vms/:name" element={<VMDetails />} />
            <Route path="vms/:name/console" element={<Console />} />
            <Route path="create" element={<CreateVM />} />
            <Route path="logs" element={<Logs />} />
            <Route path="network" element={<Network />} />
            <Route path="storage" element={<Storage />} />
            <Route path="templates" element={<Templates />} />
            <Route path="quotas" element={<Quotas />} />
            <Route path="container-groups" element={<ContainerGroups />} />
            <Route path="ai" element={<AiWorkloads />} />
            <Route path="agents" element={<Agents />} />
            <Route path="sessions" element={<Sessions />} />
            <Route path="sessions/:id" element={<Sessions />} />
            <Route path="merion" element={<KeepHome />} />
            <Route path="merion/history" element={<KeepHistory />} />
            <Route path="merion/:sessionId" element={<KeepSession />} />
            <Route path="schedules" element={<Schedules />} />
            <Route path="autoscale" element={<AutoscalePage />} />
            <Route path="zones" element={<Zones />} />
            <Route path="audit" element={<AuditLogs />} />
            <Route path="analytics" element={<Analytics />} />
            <Route path="backups" element={<Backups />} />
            <Route path="notifications" element={<Notifications />} />
            <Route path="storage-pools" element={<StoragePools />} />
            <Route path="system" element={<SystemResources />} />
            <Route path="settings" element={<Settings />} />
            <Route path="datacenters" element={<Datacenters />} />
            <Route path="resource-pools" element={<ResourcePools />} />
            <Route path="vm-pools" element={<WarmPools />} />
            <Route path="drs" element={<DRS />} />
            <Route path="distributed-storage" element={<DistributedStorage />} />
            <Route path="encryption" element={<Encryption />} />
            <Route path="fault-tolerance" element={<FaultTolerance />} />
            <Route path="replication" element={<Replication />} />
            <Route path="site-recovery" element={<SiteRecovery />} />
            <Route path="migrations" element={<Migrations />} />
            <Route path="profiles" element={<Profiles />} />
            <Route path="snapshots" element={<Snapshots />} />
            <Route path="content-library" element={<ContentLibrary />} />
            <Route path="lifecycle" element={<LifecycleManager />} />
            <Route path="certificates" element={<Certificates />} />
            <Route path="network-security" element={<NetworkSecurity />} />
            <Route path="edge-dataplane" element={<EdgeDataplane />} />
            <Route path="processes" element={<Processes />} />
            <Route path="security-dashboard" element={<SecurityDashboard />} />
            <Route path="kernel" element={<Kernel />} />
            <Route path="alerts" element={<Alerts />} />
            <Route path="debug" element={<Debug />} />
            <Route path="explain" element={<Explain />} />
            <Route path="timeline" element={<Timeline />} />
            <Route path="webhooks" element={<Webhooks />} />
            <Route path="playground" element={<APIPlayground />} />
            <Route path="cost-estimator" element={<CostEstimator />} />
            <Route path="system-health" element={<SystemHealth />} />
            <Route path="containers" element={<ContainersPage />} />
            <Route path="disk-converter" element={<DiskConverter />} />
            <Route path="vm-compare" element={<VMComparePage />} />
            <Route path="vm-healthcheck" element={<VMHealthCheckPage />} />
            <Route path="notification-center" element={<NotificationCenterPage />} />
            <Route path="favorites" element={<FavoriteVMs />} />
            <Route path="bulk-operations" element={<BulkOperationsPage />} />
            <Route path="iso-images" element={<ISOImages />} />
            <Route path="download-disk" element={<DownloadDisk />} />
            <Route path="upload-disk" element={<UploadDiskPage />} />
            <Route path="pipeline" element={<PipelineMonitorPage />} />
            <Route path="migration-readiness" element={<MigrationReadinessPage />} />
            <Route path="migration-history" element={<MigrationHistoryPage />} />
            <Route path="migration-report" element={<MigrationReportPage />} />
            <Route path="network-topology" element={<NetworkTopologyPage />} />
            <Route path="backup-scheduler" element={<BackupSchedulerPage />} />
            <Route path="batch-import" element={<BatchImportPage />} />
            <Route path="resource-optimizer" element={<ResourceOptimizerPage />} />
            <Route path="capacity-planning" element={<CapacityPlanningPage />} />
            <Route path="compliance" element={<ComplianceDashboardPage />} />
            <Route path="live-metrics" element={<LiveMetricsPage />} />
            <Route path="access-control" element={<AccessControlPage />} />
            <Route path="plugins" element={<PluginManagerPage />} />
            <Route path="service-map" element={<ServiceMapPage />} />
            <Route path="event-stream" element={<EventStreamPage />} />
            <Route path="job-monitor" element={<JobMonitorPage />} />
            <Route path="manifest-builder" element={<ManifestBuilderPage />} />
            <Route path="disk-images" element={<DiskImagesPage />} />
            <Route path="vm-browser" element={<VMBrowserPage />} />
            <Route path="migration-wizard" element={<MigrationWizardPage />} />
            <Route path="batch-migration" element={<BatchMigrationBuilderPage />} />
            <Route path="migration-templates" element={<MigrationTemplatesPage />} />
            <Route path="snapshot-manager" element={<SnapshotManagerPage />} />
            <Route path="storage-manager" element={<StorageManagerPage />} />
            <Route path="vm-wizard" element={<Navigate to="/app/create" replace />} />
            <Route path="*" element={<NotFound />} />
          </Routes>
        </Suspense>
      </PageErrorBoundary>
    </ConsoleLayout>
  )
}

/** Legacy bookmark → /app/... */
function LegacyRedirect() {
  const { pathname } = useLocation()
  if (pathname === '/' || pathname === '') return <Navigate to="/app" replace />
  if (pathname.startsWith('/app')) return <Navigate to="/app" replace />
  return <Navigate to={`/app${pathname}`} replace />
}

function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Home />} />
      <Route path="/product" element={<Product />} />
      <Route path="/platform" element={<Platform />} />
      <Route path="/merion" element={<Keep />} />
      <Route path="/security" element={<SecurityPage />} />
      <Route path="/sign-in" element={<SignIn />} />
      <Route path="/login" element={<Navigate to="/sign-in" replace />} />
      <Route
        path="/app/*"
        element={
          <ProtectedRoute>
            <ConsoleRoutes />
          </ProtectedRoute>
        }
      />
      {/* Legacy ops paths (pre-/app) */}
      <Route path="/vms/*" element={<LegacyRedirect />} />
      <Route path="/create" element={<Navigate to="/app/create" replace />} />
      <Route path="/network" element={<Navigate to="/app/network" replace />} />
      <Route path="/storage" element={<Navigate to="/app/storage" replace />} />
      <Route path="/settings" element={<Navigate to="/app/settings" replace />} />
      <Route path="/vms" element={<Navigate to="/app/vms" replace />} />
      <Route path="/logs" element={<Navigate to="/app/logs" replace />} />
      <Route path="/templates" element={<Navigate to="/app/templates" replace />} />
      <Route path="/backups" element={<Navigate to="/app/backups" replace />} />
      <Route path="/favorites" element={<Navigate to="/app/favorites" replace />} />
      <Route path="/network-security" element={<Navigate to="/app/network-security" replace />} />
      <Route path="/playground" element={<Navigate to="/app/playground" replace />} />
      <Route path="*" element={<NotFound />} />
    </Routes>
  )
}

function App() {
  return (
    <ThemeProvider>
      <AuthProvider>
        <ToastProvider>
          <WebSocketProvider>
            <PlatformInfoProvider>
              <BrowserRouter>
                <AppRoutes />
              </BrowserRouter>
            </PlatformInfoProvider>
          </WebSocketProvider>
        </ToastProvider>
      </AuthProvider>
    </ThemeProvider>
  )
}

export default App
