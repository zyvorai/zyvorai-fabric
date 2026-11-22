// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useAuth } from '../contexts/AuthContext'

export function usePermissions() {
  const { user } = useAuth()
  const role = (user?.role ?? 'viewer').toLowerCase()
  const canWrite = role === 'admin' || role === 'operator' || role === 'user'
  const canAdmin = role === 'admin'
  const isViewer = role === 'viewer'
  return { role, canWrite, canAdmin, isViewer }
}
