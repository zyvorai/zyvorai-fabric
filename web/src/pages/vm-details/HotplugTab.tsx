// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { Cpu, MemoryStick, HardDrive, Network, Loader2, AlertCircle } from 'lucide-react'
import type { VM } from '../../api/vm'
import {
  hotplugCpu,
  hotplugMemory,
  hotplugDisk,
  hotplugNic,
  hotremoveDisk,
  hotremoveNic,
} from '../../api/hotplug'
import { useToastContext } from '../../contexts/ToastContext'
import { toastFailure } from '../../utils/toastError'
import { usePermissions } from '../../hooks/usePermissions'

export default function HotplugTab({ vm }: { vm: VM }) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [cpuCount, setCpuCount] = useState(vm.cpus + 1)
  const [memMb, setMemMb] = useState(512)
  const [diskPath, setDiskPath] = useState('')
  const [nicBridge, setNicBridge] = useState('')
  const [nicUplink, setNicUplink] = useState('')
  const [nicGuestIps, setNicGuestIps] = useState('')
  const [diskDeviceId, setDiskDeviceId] = useState('')
  const [nicDeviceId, setNicDeviceId] = useState('')
  const [busy, setBusy] = useState<string | null>(null)

  const running = vm.state === 'running'

  const run = async (key: string, fn: () => Promise<unknown>, success: string) => {
    setBusy(key)
    try {
      await fn()
      toast.success(success)
    } catch (e) {
      toastFailure(toast, 'Hotplug failed', e)
    } finally {
      setBusy(null)
    }
  }

  if (!running) {
    return (
      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-8 text-center">
        <AlertCircle className="w-10 h-10 text-amber-400 mx-auto mb-3" />
        <p className="text-[var(--zf-ink)] font-medium mb-1">VM must be running</p>
        <p className="text-[var(--zf-muted)] text-sm">Start the VM to hotplug CPU, memory, disks, or NICs.</p>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {!canWrite && (
        <p className="text-sm text-amber-400/90 bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2">
          Viewer accounts cannot change live resources.
        </p>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <Section icon={Cpu} title="CPU">
          <div className="flex gap-2 items-end">
            <div className="flex-1">
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Target vCPU count</label>
              <input aria-label="Target vCPU count"
                type="number"
                min={1}
                max={256}
                value={cpuCount}
                onChange={(e) => setCpuCount(parseInt(e.target.value) || 1)}
                disabled={!canWrite}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
              />
            </div>
            <ActionButton
              disabled={!canWrite || busy !== null}
              loading={busy === 'cpu'}
              onClick={() => run('cpu', () => hotplugCpu(vm.name, { count: cpuCount }), `CPU count set to ${cpuCount}`)}
              label="Apply"
            />
          </div>
        </Section>

        <Section icon={MemoryStick} title="Memory">
          <div className="flex gap-2 items-end">
            <div className="flex-1">
              <label className="block text-xs text-[var(--zf-muted)] mb-1">Add memory (MB)</label>
              <input aria-label="Add memory (MB)"
                type="number"
                min={128}
                step={128}
                value={memMb}
                onChange={(e) => setMemMb(parseInt(e.target.value) || 128)}
                disabled={!canWrite}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
              />
            </div>
            <ActionButton
              disabled={!canWrite || busy !== null}
              loading={busy === 'mem'}
              onClick={() => run('mem', () => hotplugMemory(vm.name, { size_mb: memMb }), `Added ${memMb} MB memory`)}
              label="Add"
            />
          </div>
        </Section>

        <Section icon={HardDrive} title="Disk">
          <div className="space-y-2">
            <input
              type="text"
              placeholder="/path/to/disk.qcow2"
              value={diskPath}
              onChange={(e) => setDiskPath(e.target.value)}
              disabled={!canWrite}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] font-mono disabled:opacity-50"
            />
            <ActionButton
              disabled={!canWrite || busy !== null || !diskPath.trim()}
              loading={busy === 'disk'}
              onClick={() => run('disk', () => hotplugDisk(vm.name, { path: diskPath.trim() }), 'Disk attached')}
              label="Attach disk"
            />
            <div className="flex gap-2 items-end pt-1">
              <input
                type="text"
                placeholder="Device ID to remove"
                value={diskDeviceId}
                onChange={(e) => setDiskDeviceId(e.target.value)}
                disabled={!canWrite}
                className="flex-1 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] font-mono disabled:opacity-50"
              />
              <ActionButton
                disabled={!canWrite || busy !== null || !diskDeviceId.trim()}
                loading={busy === 'disk-rm'}
                onClick={() => run('disk-rm', () => hotremoveDisk(vm.name, diskDeviceId.trim()), 'Disk removed')}
                label="Remove"
                variant="danger"
              />
            </div>
          </div>
        </Section>

        <Section icon={Network} title="Network">
          <div className="space-y-2">
            <input
              type="text"
              placeholder="Bridge name (e.g. br0)"
              value={nicBridge}
              onChange={(e) => setNicBridge(e.target.value)}
              disabled={!canWrite}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
            />
            <input
              type="text"
              placeholder="Or direct uplink NIC (e.g. enp1s0)"
              value={nicUplink}
              onChange={(e) => setNicUplink(e.target.value)}
              disabled={!canWrite}
              className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
            />
            {nicUplink.trim() && (
              <input
                type="text"
                placeholder="Guest IPv4s, optional (comma-separated)"
                value={nicGuestIps}
                onChange={(e) => setNicGuestIps(e.target.value)}
                disabled={!canWrite}
                className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] disabled:opacity-50"
              />
            )}
            <ActionButton
              disabled={!canWrite || busy !== null || (!nicBridge.trim() && !nicUplink.trim()) || (!!nicBridge.trim() && !!nicUplink.trim())}
              loading={busy === 'nic'}
              onClick={() => {
                const uplink = nicUplink.trim()
                const ips = nicGuestIps.split(',').map((s) => s.trim()).filter(Boolean)
                return run(
                  'nic',
                  () => hotplugNic(vm.name, uplink
                    ? { bridge: '', direct_uplink: uplink, ...(ips.length ? { direct_guest_ips: ips } : {}) }
                    : { bridge: nicBridge.trim() }),
                  'NIC attached',
                )
              }}
              label="Attach NIC"
            />
            <div className="flex gap-2 items-end pt-1">
              <input
                type="text"
                placeholder="NIC device ID to remove"
                value={nicDeviceId}
                onChange={(e) => setNicDeviceId(e.target.value)}
                disabled={!canWrite}
                className="flex-1 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)] font-mono disabled:opacity-50"
              />
              <ActionButton
                disabled={!canWrite || busy !== null || !nicDeviceId.trim()}
                loading={busy === 'nic-rm'}
                onClick={() => run('nic-rm', () => hotremoveNic(vm.name, nicDeviceId.trim()), 'NIC removed')}
                label="Remove"
                variant="danger"
              />
            </div>
          </div>
        </Section>
      </div>
    </div>
  )
}

function Section({ icon: Icon, title, children }: { icon: typeof Cpu; title: string; children: React.ReactNode }) {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-5">
      <h3 className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-4">
        <Icon className="w-4 h-4 text-[var(--zf-link)]" />
        {title}
      </h3>
      {children}
    </div>
  )
}

function ActionButton({
  onClick,
  label,
  disabled,
  loading,
  variant = 'primary',
}: {
  onClick: () => void
  label: string
  disabled?: boolean
  loading?: boolean
  variant?: 'primary' | 'danger'
}) {
  const cls =
    variant === 'danger'
      ? 'bg-red-600/80 hover:bg-red-600 text-white'
      : 'bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] text-white'
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled || loading}
      className={`px-3 py-2 rounded-lg text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-1.5 ${cls}`}
    >
      {loading && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
      {label}
    </button>
  )
}
