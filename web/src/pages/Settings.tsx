// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react'
import { Save, RotateCcw, Bell, Globe, Shield, Database, Loader2 } from 'lucide-react'
import { useToastContext } from '../contexts/ToastContext'
import { apiGet, apiPut } from '../api/client'
import { listStoragePools } from '../api/storage'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'

interface AppSettings {
  daemon_name: string
  log_level: string
  auto_refresh: boolean
  refresh_interval: number
  default_bridge: string
  enable_ipv6: boolean
  dns_servers: string
  default_pool: string
  default_format: string
  enable_compression: boolean
  snapshot_retention: number
  enable_auth: boolean
  enable_tls: boolean
  session_timeout: number
  audit_logging: boolean
  email_notifications: boolean
  webhook_url: string
  notify_on_start: boolean
  notify_on_stop: boolean
  notify_on_error: boolean
}

const defaultSettings: AppSettings = {
  daemon_name: 'zyvor-fabricd-01',
  log_level: 'info',
  auto_refresh: true,
  refresh_interval: 5,
  default_bridge: 'br0',
  enable_ipv6: false,
  dns_servers: '8.8.8.8, 8.8.4.4',
  default_pool: 'default',
  default_format: 'qcow2',
  enable_compression: true,
  snapshot_retention: 30,
  enable_auth: true,
  enable_tls: false,
  session_timeout: 3600,
  audit_logging: true,
  email_notifications: false,
  webhook_url: '',
  notify_on_start: true,
  notify_on_stop: true,
  notify_on_error: true,
}

export default function Settings() {
  const toast = useToastContext()
  const [settings, setSettings] = useState<AppSettings>(defaultSettings)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [poolNames, setPoolNames] = useState<string[]>([])
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    loadSettings()
    loadPools()
  }, [])

  const loadPools = async () => {
    try {
      const pools = await listStoragePools()
      setPoolNames(pools.map(p => p.name))
    } catch {
      // Fallback - keep empty, user can type
    }
  }

  const loadSettings = async () => {
    setLoadError(null)
    try {
      const data = await apiGet<AppSettings>('/api/settings')
      setSettings(data)
    } catch (error) {
      const msg = formatUserError(error)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load settings', error)
    } finally {
      setLoading(false)
    }
  }

  const handleSave = async () => {
    setSaving(true)
    try {
      await apiPut<AppSettings>('/api/settings', settings)
      toast.success('Settings saved successfully')
    } catch (_error) {
      toast.error('Failed to save settings')
    } finally {
      setSaving(false)
    }
  }

  const handleReset = () => {
    setSettings(defaultSettings)
    toast.info('Settings reset to defaults')
  }

  const update = (field: keyof AppSettings, value: any) => {
    setSettings((prev) => ({ ...prev, [field]: value }))
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="w-8 h-8 animate-spin text-[var(--zf-ink)]" />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {loadError && (
        <ErrorBanner
          title="Could not load settings"
          headline={loadError}
          onRetry={loadSettings}
        />
      )}
      <PageHeader
        title="Settings"
        actions={
          <>
            <button
              onClick={handleReset}
              className="zf-btn zf-btn-ghost"
            >
              <RotateCcw className="w-4 h-4" />
              Reset
            </button>
            <button
              onClick={handleSave}
              disabled={saving}
              className="zf-btn zf-btn-primary"
            >
              {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
              Save Changes
            </button>
          </>
        }
      />

      {/* General Settings */}
      <div className="zf-panel">
        <div className="p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-semibold flex items-center gap-2">
            <Globe className="w-5 h-5 text-[var(--zf-link)]" />
            General
          </h2>
        </div>
        <div className="p-6 space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Daemon Name</label>
              <input
                type="text"
                value={settings.daemon_name}
                onChange={(e) => update('daemon_name', e.target.value)}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Log Level</label>
              <select
                value={settings.log_level}
                onChange={(e) => update('log_level', e.target.value)}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
              >
                <option value="debug">Debug</option>
                <option value="info">Info</option>
                <option value="warn">Warning</option>
                <option value="error">Error</option>
              </select>
            </div>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Refresh Interval (seconds)</label>
              <input
                type="number"
                value={settings.refresh_interval}
                onChange={(e) => update('refresh_interval', parseInt(e.target.value) || 5)}
                disabled={!settings.auto_refresh}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)] disabled:opacity-50"
              />
            </div>
          </div>
          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="autoRefresh"
              checked={settings.auto_refresh}
              onChange={(e) => update('auto_refresh', e.target.checked)}
              className="w-4 h-4 text-[var(--zf-ink)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-[var(--zf-ink)]"
            />
            <label htmlFor="autoRefresh" className="text-sm text-[var(--zf-ink)]">Enable auto-refresh</label>
          </div>
        </div>
      </div>

      {/* Network Settings */}
      <div className="zf-panel">
        <div className="p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-semibold flex items-center gap-2">
            <Globe className="w-5 h-5 text-[var(--zf-muted)]" />
            Network
          </h2>
        </div>
        <div className="p-6 space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Default Bridge</label>
              <input
                type="text"
                value={settings.default_bridge}
                onChange={(e) => update('default_bridge', e.target.value)}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">DNS Servers</label>
              <input
                type="text"
                value={settings.dns_servers}
                onChange={(e) => update('dns_servers', e.target.value)}
                placeholder="Comma-separated"
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
              />
            </div>
          </div>
          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="enableIPv6"
              checked={settings.enable_ipv6}
              onChange={(e) => update('enable_ipv6', e.target.checked)}
              className="w-4 h-4 text-[var(--zf-ink)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-[var(--zf-ink)]"
            />
            <label htmlFor="enableIPv6" className="text-sm text-[var(--zf-ink)]">Enable IPv6 networking</label>
          </div>
        </div>
      </div>

      {/* Storage Settings */}
      <div className="zf-panel">
        <div className="p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-semibold flex items-center gap-2">
            <Database className="w-5 h-5 text-[var(--zf-muted)]" />
            Storage
          </h2>
        </div>
        <div className="p-6 space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Default Storage Pool</label>
              <select
                value={settings.default_pool}
                onChange={(e) => update('default_pool', e.target.value)}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
              >
                {poolNames.length > 0 ? (
                  poolNames.map(name => (
                    <option key={name} value={name}>{name}</option>
                  ))
                ) : (
                  <option value={settings.default_pool}>{settings.default_pool}</option>
                )}
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Default Disk Format</label>
              <select
                value={settings.default_format}
                onChange={(e) => update('default_format', e.target.value)}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
              >
                <option value="qcow2">QCOW2</option>
                <option value="raw">RAW</option>
                <option value="vmdk">VMDK</option>
              </select>
            </div>
          </div>
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Snapshot Retention (days)</label>
            <input
              type="number"
              value={settings.snapshot_retention}
              onChange={(e) => update('snapshot_retention', parseInt(e.target.value) || 30)}
              className="w-full md:w-1/2 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
            />
          </div>
          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="enableCompression"
              checked={settings.enable_compression}
              onChange={(e) => update('enable_compression', e.target.checked)}
              className="w-4 h-4 text-[var(--zf-ink)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-[var(--zf-ink)]"
            />
            <label htmlFor="enableCompression" className="text-sm text-[var(--zf-ink)]">Enable disk compression for QCOW2</label>
          </div>
        </div>
      </div>

      {/* Security Settings */}
      <div className="zf-panel">
        <div className="p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-semibold flex items-center gap-2">
            <Shield className="w-5 h-5 text-[var(--zf-muted)]" />
            Security
          </h2>
        </div>
        <div className="p-6 space-y-4">
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Session Timeout (seconds)</label>
            <input
              type="number"
              value={settings.session_timeout}
              onChange={(e) => update('session_timeout', parseInt(e.target.value) || 3600)}
              className="w-full md:w-1/2 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
            />
          </div>
          <div className="space-y-2">
            {[
              { id: 'enableAuth', label: 'Enable authentication (JWT)', field: 'enable_auth' as const },
              { id: 'enableTLS', label: 'Enable TLS/HTTPS', field: 'enable_tls' as const },
              { id: 'auditLogging', label: 'Enable audit logging', field: 'audit_logging' as const },
            ].map(({ id, label, field }) => (
              <div key={id} className="flex items-center gap-2">
                <input
                  type="checkbox"
                  id={id}
                  checked={settings[field] as boolean}
                  onChange={(e) => update(field, e.target.checked)}
                  className="w-4 h-4 text-[var(--zf-ink)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-[var(--zf-ink)]"
                />
                <label htmlFor={id} className="text-sm text-[var(--zf-ink)]">{label}</label>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Notification Settings */}
      <div className="zf-panel">
        <div className="p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-semibold flex items-center gap-2">
            <Bell className="w-5 h-5 text-[var(--zf-muted)]" />
            Notifications
          </h2>
        </div>
        <div className="p-6 space-y-4">
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Webhook URL (optional)</label>
            <input
              type="text"
              value={settings.webhook_url}
              onChange={(e) => update('webhook_url', e.target.value)}
              placeholder="https://hooks.slack.com/..."
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-ink)]"
            />
          </div>
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="emailNotifications"
                checked={settings.email_notifications}
                onChange={(e) => update('email_notifications', e.target.checked)}
                className="w-4 h-4 text-[var(--zf-ink)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-[var(--zf-ink)]"
              />
              <label htmlFor="emailNotifications" className="text-sm text-[var(--zf-ink)]">Enable email notifications</label>
            </div>
            <div className="ml-6 space-y-2 text-sm text-[var(--zf-muted)]">
              {[
                { id: 'notifyOnStart', label: 'VM started', field: 'notify_on_start' as const },
                { id: 'notifyOnStop', label: 'VM stopped', field: 'notify_on_stop' as const },
                { id: 'notifyOnError', label: 'VM errors', field: 'notify_on_error' as const },
              ].map(({ id, label, field }) => (
                <div key={id} className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id={id}
                    checked={settings[field] as boolean}
                    onChange={(e) => update(field, e.target.checked)}
                    className="w-4 h-4 text-[var(--zf-ink)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] rounded focus:ring-[var(--zf-ink)]"
                  />
                  <label htmlFor={id}>{label}</label>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
