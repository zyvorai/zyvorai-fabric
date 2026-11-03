// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { Wrench, Key, Terminal, Tag, Lock, Package, Search, AlertTriangle, Loader2 } from 'lucide-react'
import type { VM } from '../../api/vm'
import { rescueVM, inspectVM } from '../../api/guestRescue'
import { useToastContext } from '../../contexts/ToastContext'
import { toastFailure } from '../../utils/toastError'
import { usePermissions } from '../../hooks/usePermissions'
import { AppleTerminalFrame } from '../../components/AppleTerminalFrame'

export default function RescueTab({ vm }: { vm: VM }) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const stopped = vm.state === 'stopped'
  const disabled = !canWrite || !stopped

  const [sshUser, setSshUser] = useState('')
  const [sshKey, setSshKey] = useState('')
  const [hostname, setHostname] = useState('')
  const [pwUser, setPwUser] = useState('')
  const [password, setPassword] = useState('')
  const [packages, setPackages] = useState('')
  const [allowNetwork, setAllowNetwork] = useState(false)
  const [inspectResult, setInspectResult] = useState<Record<string, unknown> | null>(null)
  const [busy, setBusy] = useState<string | null>(null)

  const run = async (key: string, fn: () => Promise<unknown>, successMsg: string) => {
    setBusy(key)
    try {
      await fn()
      toast.success(successMsg)
    } catch (err) {
      toastFailure(toast, 'Rescue operation failed', err)
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="max-w-3xl space-y-4">
      <div className="flex items-start gap-2 text-sm text-[var(--zf-muted)] bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)] px-4 py-3">
        <Wrench className="w-4 h-4 text-purple-400 shrink-0 mt-0.5" />
        <div>
          Offline guest configuration via GuestKit — mounts this VM's disk directly, no network or
          in-guest agent needed. Two things GuestKit doesn't support on Linux, so they're not offered
          here: static IP/gateway (Windows-only), and creating a brand-new user account — SSH key
          injection and password reset both require the target user to already exist on the image
          (e.g. <code className="text-[var(--zf-muted)]">root</code>, or a user your image/cloud-init already created).
        </div>
      </div>

      {!stopped && (
        <div className="flex items-center gap-2 text-sm text-amber-400/90 bg-amber-500/10 border border-amber-500/20 rounded-lg px-4 py-3">
          <AlertTriangle className="w-4 h-4 shrink-0" />
          Stop this VM first — GuestKit needs exclusive access to the disk, which a running VM already holds.
        </div>
      )}
      {!canWrite && (
        <p className="text-sm text-amber-400/90 bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2">
          Viewer accounts cannot run rescue operations.
        </p>
      )}

      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)]">
          <Key className="w-4 h-4 text-emerald-600" />
          Inject SSH Key
        </div>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          <input
            value={sshUser} onChange={(e) => setSshUser(e.target.value)} disabled={disabled}
            placeholder="Existing Linux user (e.g. root)"
            className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
          />
          <input
            value={sshKey} onChange={(e) => setSshKey(e.target.value)} disabled={disabled}
            placeholder="ssh-ed25519 AAAA..."
            className="md:col-span-2 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] font-mono disabled:opacity-50"
          />
        </div>
        <div className="flex gap-2">
          <button
            onClick={() => run('inject-key', () => rescueVM(vm.name, { operation: 'inject-ssh-key', user: sshUser, key: sshKey }), 'SSH key injected')}
            disabled={disabled || !sshUser || !sshKey || busy !== null}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm font-medium text-white disabled:opacity-50"
          >
            {busy === 'inject-key' && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
            Inject Key
          </button>
          <button
            onClick={() => run('enable-ssh', () => rescueVM(vm.name, { operation: 'enable-ssh' }), 'SSH enabled')}
            disabled={disabled || busy !== null}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-canvas-alt)] hover:bg-[var(--zf-hairline)] rounded-lg text-sm font-medium text-[var(--zf-ink)] disabled:opacity-50"
          >
            {busy === 'enable-ssh' && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
            Enable SSH Service
          </button>
        </div>
      </div>

      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)]">
          <Tag className="w-4 h-4 text-cyan-400" />
          Set Hostname
        </div>
        <div className="flex gap-2">
          <input
            value={hostname} onChange={(e) => setHostname(e.target.value)} disabled={disabled}
            placeholder="new-hostname"
            className="flex-1 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
          />
          <button
            onClick={() => run('hostname', () => rescueVM(vm.name, { operation: 'set-hostname', hostname }), 'Hostname set')}
            disabled={disabled || !hostname || busy !== null}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm font-medium text-white disabled:opacity-50"
          >
            {busy === 'hostname' && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
            Apply
          </button>
        </div>
      </div>

      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)]">
          <Lock className="w-4 h-4 text-red-600" />
          Reset Password
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <input
            value={pwUser} onChange={(e) => setPwUser(e.target.value)} disabled={disabled}
            placeholder="Existing Linux user"
            className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
          />
          <input
            value={password} onChange={(e) => setPassword(e.target.value)} disabled={disabled} type="password"
            placeholder="New password"
            className="bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
          />
        </div>
        <button
          onClick={() => run('reset-pw', () => rescueVM(vm.name, { operation: 'reset-password', user: pwUser, password }), 'Password reset')}
          disabled={disabled || !pwUser || !password || busy !== null}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm font-medium text-white disabled:opacity-50"
        >
          {busy === 'reset-pw' && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
          Reset Password
        </button>
      </div>

      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)]">
          <Package className="w-4 h-4 text-amber-400" />
          Install Packages
        </div>
        <input
          value={packages} onChange={(e) => setPackages(e.target.value)} disabled={disabled}
          placeholder="package-one, package-two"
          className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
        />
        <label className="flex items-center gap-2 text-sm text-[var(--zf-muted)]">
          <input type="checkbox" checked={allowNetwork} onChange={(e) => setAllowNetwork(e.target.checked)} disabled={disabled} />
          Allow network access during install (uses the host's DNS, restored afterward)
        </label>
        <button
          onClick={() => run('install-pkgs', () => rescueVM(vm.name, {
            operation: 'install-packages',
            packages: packages.split(',').map((p) => p.trim()).filter(Boolean),
            network: allowNetwork,
          }), 'Packages installed')}
          disabled={disabled || !packages.trim() || busy !== null}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm font-medium text-white disabled:opacity-50"
        >
          {busy === 'install-pkgs' && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
          Install
        </button>
      </div>

      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)]">
          <Search className="w-4 h-4 text-[var(--zf-muted)]" />
          Pull Guest Info
        </div>
        <button
          onClick={async () => {
            setBusy('inspect')
            try {
              setInspectResult(await inspectVM(vm.name))
            } catch (err) {
              toastFailure(toast, 'Inspect failed', err)
            } finally {
              setBusy(null)
            }
          }}
          disabled={!stopped || busy !== null}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-canvas-alt)] hover:bg-[var(--zf-hairline)] rounded-lg text-sm font-medium text-[var(--zf-ink)] disabled:opacity-50"
        >
          {busy === 'inspect' && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
          <Terminal className="w-3.5 h-3.5" />
          Inspect Disk
        </button>
        {inspectResult && (
          <AppleTerminalFrame
            title="inspect — guest disk"
            bodyClassName="max-h-64 overflow-auto px-3 py-2 whitespace-pre"
          >
            {JSON.stringify(inspectResult, null, 2)}
          </AppleTerminalFrame>
        )}
      </div>
    </div>
  )
}
