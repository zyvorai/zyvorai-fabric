// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react'
import { Usb, Cpu, Loader2, RefreshCw } from 'lucide-react'
import type { VM } from '../../api/vm'
import {
  listUSBDevices,
  listPCIDevices,
  attachUSB,
  detachUSB,
  attachPCI,
  detachPCI,
  type USBDevice,
  type PCIDevice,
} from '../../api/devices'
import ErrorBanner from '../../components/ErrorBanner'
import { formatUserError } from '../../utils/apiError'
import { toastFailure } from '../../utils/toastError'
import { useToastContext } from '../../contexts/ToastContext'
import { usePermissions } from '../../hooks/usePermissions'

export default function DevicesTab({ vm }: { vm: VM }) {
  const toast = useToastContext()
  const { canWrite } = usePermissions()
  const [usb, setUsb] = useState<USBDevice[]>([])
  const [pci, setPci] = useState<PCIDevice[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const [u, p] = await Promise.all([listUSBDevices(), listPCIDevices()])
      setUsb(u)
      setPci(p)
    } catch (e) {
      setLoadError(formatUserError(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const attachUsb = async (dev: USBDevice) => {
    setBusy(`usb-${dev.vendor_id}:${dev.product_id}`)
    try {
      await attachUSB(vm.name, { vendor_id: dev.vendor_id, product_id: dev.product_id })
      toast.success('USB device attached')
      await load()
    } catch (e) {
      toastFailure(toast, 'Failed to attach USB device', e)
    } finally {
      setBusy(null)
    }
  }

  const detachUsb = async (dev: USBDevice) => {
    setBusy(`usb-rm-${dev.vendor_id}:${dev.product_id}`)
    try {
      await detachUSB(vm.name, dev.vendor_id, dev.product_id)
      toast.success('USB device detached')
      await load()
    } catch (e) {
      toastFailure(toast, 'Failed to detach USB device', e)
    } finally {
      setBusy(null)
    }
  }

  const attachPci = async (dev: PCIDevice) => {
    setBusy(`pci-${dev.address}`)
    try {
      await attachPCI(vm.name, { address: dev.address })
      toast.success('PCI device attached')
      await load()
    } catch (e) {
      toastFailure(toast, 'Failed to attach PCI device', e)
    } finally {
      setBusy(null)
    }
  }

  const detachPci = async (dev: PCIDevice) => {
    setBusy(`pci-rm-${dev.address}`)
    try {
      await detachPCI(vm.name, dev.address)
      toast.success('PCI device detached')
      await load()
    } catch (e) {
      toastFailure(toast, 'Failed to detach PCI device', e)
    } finally {
      setBusy(null)
    }
  }

  if (loading) {
    return (
      <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-8 text-center">
        <Loader2 className="w-6 h-6 text-[var(--zf-muted)] mx-auto mb-2 animate-spin" />
        <p className="text-[var(--zf-muted)] text-sm">Loading host devices...</p>
      </div>
    )
  }

  const gpus = pci.filter((d) => d.class_name.toLowerCase().includes('vga') || d.class_name.toLowerCase().includes('3d'))
  const otherPci = pci.filter((d) => !gpus.includes(d))

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
        <ErrorBanner title="Could not load devices" headline={loadError} onRetry={() => void load()} />
      )}

      {!canWrite && (
        <p className="text-sm text-amber-400/90 bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2">
          Viewer accounts cannot attach or detach devices.
        </p>
      )}

      <DeviceTable
        icon={Usb}
        title="USB devices"
        empty="No USB devices detected on the host"
        rows={usb.map((d) => ({
          key: `${d.bus}-${d.device}`,
          name: d.product_name || `${d.vendor_id}:${d.product_id}`,
          detail: `${d.vendor_name} · ${d.speed}`,
          attached: d.attached_to === vm.name,
          attachedElsewhere: !!d.attached_to && d.attached_to !== vm.name,
          busy: busy === `usb-${d.vendor_id}:${d.product_id}` || busy === `usb-rm-${d.vendor_id}:${d.product_id}`,
          onAttach: () => void attachUsb(d),
          onDetach: () => void detachUsb(d),
        }))}
        canWrite={canWrite}
      />

      <DeviceTable
        icon={Cpu}
        title="GPU / PCI devices"
        empty="No PCI devices detected on the host"
        rows={[...gpus, ...otherPci].map((d) => ({
          key: d.address,
          name: d.device_name || d.address,
          detail: `${d.class_name} · IOMMU ${d.iommu_group} · ${d.driver || 'no driver'}`,
          attached: d.attached_to === vm.name,
          attachedElsewhere: !!d.attached_to && d.attached_to !== vm.name,
          busy: busy === `pci-${d.address}` || busy === `pci-rm-${d.address}`,
          onAttach: () => void attachPci(d),
          onDetach: () => void detachPci(d),
        }))}
        canWrite={canWrite}
      />
    </div>
  )
}

function DeviceTable({
  icon: Icon,
  title,
  empty,
  rows,
  canWrite,
}: {
  icon: typeof Usb
  title: string
  empty: string
  rows: {
    key: string
    name: string
    detail: string
    attached: boolean
    attachedElsewhere: boolean
    busy: boolean
    onAttach: () => void
    onDetach: () => void
  }[]
  canWrite: boolean
}) {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
      <div className="px-5 py-3 border-b border-[var(--zf-hairline)] flex items-center gap-2">
        <Icon className="w-4 h-4 text-teal-400" />
        <h3 className="text-sm font-medium text-[var(--zf-ink)]">{title}</h3>
      </div>
      {rows.length === 0 ? (
        <p className="p-6 text-sm text-[var(--zf-muted)] text-center">{empty}</p>
      ) : (
        <div className="divide-y divide-[var(--zf-hairline)]">
          {rows.map((row) => (
            <div key={row.key} className="flex items-center justify-between gap-4 px-5 py-3">
              <div className="min-w-0">
                <div className="text-sm text-[var(--zf-ink)] truncate">{row.name}</div>
                <div className="text-xs text-[var(--zf-muted)] truncate">{row.detail}</div>
              </div>
              <div className="shrink-0">
                {row.attached ? (
                  <button
                    type="button"
                    disabled={!canWrite || row.busy}
                    onClick={row.onDetach}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium bg-red-600/20 text-red-600 hover:bg-red-600/30 disabled:opacity-50"
                  >
                    {row.busy ? '…' : 'Detach'}
                  </button>
                ) : row.attachedElsewhere ? (
                  <span className="text-xs text-[var(--zf-muted)]">In use</span>
                ) : (
                  <button
                    type="button"
                    disabled={!canWrite || row.busy}
                    onClick={row.onAttach}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium bg-[var(--zf-link)]/20 text-[var(--zf-link)] hover:bg-[var(--zf-link)]/30 disabled:opacity-50"
                  >
                    {row.busy ? '…' : 'Attach'}
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
