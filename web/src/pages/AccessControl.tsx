// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PanelLoading } from '../components/Skeleton'
import { useState, useEffect, useCallback } from 'react'
import { Users, Plus, Trash2, Loader2, CheckCircle } from 'lucide-react'
import { apiFetch } from '../api/client'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui'
import { formatHttpErrorBody, formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'

interface UserAccount { id: string; username: string; role: string; enabled: boolean; created_at: string; last_login?: string }

function roleBadge(role: string): string {
  switch (role?.toLowerCase()) {
    case 'admin': return 'text-red-700 bg-red-50 border-red-200'
    case 'operator': return 'text-amber-800 bg-amber-50 border-amber-200'
    case 'viewer': return 'text-emerald-700 bg-emerald-50 border-emerald-200'
    default: return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  }
}

const USERNAME_REGEX = /^[a-zA-Z0-9_-]{3,32}$/
const MIN_PASSWORD_LENGTH = 8

export default function AccessControl() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [users, setUsers] = useState<UserAccount[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [showAdd, setShowAdd] = useState(false)
  const [newUsername, setNewUsername] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [newRole, setNewRole] = useState('viewer')
  const [adding, setAdding] = useState(false)
  const [addError, setAddError] = useState('')
  const [addSuccess, setAddSuccess] = useState('')

  const fetchUsers = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const res = await apiFetch('/api/users')
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      const data = await res.json()
      setUsers(Array.isArray(data) ? data : data.users || [])
    } catch (err) {
      const msg = formatUserError(err)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load users', err)
    } finally {
      setLoading(false)
    }
  }, [toast])

  useEffect(() => { fetchUsers() }, [fetchUsers])

  const handleAdd = async () => {
    if (!newUsername.trim()) { setAddError('Username is required'); return }
    if (!USERNAME_REGEX.test(newUsername.trim())) { setAddError('Username must be 3-32 characters: letters, numbers, hyphens, underscores'); return }
    if (!newPassword.trim()) { setAddError('Password is required'); return }
    if (newPassword.length < MIN_PASSWORD_LENGTH) { setAddError(`Password must be at least ${MIN_PASSWORD_LENGTH} characters`); return }
    setAdding(true); setAddError(''); setAddSuccess('')
    try {
      const res = await apiFetch('/api/users', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ username: newUsername.trim(), password: newPassword, role: newRole }) })
      if (!res.ok) { const body = await res.json().catch(() => null); throw new Error(body?.error || `HTTP ${res.status}`) }
      setAddSuccess(`User "${newUsername}" created`); setNewUsername(''); setNewPassword(''); setShowAdd(false); fetchUsers()
      setTimeout(() => setAddSuccess(''), 3000)
    } catch (err) {
      setAddError(formatUserError(err))
      toastFailure(toast, 'Failed to create user', err)
    } finally { setAdding(false) }
  }

  const handleDelete = async (id: string, username: string) => {
    if (!await confirm('Delete User', `Delete user "${username}"?`, { variant: 'danger', confirmLabel: 'Delete' })) return
    try {
      const res = await apiFetch(`/api/users/${id}`, { method: 'DELETE' })
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      setUsers(prev => prev.filter(u => u.id !== id))
      toast.success(`User "${username}" deleted`)
    } catch (err) {
      toastFailure(toast, 'Failed to delete user', err)
    }
  }

  const handleToggle = async (id: string, enabled: boolean) => {
    try {
      const res = await apiFetch(`/api/users/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled: !enabled }) })
      if (!res.ok) {
        const body = await res.text()
        throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
      }
      setUsers(prev => prev.map(u => u.id === id ? { ...u, enabled: !enabled } : u))
    } catch (err) {
      toastFailure(toast, 'Failed to update user', err)
    }
  }

  const adminCount = users.filter(u => u.role === 'admin').length
  const operatorCount = users.filter(u => u.role === 'operator').length
  const viewerCount = users.filter(u => u.role === 'viewer').length

  return (
    <div className="space-y-6">
      <PageHeader
        title="Access Control"
        description="User accounts and role management"
        onRefresh={fetchUsers}
        refreshing={loading}
        primaryAction={
          <button
            type="button"
            onClick={() => setShowAdd(!showAdd)}
            className="zf-btn zf-btn-primary zf-btn-sm"
          >
            <Plus className="w-4 h-4" /> Add User
          </button>
        }
      />

      {loadError && (
        <ErrorBanner
          title="Could not load users"
          headline={loadError}
          hints={hintsForError(loadError, 'auth')}
          onRetry={fetchUsers}
        />
      )}

      {loading && !loadError ? (
        <PanelLoading label="Loading users" />
      ) : !loadError ? (
        <>

      {addSuccess && <div className="bg-emerald-50 border border-emerald-200 rounded-xl px-4 py-3 text-sm text-emerald-700 flex items-center gap-2"><CheckCircle className="w-4 h-4" />{addSuccess}</div>}

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="stat-card-blue rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{users.length}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Total Users</div>
        </div>
        <div className="stat-card-red rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{adminCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Admins</div>
        </div>
        <div className="stat-card-cyan rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-cyan transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{operatorCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Operators</div>
        </div>
        <div className="stat-card-green rounded-xl border border-[var(--zf-hairline)] px-4 py-3 card-glow-green transition-all hover:scale-[1.02]">
          <div className="text-2xl font-bold text-[var(--zf-ink)]">{viewerCount}</div>
          <div className="text-xs text-[var(--zf-muted)] mt-1">Viewers</div>
        </div>
      </div>

      {showAdd && (
        <div className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-xl p-5 space-y-4">
          <h3 className="text-sm font-semibold text-[var(--zf-ink)]">New User</h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div><label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Username</label><input type="text" value={newUsername} onChange={(e) => setNewUsername(e.target.value)} placeholder="username" className="input-field text-sm" /></div>
            <div><label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Password</label><input type="password" value={newPassword} onChange={(e) => setNewPassword(e.target.value)} placeholder="password" className="input-field text-sm" /></div>
            <div><label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Role</label>
              <div className="flex gap-2">
                {['admin', 'operator', 'viewer'].map(r => (
                  <button key={r} onClick={() => setNewRole(r)} className={`px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors capitalize ${newRole === r ? 'bg-[var(--zf-link-fill)] text-white border-[var(--zf-link)]' : 'text-[var(--zf-muted)] bg-[var(--zf-surface)] border-[var(--zf-hairline)] hover:border-[var(--zf-ink)]'}`}>{r}</button>
                ))}
              </div>
            </div>
          </div>
          {addError && <p className="text-sm text-red-600">{addError}</p>}
          <div className="flex gap-2">
            <button onClick={handleAdd} disabled={adding} className="zf-btn zf-btn-primary zf-btn-sm">{adding ? <Loader2 className="w-4 h-4 animate-spin" /> : <Plus className="w-4 h-4" />}{adding ? 'Adding...' : 'Add'}</button>
            <button onClick={() => { setShowAdd(false); setAddError('') }} className="zf-btn zf-btn-ghost zf-btn-sm">Cancel</button>
          </div>
        </div>
      )}

      {users.length === 0 ? (
        <div className="bg-[var(--zf-surface)] rounded-xl p-10 border border-[var(--zf-hairline)] text-center text-[var(--zf-muted)]"><Users className="w-10 h-10 mx-auto mb-3 opacity-50" /><p className="text-sm">No users configured</p></div>
      ) : (
        <div className="bg-[var(--zf-surface)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
          <table className="w-full text-sm">
            <thead><tr className="border-b border-[var(--zf-hairline)]">
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">User</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Role</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Status</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Created</th>
              <th className="text-left px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Last Login</th>
              <th className="text-right px-5 py-3 text-xs font-medium text-[var(--zf-muted)] uppercase tracking-wider">Actions</th>
            </tr></thead>
            <tbody className="divide-y divide-[var(--zf-hairline)]/30">
              {users.map(user => (
                <tr key={user.id} className="hover:bg-black/[0.04] transition-colors">
                  <td className="px-5 py-3"><div className="flex items-center gap-3"><div className="w-8 h-8 rounded-full bg-[var(--zf-ink)] flex items-center justify-center text-xs font-bold text-white uppercase">{user.username.charAt(0)}</div><span className="text-[var(--zf-ink)] font-medium">{user.username}</span></div></td>
                  <td className="px-5 py-3"><span className={`px-2.5 py-0.5 rounded-full text-xs font-medium border capitalize ${roleBadge(user.role)}`}>{user.role}</span></td>
                  <td className="px-5 py-3">
                    <button onClick={() => handleToggle(user.id, user.enabled)} className="flex items-center gap-1.5" aria-label={`${user.enabled ? 'Disable' : 'Enable'} user ${user.username}`}>
                      <div className={`relative w-8 h-4 rounded-full transition-colors ${user.enabled ? 'bg-emerald-500' : 'bg-[var(--zf-hairline)]'}`} role="switch" aria-checked={user.enabled}><div className={`absolute top-0.5 w-3.5 h-3.5 rounded-full bg-[var(--zf-surface)] transition-transform ${user.enabled ? 'translate-x-4' : 'translate-x-0.5'}`} /></div>
                      <span className={`text-xs ${user.enabled ? 'text-emerald-700' : 'text-[var(--zf-muted)]'}`}>{user.enabled ? 'Active' : 'Disabled'}</span>
                    </button>
                  </td>
                  <td className="px-5 py-3 text-xs text-[var(--zf-muted)]">{user.created_at ? new Date(user.created_at).toLocaleDateString() : '-'}</td>
                  <td className="px-5 py-3 text-xs text-[var(--zf-muted)]">{user.last_login ? new Date(user.last_login).toLocaleString() : 'Never'}</td>
                  <td className="px-5 py-3 text-right">
                    <button onClick={() => handleDelete(user.id, user.username)} className="p-1.5 text-[var(--zf-muted)] hover:text-red-600 hover:bg-red-500/10 rounded-lg transition-colors" title="Delete user"><Trash2 className="w-4 h-4" /></button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
        </>
      ) : null}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel ?? 'Delete'}
          variant={confirmState.variant ?? 'danger'}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}
