// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PageLoading } from '../components/Skeleton'
import { useEffect, useState } from 'react'
import { Bell, Plus, Trash2, Power, PowerOff, Send, Mail, MessageSquare, Webhook } from 'lucide-react'
import {
  listChannels,
  listRules,
  createChannel,
  createRule,
  deleteChannel,
  deleteRule,
  enableRule,
  disableRule,
  testChannel,
  getHistory,
  NotificationChannel,
  NotificationRule,
  NotificationHistory,
} from '../api/notifications'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader, Modal, EmptyState } from '../components/ui'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import RelativeTime from '../components/RelativeTime'

export default function Notifications() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [channels, setChannels] = useState<NotificationChannel[]>([])
  const [rules, setRules] = useState<NotificationRule[]>([])
  const [history, setHistory] = useState<NotificationHistory[]>([])
  const [loading, setLoading] = useState(true)
  const [activeTab, setActiveTab] = useState<'channels' | 'rules' | 'history'>('channels')
  const [showCreateChannel, setShowCreateChannel] = useState(false)
  const [showCreateRule, setShowCreateRule] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    loadData()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const loadData = async () => {
    setLoadError(null)
    const [chRes, rulesRes, histRes] = await Promise.allSettled([
      listChannels(),
      listRules(),
      getHistory(50),
    ])
    if (chRes.status === 'fulfilled') setChannels(chRes.value)
    else {
      setChannels([])
      const msg = formatUserError(chRes.reason)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load notifications', chRes.reason)
    }
    if (rulesRes.status === 'fulfilled') setRules(rulesRes.value)
    else setRules([])
    if (histRes.status === 'fulfilled') setHistory(histRes.value)
    else setHistory([])
    setLoading(false)
  }

  const handleDeleteChannel = async (id: string) => {
    const ok = await confirm('Delete Channel', 'Delete this notification channel?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return

    try {
      await deleteChannel(id)
      toast.success('Channel deleted')
      loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to delete channel', error)
    }
  }

  const handleTestChannel = async (id: string, name: string) => {
    try {
      await testChannel(id)
      toast.success(`Test notification sent to ${name}`)
    } catch (error) {
      toastFailure(toast, 'Failed to send test notification', error)
    }
  }

  const handleDeleteRule = async (id: string) => {
    const ok = await confirm('Delete Rule', 'Delete this notification rule?', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return

    try {
      await deleteRule(id)
      toast.success('Rule deleted')
      loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to delete rule', error)
    }
  }

  const handleToggleRule = async (rule: NotificationRule) => {
    try {
      if (rule.enabled) {
        await disableRule(rule.id)
        toast.success('Rule disabled')
      } else {
        await enableRule(rule.id)
        toast.success('Rule enabled')
      }
      loadData()
    } catch (error) {
      toastFailure(toast, `Failed to ${rule.enabled ? 'disable' : 'enable'} rule`, error)
    }
  }

  const getChannelIcon = (type: string) => {
    switch (type) {
      case 'email': return <Mail className="w-5 h-5 text-[var(--zf-ink)]" />
      case 'slack': return <MessageSquare className="w-5 h-5 text-[var(--zf-ink)]" />
      case 'webhook': return <Webhook className="w-5 h-5 text-[var(--zf-ink)]" />
      case 'teams': return <MessageSquare className="w-5 h-5 text-[var(--zf-ink)]" />
      default: return <Bell className="w-5 h-5 text-[var(--zf-muted)]" />
    }
  }

  const getSeverityColor = (severity: string): string => {
    switch (severity) {
      case 'critical': return 'text-red-700 bg-red-50 border-red-200'
      case 'warning': return 'text-amber-800 bg-amber-50 border-amber-200'
      case 'info': return 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
      default: return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
    }
  }

  if (loading) {
    return <PageLoading />
  }

  return (
    <div>
      {loadError && (
        <ErrorBanner
          title="Could not load notifications"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadData}
        />
      )}
      <PageHeader
        title="Notifications"
        description="Manage notification channels and alert rules"
      />

      {/* Tabs */}
      <div className="flex gap-4 mb-6 border-b border-[var(--zf-hairline)]">
        <button
          onClick={() => setActiveTab('channels')}
          className={`px-4 py-2 font-medium transition ${
            activeTab === 'channels'
              ? 'text-[var(--zf-link)] border-b-2 border-[var(--zf-link)]'
              : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
          }`}
        >
          Channels ({channels.length})
        </button>
        <button
          onClick={() => setActiveTab('rules')}
          className={`px-4 py-2 font-medium transition ${
            activeTab === 'rules'
              ? 'text-[var(--zf-link)] border-b-2 border-[var(--zf-link)]'
              : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
          }`}
        >
          Rules ({rules.length})
        </button>
        <button
          onClick={() => setActiveTab('history')}
          className={`px-4 py-2 font-medium transition ${
            activeTab === 'history'
              ? 'text-[var(--zf-link)] border-b-2 border-[var(--zf-link)]'
              : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
          }`}
        >
          History ({history.length})
        </button>
      </div>

      {/* Channels Tab */}
      {activeTab === 'channels' && (
        <div>
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-bold text-[var(--zf-ink)]">Notification Channels</h2>
            <button
              onClick={() => setShowCreateChannel(true)}
              className="zf-btn zf-btn-primary"
            >
              <Plus className="w-4 h-4" />
              Add Channel
            </button>
          </div>

          {channels.length === 0 ? (
            <div className="zf-panel">
              <EmptyState
                icon={<Bell className="w-6 h-6" />}
                title="No notification channels"
                description="Add a channel to receive notifications"
                action={
                  <button
                    onClick={() => setShowCreateChannel(true)}
                    className="zf-btn zf-btn-primary"
                  >
                    <Plus className="w-4 h-4" />
                    Add First Channel
                  </button>
                }
              />
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {channels.map((channel) => (
                <div
                  key={channel.id}
                  className="zf-panel p-4"
                >
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex items-center gap-3">
                      {getChannelIcon(channel.type)}
                      <div>
                        <h3 className="font-bold text-[var(--zf-ink)]">{channel.name}</h3>
                        <p className="text-xs text-[var(--zf-muted)] capitalize">{channel.type}</p>
                      </div>
                    </div>
                    <span
                      className={`px-2 py-1 rounded text-xs font-medium border ${
                        channel.enabled ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
                      }`}
                    >
                      {channel.enabled ? 'Enabled' : 'Disabled'}
                    </span>
                  </div>

                  <div className="flex gap-2 mt-4">
                    <button
                      onClick={() => handleTestChannel(channel.id, channel.name)}
                      disabled={!channel.enabled}
                      className="flex-1 zf-btn zf-btn-ghost zf-btn-sm"
                    >
                      <Send className="w-3.5 h-3.5" />
                      Test
                    </button>
                    <button
                      onClick={() => handleDeleteChannel(channel.id)}
                      className="zf-btn zf-btn-danger zf-btn-sm"
                      title="Delete"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Rules Tab */}
      {activeTab === 'rules' && (
        <div>
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-bold text-[var(--zf-ink)]">Notification Rules</h2>
            <button
              onClick={() => setShowCreateRule(true)}
              className="zf-btn zf-btn-primary"
            >
              <Plus className="w-4 h-4" />
              Create Rule
            </button>
          </div>

          {rules.length === 0 ? (
            <div className="zf-panel">
              <EmptyState
                icon={<Bell className="w-6 h-6" />}
                title="No notification rules"
                description="Create rules to trigger notifications"
                action={
                  <button
                    onClick={() => setShowCreateRule(true)}
                    className="zf-btn zf-btn-primary"
                  >
                    <Plus className="w-4 h-4" />
                    Create First Rule
                  </button>
                }
              />
            </div>
          ) : (
            <div className="space-y-4">
              {rules.map((rule) => (
                <div
                  key={rule.id}
                  className="zf-panel p-4"
                >
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex-1">
                      <div className="flex items-center gap-3 mb-2">
                        <h3 className="font-bold text-[var(--zf-ink)]">{rule.name}</h3>
                        <span
                          className={`px-2 py-1 rounded text-xs font-medium border ${
                            rule.enabled ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
                          }`}
                        >
                          {rule.enabled ? 'Enabled' : 'Disabled'}
                        </span>
                      </div>
                      {rule.description && (
                        <p className="text-sm text-[var(--zf-muted)] mb-2">{rule.description}</p>
                      )}
                      <div className="flex flex-wrap gap-2 mb-2">
                        {rule.event_types.slice(0, 3).map((event) => (
                          <span
                            key={event}
                            className="px-2 py-1 rounded text-xs border text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]"
                          >
                            {event}
                          </span>
                        ))}
                        {rule.event_types.length > 3 && (
                          <span className="px-2 py-1 rounded text-xs border text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]">
                            +{rule.event_types.length - 3} more
                          </span>
                        )}
                      </div>
                      <div className="flex gap-2 text-xs text-[var(--zf-muted)]">
                        <span>Triggered: {rule.triggered_count} times</span>
                        {rule.last_triggered && (
                          <span>• Last: <RelativeTime date={rule.last_triggered} /></span>
                        )}
                      </div>
                    </div>

                    <div className="flex gap-2">
                      <button
                        onClick={() => handleToggleRule(rule)}
                        className={`p-2 rounded border transition ${
                          rule.enabled
                            ? 'text-amber-800 bg-amber-50 border-amber-200 hover:bg-amber-100'
                            : 'text-emerald-700 bg-emerald-50 border-emerald-200 hover:bg-emerald-100'
                        }`}
                        title={rule.enabled ? 'Disable' : 'Enable'}
                      >
                        {rule.enabled ? (
                          <PowerOff className="w-4 h-4" />
                        ) : (
                          <Power className="w-4 h-4" />
                        )}
                      </button>
                      <button
                        onClick={() => handleDeleteRule(rule.id)}
                        className="zf-btn zf-btn-danger zf-btn-sm"
                        title="Delete"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* History Tab */}
      {activeTab === 'history' && (
        <div>
          <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Notification History</h2>
          {history.length === 0 ? (
            <div className="text-center py-12 zf-panel">
              <p className="text-[var(--zf-muted)]">No notification history yet</p>
            </div>
          ) : (
            <div className="zf-panel overflow-hidden">
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead className="bg-[var(--zf-canvas)]">
                    <tr>
                      <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                        Timestamp
                      </th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                        Rule
                      </th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                        Event
                      </th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                        VM
                      </th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                        Channel
                      </th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                        Status
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-[var(--zf-hairline)]">
                    {history.map((item) => (
                      <tr key={item.id} className="hover:bg-black/[0.03]">
                        <td className="px-4 py-3 text-sm text-[var(--zf-muted)]">
                          <RelativeTime date={item.sent_at} />
                        </td>
                        <td className="px-4 py-3 text-sm text-[var(--zf-ink)]">{item.rule_name}</td>
                        <td className="px-4 py-3">
                          <span className={`px-2 py-1 rounded text-xs font-medium border ${getSeverityColor(item.severity)}`}>
                            {item.event_type}
                          </span>
                        </td>
                        <td className="px-4 py-3 text-sm text-[var(--zf-ink)]">{item.vm_name || '-'}</td>
                        <td className="px-4 py-3 text-sm text-[var(--zf-muted)]">{item.channel}</td>
                        <td className="px-4 py-3">
                          <span
                            className={`px-2 py-1 rounded text-xs font-medium border ${
                              item.status === 'sent' ? 'text-emerald-700 bg-emerald-50 border-emerald-200' : 'text-red-700 bg-red-50 border-red-200'
                            }`}
                          >
                            {item.status.toUpperCase()}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}

      {showCreateChannel && (
        <CreateChannelModal
          onClose={() => setShowCreateChannel(false)}
          onCreated={() => { setShowCreateChannel(false); loadData() }}
        />
      )}

      {showCreateRule && (
        <CreateRuleModal
          channels={channels}
          onClose={() => setShowCreateRule(false)}
          onCreated={() => { setShowCreateRule(false); loadData() }}
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

type ChannelType = 'email' | 'slack' | 'webhook' | 'teams'

function CreateChannelModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [type, setType] = useState<ChannelType>('webhook')
  const [saving, setSaving] = useState(false)
  // One field set per type -- only the active type's fields are sent as `config`.
  const [webhookUrl, setWebhookUrl] = useState('')
  const [slackWebhookUrl, setSlackWebhookUrl] = useState('')
  const [teamsWebhookUrl, setTeamsWebhookUrl] = useState('')
  const [smtpHost, setSmtpHost] = useState('')
  const [smtpPort, setSmtpPort] = useState('587')
  const [fromAddress, setFromAddress] = useState('')
  const [toAddress, setToAddress] = useState('')

  const configFor = (t: ChannelType): Record<string, unknown> => {
    switch (t) {
      case 'webhook':
        return { url: webhookUrl }
      case 'slack':
        return { webhook_url: slackWebhookUrl }
      case 'teams':
        return { webhook_url: teamsWebhookUrl }
      case 'email':
        return { smtp_host: smtpHost, smtp_port: Number(smtpPort), from: fromAddress, to: toAddress }
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSaving(true)
    try {
      await createChannel({ name, type, config: configFor(type) })
      toast.success('Notification channel created')
      onCreated()
    } catch (err) {
      toastFailure(toast, 'Failed to create channel', err)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-md">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Add Notification Channel</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input aria-label="Name" type="text" value={name} onChange={(e) => setName(e.target.value)} required
            className="input-field" placeholder="e.g. ops-slack" />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Type</label>
          <select aria-label="Type" value={type} onChange={(e) => setType(e.target.value as ChannelType)}
            className="input-field">
            <option value="webhook">Webhook</option>
            <option value="slack">Slack</option>
            <option value="teams">Microsoft Teams</option>
            <option value="email">Email</option>
          </select>
        </div>

        {type === 'webhook' && (
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Webhook URL</label>
            <input aria-label="Webhook URL" type="url" value={webhookUrl} onChange={(e) => setWebhookUrl(e.target.value)} required
              className="input-field" placeholder="https://..." />
          </div>
        )}

        {type === 'slack' && (
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Slack Webhook URL</label>
            <input aria-label="Slack Webhook URL" type="url" value={slackWebhookUrl} onChange={(e) => setSlackWebhookUrl(e.target.value)} required
              className="input-field" placeholder="https://hooks.slack.com/services/..." />
          </div>
        )}

        {type === 'teams' && (
          <div>
            <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Teams Webhook URL</label>
            <input aria-label="Teams Webhook URL" type="url" value={teamsWebhookUrl} onChange={(e) => setTeamsWebhookUrl(e.target.value)} required
              className="input-field" placeholder="https://xxx.webhook.office.com/..." />
          </div>
        )}

        {type === 'email' && (
          <div className="space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">SMTP Host</label>
                <input aria-label="SMTP Host" type="text" value={smtpHost} onChange={(e) => setSmtpHost(e.target.value)} required
                  className="input-field" placeholder="smtp.example.com" />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">SMTP Port</label>
                <input aria-label="SMTP Port" type="number" value={smtpPort} onChange={(e) => setSmtpPort(e.target.value)} required
                  className="input-field" />
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">From Address</label>
              <input aria-label="From Address" type="email" value={fromAddress} onChange={(e) => setFromAddress(e.target.value)} required
                className="input-field" placeholder="alerts@example.com" />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">To Address</label>
              <input aria-label="To Address" type="email" value={toAddress} onChange={(e) => setToAddress(e.target.value)} required
                className="input-field" placeholder="oncall@example.com" />
            </div>
          </div>
        )}

        <div className="flex justify-end gap-3 pt-2">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost">
            Cancel
          </button>
          <button type="submit" disabled={saving}
            className="zf-btn zf-btn-primary">
            {saving ? 'Creating…' : 'Create Channel'}
          </button>
        </div>
      </form>
    </Modal>
  )
}

const RULE_EVENT_TYPES = [
  'created', 'started', 'stopped', 'paused', 'resumed', 'deleted', 'cloned', 'migrated',
  'snapshot_created', 'snapshot_reverted', 'cpu_hotplug', 'memory_hotplug',
  'disk_attached', 'disk_detached', 'error', 'auto_healed',
]
const RULE_SEVERITIES: Array<'info' | 'warning' | 'critical'> = ['info', 'warning', 'critical']

function CreateRuleModal({ channels, onClose, onCreated }: { channels: NotificationChannel[]; onClose: () => void; onCreated: () => void }) {
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [eventTypes, setEventTypes] = useState<string[]>(['error'])
  const [severityLevels, setSeverityLevels] = useState<Array<'info' | 'warning' | 'critical'>>(['warning', 'critical'])
  const [selectedChannels, setSelectedChannels] = useState<string[]>([])
  const [saving, setSaving] = useState(false)

  const toggle = <T,>(list: T[], value: T, setList: (v: T[]) => void) => {
    setList(list.includes(value) ? list.filter((v) => v !== value) : [...list, value])
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (eventTypes.length === 0) { toast.error('Select at least one event type'); return }
    if (severityLevels.length === 0) { toast.error('Select at least one severity'); return }
    if (selectedChannels.length === 0) { toast.error('Select at least one channel'); return }
    setSaving(true)
    try {
      await createRule({
        name,
        description: description || undefined,
        event_types: eventTypes,
        severity_levels: severityLevels,
        channels: selectedChannels,
      })
      toast.success('Notification rule created')
      onCreated()
    } catch (err) {
      toastFailure(toast, 'Failed to create rule', err)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-lg max-h-[90vh] overflow-y-auto">
      <h2 className="text-xl font-bold mb-4 text-[var(--zf-ink)]">Create Notification Rule</h2>
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Name</label>
          <input aria-label="Name" type="text" value={name} onChange={(e) => setName(e.target.value)} required
            className="input-field" placeholder="e.g. vm-failures-to-oncall" />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Description (optional)</label>
          <input aria-label="Description (optional)" type="text" value={description} onChange={(e) => setDescription(e.target.value)}
            className="input-field" />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Event types</label>
          <div className="flex flex-wrap gap-2">
            {RULE_EVENT_TYPES.map((et) => (
              <button type="button" key={et} onClick={() => toggle(eventTypes, et, setEventTypes)}
                className={`px-2.5 py-1 rounded text-xs font-mono transition border ${eventTypes.includes(et) ? 'bg-[var(--zf-link-fill)] text-white border-[var(--zf-link)]' : 'bg-[var(--zf-surface)] text-[var(--zf-muted)] border-[var(--zf-hairline)]'}`}>
                {et}
              </button>
            ))}
          </div>
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Severity</label>
          <div className="flex gap-2">
            {RULE_SEVERITIES.map((sev) => (
              <button type="button" key={sev} onClick={() => toggle(severityLevels, sev, setSeverityLevels)}
                className={`px-3 py-1.5 rounded text-sm capitalize transition border ${severityLevels.includes(sev) ? 'bg-[var(--zf-link-fill)] text-white border-[var(--zf-link)]' : 'bg-[var(--zf-surface)] text-[var(--zf-muted)] border-[var(--zf-hairline)]'}`}>
                {sev}
              </button>
            ))}
          </div>
        </div>
        <div>
          <label className="block text-sm font-medium mb-1 text-[var(--zf-ink)]">Channels</label>
          {channels.length === 0 ? (
            <p className="text-sm text-[var(--zf-muted)]">No channels yet — add one first.</p>
          ) : (
            <div className="flex flex-wrap gap-2">
              {channels.map((ch) => (
                <button type="button" key={ch.id} onClick={() => toggle(selectedChannels, ch.id, setSelectedChannels)}
                  className={`px-3 py-1.5 rounded text-sm transition border ${selectedChannels.includes(ch.id) ? 'bg-[var(--zf-link-fill)] text-white border-[var(--zf-link)]' : 'bg-[var(--zf-surface)] text-[var(--zf-muted)] border-[var(--zf-hairline)]'}`}>
                  {ch.name}
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-3 pt-2">
          <button type="button" onClick={onClose} className="zf-btn zf-btn-ghost">
            Cancel
          </button>
          <button type="submit" disabled={saving || channels.length === 0}
            className="zf-btn zf-btn-primary">
            {saving ? 'Creating…' : 'Create Rule'}
          </button>
        </div>
      </form>
    </Modal>
  )
}
