// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPut, apiPostVoid, apiDelete } from './client'

export interface Schedule {
  id: string
  name: string
  vm_name: string
  action: 'start' | 'stop' | 'restart' | 'snapshot'
  schedule_type: 'once' | 'daily' | 'weekly'
  time: string // HH:MM format
  days_of_week?: number[] // 0-6, Sunday = 0 (for weekly schedules)
  enabled: boolean
  created: string
  last_run?: string
  next_run?: string
}

export interface CreateScheduleRequest {
  name: string
  vm_name: string
  action: 'start' | 'stop' | 'restart' | 'snapshot'
  schedule_type: 'once' | 'daily' | 'weekly'
  time: string
  days_of_week?: number[]
  enabled?: boolean
}

export interface ScheduleHistory {
  schedule_id: string
  schedule_name: string
  vm_name: string
  action: string
  executed_at: string
  status: 'success' | 'failed'
  error?: string
}

const API_BASE = '/api'

export async function listSchedules(): Promise<Schedule[]> {
  return apiGet<Schedule[]>(`${API_BASE}/schedules`)
}

export async function getSchedule(id: string): Promise<Schedule> {
  return apiGet<Schedule>(`${API_BASE}/schedules/${id}`)
}

export async function createSchedule(req: CreateScheduleRequest): Promise<Schedule> {
  return apiPost<Schedule>(`${API_BASE}/schedules`, req)
}

export async function updateSchedule(id: string, req: Partial<CreateScheduleRequest>): Promise<Schedule> {
  return apiPut<Schedule>(`${API_BASE}/schedules/${id}`, req)
}

export async function deleteSchedule(id: string): Promise<void> {
  return apiDelete(`${API_BASE}/schedules/${id}`)
}

export async function enableSchedule(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/schedules/${id}/enable`)
}

export async function disableSchedule(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/schedules/${id}/disable`)
}

export async function getScheduleHistory(scheduleId?: string): Promise<ScheduleHistory[]> {
  const url = scheduleId
    ? `${API_BASE}/schedules/${scheduleId}/history`
    : `${API_BASE}/schedules/history`
  return apiGet<ScheduleHistory[]>(url)
}

export async function runScheduleNow(id: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/schedules/${id}/run`)
}
