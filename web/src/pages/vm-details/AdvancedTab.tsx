// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react'
import { Loader2, RefreshCw, Save, Plus, ShieldCheck } from 'lucide-react'
import type { VM } from '../../api/vm'
import {
  getBootConfig,
  updateBootConfig,
  getDisplay,
  updateDisplay,
  getCPUConfig,
  updateCPUConfig,
  listCPUModels,
  getWatchdog,
  setWatchdog,
  listSerials,
  addSerial,
  type BootConfig,
  type DisplayConfig,
  type CPUModelConfig,
  type CPUModel,
  type WatchdogConfig,
  type SerialConfig,
} from '../../api/devices'
import {
  getFirmwareStatus,
  enableUefi,
  enableSecureBoot,
  disableSecureBoot,
  resetNvram,
  type FirmwareStatus,
} from '../../api/firmware'
import {
  getCpuTopology,
  getNumaTopology,
  getNumaPlacement,
  setCpuPinning,
  removeCpuPinning,
  getCpuAffinity,
  type CpuTopology,
  type NumaTopology,
  type CpuPinning,
} from '../../api/system'
import ErrorBanner from '../../components/ErrorBanner'
import { formatUserError } from '../../utils/apiError'
import { toastFailure } from '../../utils/toastError'
import { useToastContext } from '../../contexts/ToastContext'
import { usePermissions } from '../../hooks/usePermissions'

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
      <div className="px-5 py-3 border-b border-[var(--zf-hairline)]">
        <h3 className="text-sm font-medium text-[var(--zf-ink)]">{title}</h3>
      </div>
      <div className="p-5 space-y-3">{children}</div>
    </div>
  )
}

const inputCls = 'w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded px-3 py-1.5 text-sm'
const labelCls = 'block text-xs font-medium text-[var(--zf-muted)] mb-1'

export default function AdvancedTab({ vm }: { vm: VM }) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)

  const [boot, setBoot] = useState<BootConfig | null>(null)
  const [display, setDisplay] = useState<DisplayConfig | null>(null)
  const [cpuConfig, setCpuConfig] = useState<CPUModelConfig | null>(null)
  const [cpuModels, setCpuModels] = useState<CPUModel[]>([])
  const [watchdog, setWatchdogState] = useState<WatchdogConfig | null>(null)
  const [serials, setSerialsState] = useState<SerialConfig[]>([])
  const [firmware, setFirmware] = useState<FirmwareStatus | null>(null)
  const [cpuTopology, setCpuTopology] = useState<CpuTopology | null>(null)
  const [numaTopology, setNumaTopology] = useState<NumaTopology | null>(null)
  const [cpuAffinity, setCpuAffinity] = useState<number[] | null>(null)
  const [pinMode, setPinMode] = useState<CpuPinning['type']>('Auto')
  const [pinNumaNode, setPinNumaNode] = useState(0)
  const [pinSocket, setPinSocket] = useState(0)
  const [pinExplicit, setPinExplicit] = useState('')

  const [savingBoot, setSavingBoot] = useState(false)
  const [savingDisplay, setSavingDisplay] = useState(false)
  const [savingCpu, setSavingCpu] = useState(false)
  const [savingWatchdog, setSavingWatchdog] = useState(false)
  const [addingSerial, setAddingSerial] = useState(false)
  const [newSerialType, setNewSerialType] = useState<SerialConfig['type']>('pty')
  const [firmwareBusy, setFirmwareBusy] = useState(false)
  const [savingPinning, setSavingPinning] = useState(false)

  const load = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const [b, d, c, models, w, s, f, cpuTopo, numaTopo, affinity] = await Promise.all([
        getBootConfig(vm.name),
        getDisplay(vm.name),
        getCPUConfig(vm.name),
        listCPUModels().catch(() => []),
        getWatchdog(vm.name).catch(() => null),
        listSerials(vm.name).catch(() => []),
        getFirmwareStatus(vm.name).catch(() => null),
        getCpuTopology().catch(() => null),
        getNumaTopology().catch(() => null),
        getCpuAffinity(vm.name).catch(() => null),
      ])
      setBoot(b)
      setDisplay(d)
      setCpuConfig(c)
      setCpuModels(models)
      setWatchdogState(w)
      setSerialsState(s)
      setFirmware(f)
      setCpuTopology(cpuTopo)
      setNumaTopology(numaTopo)
      setCpuAffinity(affinity)
    } catch (e) {
      setLoadError(formatUserError(e))
    } finally {
      setLoading(false)
    }
  }, [vm.name])

  useEffect(() => {
    void load()
  }, [load])

  const saveBoot = async () => {
    if (!boot) return
    setSavingBoot(true)
    try {
      await updateBootConfig(vm.name, boot)
      toast.success('Boot configuration saved')
    } catch (e) {
      toastFailure(toast, 'Failed to save boot configuration', e)
    } finally {
      setSavingBoot(false)
    }
  }

  const saveDisplay = async () => {
    if (!display) return
    setSavingDisplay(true)
    try {
      await updateDisplay(vm.name, display)
      toast.success('Display configuration saved')
    } catch (e) {
      toastFailure(toast, 'Failed to save display configuration', e)
    } finally {
      setSavingDisplay(false)
    }
  }

  const saveCpu = async () => {
    if (!cpuConfig) return
    setSavingCpu(true)
    try {
      await updateCPUConfig(vm.name, cpuConfig)
      toast.success('CPU configuration saved')
    } catch (e) {
      toastFailure(toast, 'Failed to save CPU configuration', e)
    } finally {
      setSavingCpu(false)
    }
  }

  const saveWatchdog = async (model: WatchdogConfig['model'], action: WatchdogConfig['action']) => {
    setSavingWatchdog(true)
    try {
      await setWatchdog(vm.name, { model, action })
      setWatchdogState({ model, action })
      toast.success('Watchdog configured')
    } catch (e) {
      toastFailure(toast, 'Failed to configure watchdog', e)
    } finally {
      setSavingWatchdog(false)
    }
  }

  const applyPinning = async () => {
    setSavingPinning(true)
    try {
      if (pinMode === 'Explicit') {
        const cpus = pinExplicit.split(',').map((s) => s.trim()).filter(Boolean).map(Number)
        if (cpus.some((n) => Number.isNaN(n))) {
          toastFailure(toast, 'Failed to apply CPU pinning', new Error('Explicit CPUs must be a comma-separated list of numbers'))
          return
        }
        const pins = cpus.map((physical_cpu, i) => ({ vcpu_id: i, physical_cpu }))
        await setCpuPinning(vm.name, { pinning: { type: 'Explicit', value: pins } })
      } else if (pinMode === 'NumaNode') {
        await setCpuPinning(vm.name, { pinning: { type: 'NumaNode', value: pinNumaNode } })
      } else if (pinMode === 'Socket') {
        await setCpuPinning(vm.name, { pinning: { type: 'Socket', value: pinSocket } })
      } else {
        await setCpuPinning(vm.name, { pinning: { type: 'Auto' } })
      }
      toast.success(
        vm.state === 'running'
          ? 'CPU pinning applied'
          : 'CPU pinning saved — will apply on next start',
      )
      setCpuAffinity(await getCpuAffinity(vm.name).catch(() => null))
    } catch (e) {
      toastFailure(toast, 'Failed to apply CPU pinning', e)
    } finally {
      setSavingPinning(false)
    }
  }

  const clearPinning = async () => {
    setSavingPinning(true)
    try {
      await removeCpuPinning(vm.name)
      toast.success('CPU pinning removed')
      setCpuAffinity(await getCpuAffinity(vm.name).catch(() => null))
    } catch (e) {
      toastFailure(toast, 'Failed to remove CPU pinning', e)
    } finally {
      setSavingPinning(false)
    }
  }

  const applyRecommendedNuma = async () => {
    try {
      const rec = await getNumaPlacement(vm.memory, vm.cpus)
      setPinMode('NumaNode')
      setPinNumaNode(rec.numa_node)
      toast.success(`Recommended NUMA node ${rec.numa_node} (affinity: ${rec.cpu_affinity.join(', ')})`)
    } catch (e) {
      toastFailure(toast, 'Failed to get NUMA placement recommendation', e)
    }
  }

  const submitSerial = async () => {
    setAddingSerial(true)
    try {
      await addSerial(vm.name, { type: newSerialType })
      toast.success('Serial console added')
      const s = await listSerials(vm.name)
      setSerialsState(s)
    } catch (e) {
      toastFailure(toast, 'Failed to add serial console', e)
    } finally {
      setAddingSerial(false)
    }
  }

  const refreshFirmware = async () => {
    setFirmware(await getFirmwareStatus(vm.name).catch(() => null))
  }

  const handleEnableUefi = async () => {
    setFirmwareBusy(true)
    try {
      await enableUefi(vm.name, { secure_boot: false })
      toast.success('UEFI enabled')
      await refreshFirmware()
    } catch (e) {
      toastFailure(toast, 'Failed to enable UEFI', e)
    } finally {
      setFirmwareBusy(false)
    }
  }

  const handleToggleSecureBoot = async () => {
    setFirmwareBusy(true)
    try {
      if (firmware?.secure_boot_enabled) {
        await disableSecureBoot(vm.name)
        toast.success('Secure Boot disabled')
      } else {
        await enableSecureBoot(vm.name)
        toast.success('Secure Boot enabled')
      }
      await refreshFirmware()
    } catch (e) {
      toastFailure(toast, 'Failed to change Secure Boot', e)
    } finally {
      setFirmwareBusy(false)
    }
  }

  const handleResetNvram = async () => {
    setFirmwareBusy(true)
    try {
      await resetNvram(vm.name)
      toast.success('NVRAM reset to defaults')
      await refreshFirmware()
    } catch (e) {
      toastFailure(toast, 'Failed to reset NVRAM', e)
    } finally {
      setFirmwareBusy(false)
    }
  }

  if (loading) {
    return (
      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-8 text-center">
        <Loader2 className="w-6 h-6 text-[var(--zf-muted)] mx-auto mb-2 animate-spin" />
        <p className="text-[var(--zf-muted)] text-sm">Loading advanced configuration...</p>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <button
          onClick={() => void load()}
          className="flex items-center gap-1.5 px-3 py-1.5 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] text-sm"
        >
          <RefreshCw className="w-3.5 h-3.5" />
          Refresh
        </button>
      </div>

      {loadError && (
        <ErrorBanner title="Could not load advanced configuration" headline={loadError} onRetry={() => void load()} />
      )}

      {!canWrite && (
        <p className="text-sm text-amber-400/90 bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2">
          Viewer accounts cannot change advanced configuration.
        </p>
      )}

      <Section title="Firmware">
        {firmware ? (
          <>
            <p className="text-sm text-[var(--zf-ink)]">
              <span className="font-mono">{firmware.firmware_type}</span>
              {firmware.tpm_enabled && <span className="text-[var(--zf-muted)]"> · TPM {firmware.tpm_version}</span>}
            </p>
            <p className="text-sm">
              Secure Boot: <span className={firmware.secure_boot_enabled ? 'text-emerald-600' : 'text-[var(--zf-muted)]'}>{firmware.secure_boot_enabled ? 'Enabled' : 'Disabled'}</span>
            </p>
          </>
        ) : (
          <p className="text-sm text-[var(--zf-muted)]">UEFI is not enabled for this VM (currently BIOS boot).</p>
        )}
        {canWrite && (
          <div className="flex flex-wrap gap-2">
            {!firmware && (
              <button onClick={() => void handleEnableUefi()} disabled={firmwareBusy} className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50">
                <ShieldCheck className="w-3.5 h-3.5" />{firmwareBusy ? 'Working…' : 'Enable UEFI'}
              </button>
            )}
            {firmware && (
              <>
                <button onClick={() => void handleToggleSecureBoot()} disabled={firmwareBusy} className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50">
                  <ShieldCheck className="w-3.5 h-3.5" />{firmwareBusy ? 'Working…' : firmware.secure_boot_enabled ? 'Disable Secure Boot' : 'Enable Secure Boot'}
                </button>
                <button onClick={() => void handleResetNvram()} disabled={firmwareBusy} className="px-3 py-1.5 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] rounded-lg text-sm disabled:opacity-50">
                  {firmwareBusy ? 'Working…' : 'Reset NVRAM'}
                </button>
              </>
            )}
          </div>
        )}
      </Section>

      {boot && (
        <Section title="Boot">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={labelCls}>Firmware</label>
              <select
                disabled={!canWrite}
                value={boot.firmware}
                onChange={(e) => setBoot({ ...boot, firmware: e.target.value as BootConfig['firmware'] })}
                className={inputCls}
              >
                <option value="bios">BIOS</option>
                <option value="uefi">UEFI</option>
              </select>
            </div>
            <div className="flex items-end pb-1.5">
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  disabled={!canWrite || boot.firmware !== 'uefi'}
                  checked={boot.secure_boot}
                  onChange={(e) => setBoot({ ...boot, secure_boot: e.target.checked })}
                />
                Secure Boot (UEFI only)
              </label>
            </div>
          </div>
          <div>
            <label className={labelCls}>Boot order (comma-separated)</label>
            <input
              disabled={!canWrite}
              className={inputCls}
              value={boot.boot_order.join(',')}
              onChange={(e) => setBoot({ ...boot, boot_order: e.target.value.split(',').map((s) => s.trim()).filter(Boolean) })}
              placeholder="hd,cdrom,network"
            />
          </div>
          {canWrite && (
            <button onClick={() => void saveBoot()} disabled={savingBoot} className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50">
              <Save className="w-3.5 h-3.5" />{savingBoot ? 'Saving…' : 'Save Boot Config'}
            </button>
          )}
        </Section>
      )}

      {display && (
        <Section title="Display">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={labelCls}>Protocol</label>
              <select
                disabled={!canWrite}
                value={display.type}
                onChange={(e) => setDisplay({ ...display, type: e.target.value as DisplayConfig['type'] })}
                className={inputCls}
              >
                <option value="vnc">VNC</option>
                <option value="spice">SPICE</option>
              </select>
            </div>
            <div>
              <label className={labelCls}>Keymap</label>
              <input disabled={!canWrite} className={inputCls} value={display.keymap ?? ''} onChange={(e) => setDisplay({ ...display, keymap: e.target.value || undefined })} placeholder="en-us" />
            </div>
          </div>
          {canWrite && (
            <button onClick={() => void saveDisplay()} disabled={savingDisplay} className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50">
              <Save className="w-3.5 h-3.5" />{savingDisplay ? 'Saving…' : 'Save Display Config'}
            </button>
          )}
        </Section>
      )}

      {cpuConfig && (
        <Section title="CPU Model">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={labelCls}>Mode</label>
              <select
                disabled={!canWrite}
                value={cpuConfig.mode}
                onChange={(e) => setCpuConfig({ ...cpuConfig, mode: e.target.value as CPUModelConfig['mode'] })}
                className={inputCls}
              >
                <option value="host-model">Host model</option>
                <option value="host-passthrough">Host passthrough</option>
                <option value="custom">Custom</option>
              </select>
            </div>
            {cpuConfig.mode === 'custom' && (
              <div>
                <label className={labelCls}>Model</label>
                <select
                  disabled={!canWrite}
                  value={cpuConfig.model}
                  onChange={(e) => setCpuConfig({ ...cpuConfig, model: e.target.value })}
                  className={inputCls}
                >
                  <option value="">Select a model</option>
                  {cpuModels.map((m) => (
                    <option key={m.name} value={m.name}>{m.name} ({m.vendor})</option>
                  ))}
                </select>
              </div>
            )}
          </div>
          {cpuModels.length === 0 && cpuConfig.mode === 'custom' && (
            <p className="text-xs text-[var(--zf-muted)]">No host CPU model list available — enter a model name via Custom mode only if you know it's supported by qemu on this host.</p>
          )}
          {canWrite && (
            <button onClick={() => void saveCpu()} disabled={savingCpu} className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50">
              <Save className="w-3.5 h-3.5" />{savingCpu ? 'Saving…' : 'Save CPU Config'}
            </button>
          )}
        </Section>
      )}

      {(cpuTopology || numaTopology) && (
        <Section title="CPU Pinning & NUMA">
          {cpuTopology && (
            <p className="text-sm text-[var(--zf-ink)]">
              Host: <span className="font-mono">{cpuTopology.total_cpus}</span> CPUs across{' '}
              <span className="font-mono">{cpuTopology.sockets}</span> socket{cpuTopology.sockets === 1 ? '' : 's'}
              {numaTopology && <> · <span className="font-mono">{numaTopology.nodes.length}</span> NUMA node{numaTopology.nodes.length === 1 ? '' : 's'}</>}
            </p>
          )}
          <p className="text-sm text-[var(--zf-muted)]">
            Current affinity:{' '}
            {cpuAffinity && cpuAffinity.length > 0 ? (
              <span className="font-mono text-[var(--zf-ink)]">{cpuAffinity.join(', ')}</span>
            ) : (
              <span className="text-[var(--zf-muted)]">none (floating across all host CPUs)</span>
            )}
          </p>

          {canWrite && (
            <>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className={labelCls}>Pinning Mode</label>
                  <select
                    disabled={savingPinning}
                    value={pinMode}
                    onChange={(e) => setPinMode(e.target.value as CpuPinning['type'])}
                    className={inputCls}
                  >
                    <option value="Auto">Auto (no pinning)</option>
                    <option value="NumaNode">Pin to NUMA node</option>
                    <option value="Socket">Pin to socket</option>
                    <option value="Explicit">Explicit vCPU → physical CPU map</option>
                  </select>
                </div>

                {pinMode === 'NumaNode' && numaTopology && (
                  <div>
                    <label className={labelCls}>NUMA Node</label>
                    <select
                      disabled={savingPinning}
                      value={pinNumaNode}
                      onChange={(e) => setPinNumaNode(Number(e.target.value))}
                      className={inputCls}
                    >
                      {numaTopology.nodes.map((n) => (
                        <option key={n.id} value={n.id}>
                          Node {n.id} ({n.cpus.length} CPUs, {Math.round(n.memory_free_mb / 1024)} GB free)
                        </option>
                      ))}
                    </select>
                  </div>
                )}

                {pinMode === 'Socket' && cpuTopology && (
                  <div>
                    <label className={labelCls}>Socket</label>
                    <select
                      disabled={savingPinning}
                      value={pinSocket}
                      onChange={(e) => setPinSocket(Number(e.target.value))}
                      className={inputCls}
                    >
                      {Array.from({ length: cpuTopology.sockets }, (_, i) => i).map((s) => (
                        <option key={s} value={s}>Socket {s}</option>
                      ))}
                    </select>
                  </div>
                )}
              </div>

              {pinMode === 'Explicit' && (
                <div>
                  <label className={labelCls}>Physical CPUs (comma-separated, one per vCPU in order)</label>
                  <input
                    disabled={savingPinning}
                    className={inputCls}
                    value={pinExplicit}
                    onChange={(e) => setPinExplicit(e.target.value)}
                    placeholder="0,1,2,3"
                  />
                </div>
              )}

              <div className="flex flex-wrap gap-2">
                <button onClick={() => void applyPinning()} disabled={savingPinning} className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50">
                  <Save className="w-3.5 h-3.5" />{savingPinning ? 'Applying…' : 'Apply Pinning'}
                </button>
                {numaTopology && (
                  <button onClick={() => void applyRecommendedNuma()} disabled={savingPinning} className="px-3 py-1.5 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] rounded-lg text-sm disabled:opacity-50">
                    Use Recommended NUMA Node
                  </button>
                )}
                {cpuAffinity && cpuAffinity.length > 0 && (
                  <button onClick={() => void clearPinning()} disabled={savingPinning} className="px-3 py-1.5 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] rounded-lg text-sm disabled:opacity-50">
                    Clear Pinning
                  </button>
                )}
              </div>
              {vm.state === 'running' && (
                <p className="text-xs text-amber-400/80">This VM is running — pinning changes apply immediately to the live process.</p>
              )}
            </>
          )}
        </Section>
      )}

      <Section title="Watchdog">
        {watchdog ? (
          <p className="text-sm text-[var(--zf-ink)]">
            <span className="font-mono">{watchdog.model}</span> — on hang: <span className="font-mono">{watchdog.action}</span>
          </p>
        ) : (
          <p className="text-sm text-[var(--zf-muted)]">No watchdog configured.</p>
        )}
        {canWrite && (
          <div className="grid grid-cols-2 gap-3">
            <select
              disabled={savingWatchdog}
              defaultValue={watchdog?.model ?? 'i6300esb'}
              id="watchdog-model"
              className={inputCls}
            >
              <option value="i6300esb">i6300esb</option>
              <option value="ib700">ib700</option>
            </select>
            <select
              disabled={savingWatchdog}
              defaultValue={watchdog?.action ?? 'reset'}
              id="watchdog-action"
              className={inputCls}
            >
              <option value="reset">Reset</option>
              <option value="shutdown">Shutdown</option>
              <option value="poweroff">Poweroff</option>
              <option value="pause">Pause</option>
              <option value="none">None</option>
            </select>
          </div>
        )}
        {canWrite && (
          <button
            disabled={savingWatchdog}
            onClick={() => {
              const model = (document.getElementById('watchdog-model') as HTMLSelectElement).value as WatchdogConfig['model']
              const action = (document.getElementById('watchdog-action') as HTMLSelectElement).value as WatchdogConfig['action']
              void saveWatchdog(model, action)
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50"
          >
            <Save className="w-3.5 h-3.5" />{savingWatchdog ? 'Saving…' : 'Set Watchdog'}
          </button>
        )}
      </Section>

      <Section title="Serial Consoles">
        {serials.length === 0 ? (
          <p className="text-sm text-[var(--zf-muted)]">No serial consoles configured.</p>
        ) : (
          <div className="space-y-1.5">
            {serials.map((s, i) => (
              <div key={i} className="flex items-center gap-2 text-sm bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded px-3 py-1.5">
                <span className="font-mono">{s.type}</span>
                {s.path && <span className="text-[var(--zf-muted)] font-mono">{s.path}</span>}
                {s.source_host && <span className="text-[var(--zf-muted)] font-mono">{s.source_host}:{s.source_port}</span>}
              </div>
            ))}
          </div>
        )}
        {canWrite && (
          <div className="flex items-end gap-2">
            <div>
              <label className={labelCls}>Type</label>
              <select value={newSerialType} onChange={(e) => setNewSerialType(e.target.value as SerialConfig['type'])} className={inputCls}>
                <option value="pty">pty</option>
                <option value="unix">unix</option>
                <option value="tcp">tcp</option>
              </select>
            </div>
            <button onClick={() => void submitSerial()} disabled={addingSerial} className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg text-sm disabled:opacity-50">
              <Plus className="w-3.5 h-3.5" />{addingSerial ? 'Adding…' : 'Add Serial'}
            </button>
          </div>
        )}
      </Section>
    </div>
  )
}
