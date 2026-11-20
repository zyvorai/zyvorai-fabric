// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

export interface NotificationChannel {
  id: string
  name: string
  type: 'email' | 'slack' | 'webhook' | 'teams'
  config: Record<string, any>
  enabled: boolean
  created: string
  last_test?: string
}

export interface NotificationRule {
  id: string
  name: string
  description?: string
  event_types: string[]
  severity_levels: ('info' | 'warning' | 'critical')[]
  channels: string[]
  vm_tags?: string[]
  enabled: boolean
  created: string
  triggered_count: number
  last_triggered?: string
}

export interface NotificationHistory {
  id: string
  rule_id: string
  rule_name: string
  event_type: string
  severity: 'info' | 'warning' | 'critical'
  channel: string
  vm_name?: string
  message: string
  sent_at: string
  status: 'sent' | 'failed'
  error?: string
}

export interface CreateChannelRequest {
  name: string
  type: 'email' | 'slack' | 'webhook' | 'teams'
  config: Record<string, any>
  enabled?: boolean
}

export interface CreateRuleRequest {
  name: string
  description?: string
  event_types: string[]
  severity_levels: ('info' | 'warning' | 'critical')[]
  channels: string[]
  vm_tags?: string[]
  enabled?: boolean
}

const API_BASE = '/api'

// Channels
export async function listChannels(): Promise<NotificationChannel[]> {
  return apiGet<NotificationChannel[]>(`${API_BASE}/notifications/channels`)
}

export async function createChannel(req: CreateChannelRequest): Promise<NotificationChannel> {
  return apiPost<NotificationChannel>(`${API_BASE}/notifications/channels`, req)
}

export async function updateChannel(id: string, req: Partial<CreateChannelRequest>): Promise<NotificationChannel> {
  return apiPut<NotificationChannel>(`${API_BASE}/notifications/channels/${id}`, req)
}

export async function deleteChannel(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/notifications/channels/${id}`)
}

export async function testChannel(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/notifications/channels/${id}/test`)
}

// Rules
export async function listRules(): Promise<NotificationRule[]> {
  return apiGet<NotificationRule[]>(`${API_BASE}/notifications/rules`)
}

export async function createRule(req: CreateRuleRequest): Promise<NotificationRule> {
  return apiPost<NotificationRule>(`${API_BASE}/notifications/rules`, req)
}

export async function updateRule(id: string, req: Partial<CreateRuleRequest>): Promise<NotificationRule> {
  return apiPut<NotificationRule>(`${API_BASE}/notifications/rules/${id}`, req)
}

export async function deleteRule(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/notifications/rules/${id}`)
}

export async function enableRule(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/notifications/rules/${id}/enable`)
}

export async function disableRule(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/notifications/rules/${id}/disable`)
}

// History
export async function getHistory(limit: number = 50): Promise<NotificationHistory[]> {
  return apiGet<NotificationHistory[]>(`${API_BASE}/notifications/history?limit=${limit}`)
}

// Event types for reference
export const EVENT_TYPES = [
  'vm.created',
  'vm.deleted',
  'vm.started',
  'vm.stopped',
  'vm.failed',
  'quota.exceeded',
  'backup.completed',
  'backup.failed',
  'schedule.failed',
  'resource.high_usage',
]
