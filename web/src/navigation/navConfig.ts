// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { LucideIcon } from 'lucide-react'
import {
  Home, Server, Monitor, Layers, Building2, Star, Network, Shield, HardDrive, Database, Cpu,
  HeartPulse, Container, GitBranch, Zap, RefreshCw, Activity, ArrowRightLeft, CheckCircle, Clock,
  FileText, Calendar, TrendingUp, Camera, Save, PackageCheck, Archive, Package, Bell, Terminal,
  BarChart3, AlertTriangle, Bug, HelpCircle, Radio, Lock, Key, Users, Settings, Globe, DollarSign,
  Upload, Download, Disc, Workflow, FileUp, Map, Inbox, Boxes, Wrench, MoreHorizontal, Code, Bot,
} from 'lucide-react'

export interface NavItem {
  label: string
  path: string
  icon: LucideIcon
}

export interface NavSection {
  label: string
  items: NavItem[]
}

export interface NavGroup {
  name: string
  compact: string
  barIcon: LucideIcon
  items?: NavItem[]
  sections?: NavSection[]
}

export function flattenNavGroup(g: NavGroup): NavItem[] {
  if (g.items?.length) return g.items
  if (g.sections?.length) return g.sections.flatMap((s) => s.items)
  return []
}

export function navDropdownSections(group: NavGroup): NavSection[] {
  if (group.items?.length) return [{ label: '', items: group.items }]
  return group.sections ?? []
}

/** Always-visible icon shortcuts beside the main nav cluster. */
export const TOP_BAR_QUICK_LINKS: NavItem[] = [
  { label: 'Settings', path: '/app/settings', icon: Settings },
  { label: 'API Playground', path: '/app/playground', icon: Code },
]

function dedupeNavPaths(items: NavItem[]): NavItem[] {
  const seen = new Set<string>()
  return items.filter((item) => {
    if (seen.has(item.path)) return false
    seen.add(item.path)
    return true
  })
}

export const NAV_GROUPS: NavGroup[] = [
  {
    name: 'Core',
    compact: 'Core',
    barIcon: Home,
    items: [
      { label: 'Dashboard', path: '/app', icon: Home },
      { label: 'Favorites', path: '/app/favorites', icon: Star },
      { label: 'Virtual Machines', path: '/app/vms', icon: Server },
      { label: 'Warm Pools', path: '/app/vm-pools', icon: PackageCheck },
      { label: 'Profiles', path: '/app/profiles', icon: Layers },
      { label: 'Datacenters', path: '/app/datacenters', icon: Building2 },
      { label: 'VM Browser', path: '/app/vm-browser', icon: Monitor },
    ],
  },
  {
    name: 'Infrastructure',
    compact: 'Infra',
    barIcon: Boxes,
    items: [
      { label: 'Network', path: '/app/network', icon: Network },
      { label: 'Net Security', path: '/app/network-security', icon: Shield },
      { label: 'Edge Dataplane', path: '/app/edge-dataplane', icon: Activity },
      { label: 'Storage', path: '/app/storage', icon: HardDrive },
      { label: 'Storage Pools', path: '/app/storage-pools', icon: Database },
      { label: 'Distributed Storage', path: '/app/distributed-storage', icon: Database },
      { label: 'Resource Pools', path: '/app/resource-pools', icon: Layers },
      { label: 'System', path: '/app/system', icon: Cpu },
      { label: 'System Health', path: '/app/system-health', icon: HeartPulse },
      { label: 'Containers', path: '/app/containers', icon: Container },
    ],
  },
  {
    name: 'Operations',
    compact: 'Ops',
    barIcon: Zap,
    items: [
      { label: 'DRS', path: '/app/drs', icon: GitBranch },
      { label: 'Fault Tolerance', path: '/app/fault-tolerance', icon: Zap },
      { label: 'Replication', path: '/app/replication', icon: RefreshCw },
      { label: 'Site Recovery', path: '/app/site-recovery', icon: Activity },
      { label: 'Migrations', path: '/app/migrations', icon: ArrowRightLeft },
      { label: 'Migration Wizard', path: '/app/migration-wizard', icon: ArrowRightLeft },
      { label: 'Templates', path: '/app/templates', icon: Layers },
      { label: 'Content Library', path: '/app/content-library', icon: Archive },
      { label: 'Schedules', path: '/app/schedules', icon: Calendar },
      { label: 'Autoscale', path: '/app/autoscale', icon: TrendingUp },
      { label: 'Availability Zones', path: '/app/zones', icon: Globe },
      { label: 'Snapshots', path: '/app/snapshots', icon: Camera },
      { label: 'Backups', path: '/app/backups', icon: Save },
      { label: 'Container Groups', path: '/app/container-groups', icon: Boxes },
      { label: 'AI Workloads', path: '/app/ai', icon: Bot },
      { label: 'Merion', path: '/app/merion', icon: Shield },
      { label: 'Agents', path: '/app/agents', icon: Bot },
      { label: 'Sessions', path: '/app/sessions', icon: Activity },
      { label: 'Quotas', path: '/app/quotas', icon: Shield },
      { label: 'Lifecycle', path: '/app/lifecycle', icon: PackageCheck },
      { label: 'Bulk Operations', path: '/app/bulk-operations', icon: Layers },
    ],
  },
  {
    name: 'Monitoring',
    compact: 'Observe',
    barIcon: Activity,
    items: [
      { label: 'Logs', path: '/app/logs', icon: Terminal },
      { label: 'Analytics', path: '/app/analytics', icon: BarChart3 },
      { label: 'Audit', path: '/app/audit', icon: FileText },
      { label: 'Notifications', path: '/app/notifications', icon: Bell },
      { label: 'Alerts', path: '/app/alerts', icon: AlertTriangle },
      { label: 'Timeline', path: '/app/timeline', icon: Clock },
      { label: 'Processes', path: '/app/processes', icon: Cpu },
      { label: 'Kernel', path: '/app/kernel', icon: Server },
      { label: 'Debug Tools', path: '/app/debug', icon: Bug },
      { label: 'Explain', path: '/app/explain', icon: HelpCircle },
      { label: 'Live Metrics', path: '/app/live-metrics', icon: Activity },
      { label: 'Event Stream', path: '/app/event-stream', icon: Radio },
      { label: 'Optimizer', path: '/app/resource-optimizer', icon: Zap },
      { label: 'Capacity', path: '/app/capacity-planning', icon: TrendingUp },
      { label: 'Service Map', path: '/app/service-map', icon: GitBranch },
    ],
  },
  {
    name: 'Security',
    compact: 'Secure',
    barIcon: Shield,
    items: [
      { label: 'Security Dashboard', path: '/app/security-dashboard', icon: Shield },
      { label: 'Encryption', path: '/app/encryption', icon: Lock },
      { label: 'Certificates', path: '/app/certificates', icon: Key },
      { label: 'Compliance', path: '/app/compliance', icon: Shield },
      { label: 'Access Control', path: '/app/access-control', icon: Users },
      { label: 'Plugins', path: '/app/plugins', icon: Package },
    ],
  },
  {
    name: 'Tools',
    compact: 'Tools',
    barIcon: Wrench,
    items: [
      { label: 'Webhooks', path: '/app/webhooks', icon: Globe },
      { label: 'Cost Estimator', path: '/app/cost-estimator', icon: DollarSign },
      { label: 'VM Compare', path: '/app/vm-compare', icon: ArrowRightLeft },
      { label: 'VM Health Check', path: '/app/vm-healthcheck', icon: HeartPulse },
      { label: 'Notification Center', path: '/app/notification-center', icon: Inbox },
    ],
  },
  {
    name: 'More — images, migrations & managers',
    compact: 'More',
    barIcon: MoreHorizontal,
    sections: [
      {
        label: 'Migration advanced',
        items: [
          { label: 'Readiness', path: '/app/migration-readiness', icon: CheckCircle },
          { label: 'History', path: '/app/migration-history', icon: Clock },
          { label: 'Report', path: '/app/migration-report', icon: FileText },
          { label: 'Migration Templates', path: '/app/migration-templates', icon: FileText },
          { label: 'Batch Migration', path: '/app/batch-migration', icon: Layers },
        ],
      },
      {
        label: 'Image & disk',
        items: [
          { label: 'ISO Images', path: '/app/iso-images', icon: Disc },
          { label: 'Upload Disk', path: '/app/upload-disk', icon: Upload },
          { label: 'Download Disk', path: '/app/download-disk', icon: Download },
          { label: 'Pipeline', path: '/app/pipeline', icon: Workflow },
          { label: 'Disk Images', path: '/app/disk-images', icon: HardDrive },
          { label: 'Disk Converter', path: '/app/disk-converter', icon: HardDrive },
        ],
      },
      {
        label: 'Managers',
        items: [
          { label: 'Backup Scheduler', path: '/app/backup-scheduler', icon: Calendar },
          { label: 'Batch Import', path: '/app/batch-import', icon: FileUp },
          { label: 'Snapshot Mgr', path: '/app/snapshot-manager', icon: Camera },
          { label: 'Storage Mgr', path: '/app/storage-manager', icon: Database },
          { label: 'Manifest Builder', path: '/app/manifest-builder', icon: FileText },
          { label: 'Job Monitor', path: '/app/job-monitor', icon: Activity },
        ],
      },
      {
        label: 'Network extras',
        items: [{ label: 'Network Topology', path: '/app/network-topology', icon: Map }],
      },
    ],
  },
]

export const ALL_NAV_ITEMS: NavItem[] = dedupeNavPaths([
  ...NAV_GROUPS.flatMap(flattenNavGroup),
  ...TOP_BAR_QUICK_LINKS,
])

export const PAGE_TITLE_BY_PATH: Record<string, string> = Object.fromEntries(
  ALL_NAV_ITEMS.map((item) => [item.path, item.label]),
)

export const routeLabels: Record<string, string> = {
  ...PAGE_TITLE_BY_PATH,
  '/app/create': 'Create VM',
}
