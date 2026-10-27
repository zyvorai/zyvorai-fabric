// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router'
import { createVM, listVMs } from '../api/vm'
import type { PortForwardSpec, VM } from '../api/vm'
import { listImages, createImageFromVm, getConvertJob, listCloudImages, downloadCloudImage, listDownloads } from '../api/images'
import type { ImageInfo, CloudImage } from '../api/images'
import { applyCreateAdvancedOptions, AdvancedOptionsError } from '../utils/applyCreateAdvancedOptions'
import { formatBytes } from '../utils/format'
import {
  ArrowLeft, ArrowRight, Cpu, HardDrive, ChevronDown, ChevronUp, Shield, Monitor, Plus, X,
  Network, Server, Sparkles, Check, Download,
} from 'lucide-react'
import WizardStepper from '../components/WizardStepper'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader } from '../components/ui/PageHeader'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useToastContext } from '../contexts/ToastContext'

interface AdvancedOptions {
  firmware: 'bios' | 'uefi'
  secureBoot: boolean
  cpuMode: 'host-passthrough' | 'host-model' | 'custom'
  machineType: string
  displayType: 'vnc' | 'spice'
  bootOrder: string[]
  enableBalloon: boolean
}

const defaultAdvanced: AdvancedOptions = {
  firmware: 'uefi',
  secureBoot: false,
  cpuMode: 'host-passthrough',
  machineType: 'q35',
  displayType: 'vnc',
  bootOrder: ['hd', 'cdrom', 'network'],
  enableBalloon: true,
}

const WIZARD_STEPS = ['Basics', 'Resources', 'Review'] as const
const VM_NAME_REGEX = /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$/

export default function CreateVM() {
  const navigate = useNavigate()
  const toast = useToastContext()
  const [name, setName] = useState('')
  const [image, setImage] = useState('')
  const [cpus, setCpus] = useState(2)
  const [memory, setMemory] = useState(2048)
  const [diskGb, setDiskGb] = useState(20)
  const [imageOptions, setImageOptions] = useState<ImageInfo[]>([])
  const [imagesLoading, setImagesLoading] = useState(false)
  const [loading, setLoading] = useState(false)
  const [validationError, setValidationError] = useState('')
  const [submitError, setSubmitError] = useState<string | null>(null)
  const [imagesError, setImagesError] = useState<string | null>(null)
  const [showAdvanced, setShowAdvanced] = useState(false)
  const [advanced, setAdvanced] = useState<AdvancedOptions>(defaultAdvanced)
  const [wizardStep, setWizardStep] = useState(0)
  const [imagesReload, setImagesReload] = useState(0)
  const [showGoldenImage, setShowGoldenImage] = useState(false)
  const [showDownloadImage, setShowDownloadImage] = useState(false)
  const [networkMode, setNetworkMode] = useState<'nat' | 'bridged' | 'direct'>('nat')
  const [staticIp, setStaticIp] = useState(false)
  const [directUplink, setDirectUplink] = useState('')
  const [directGuestIps, setDirectGuestIps] = useState('')
  const [tenant, setTenant] = useState('')
  const [portForwards, setPortForwards] = useState<{ hostPort: string; guestPort: string; protocol: 'tcp' | 'udp' }[]>([])

  const addPortForwardRow = (guestPort = '', hostPort = '') => {
    setPortForwards((rows) => [...rows, { hostPort, guestPort, protocol: 'tcp' }])
  }
  const updatePortForwardRow = (index: number, patch: Partial<{ hostPort: string; guestPort: string; protocol: 'tcp' | 'udp' }>) => {
    setPortForwards((rows) => rows.map((r, i) => (i === index ? { ...r, ...patch } : r)))
  }
  const removePortForwardRow = (index: number) => {
    setPortForwards((rows) => rows.filter((_, i) => i !== index))
  }

  useEffect(() => {
    if (wizardStep !== 0) return
    void imagesReload
    let cancelled = false
    setImagesLoading(true)
    setImagesError(null)
    listImages()
      .then((data) => {
        if (cancelled) return
        setImageOptions((Array.isArray(data) ? data : []).filter((i) => i.path))
      })
      .catch((err) => {
        if (!cancelled) {
          setImageOptions([])
          const msg = formatUserError(err)
          setImagesError(msg)
          toastFailure(toast, 'Could not load disk images', err)
        }
      })
      .finally(() => {
        if (!cancelled) setImagesLoading(false)
      })
    return () => { cancelled = true }
  }, [wizardStep, imagesReload, toast])

  const memoryPresets = [
    { label: '512 MB', value: 512 },
    { label: '1 GB', value: 1024 },
    { label: '2 GB', value: 2048 },
    { label: '4 GB', value: 4096 },
    { label: '8 GB', value: 8192 },
    { label: '16 GB', value: 16384 },
  ]

  const validateStep = (step: number): string | null => {
    if (step === 0) {
      if (!name.trim()) return 'VM name is required'
      if (!VM_NAME_REGEX.test(name)) {
        return 'VM name must start with a letter or number, use only letters, numbers, dots, hyphens, underscores, and be 1–64 characters'
      }
      if (!image.trim()) return 'Image path is required'
    }
    if (step === 1) {
      if (cpus < 1 || cpus > 32) return 'vCPUs must be between 1 and 32'
      if (memory < 256) return 'Memory must be at least 256 MB'
      if (networkMode === 'direct') {
        const nic = directUplink.trim()
        if (!nic) return 'Direct uplink needs the host NIC name (for example enp1s0)'
        if (nic.length > 15 || nic.includes('/') || nic.includes(' ')) {
          return 'Uplink NIC must be an interface name (1–15 characters, no slash or space)'
        }
        const ips = directGuestIps.split(',').map((s) => s.trim()).filter(Boolean)
        if (ips.length > 8) return 'At most 8 guest IPv4 addresses'
        for (const ip of ips) {
          if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(ip)) return `Guest IP '${ip}' is not an IPv4 address`
        }
      }
      const hostPorts = new Set<string>()
      for (const row of networkMode === 'nat' ? portForwards : []) {
        const h = parseInt(row.hostPort)
        const g = parseInt(row.guestPort)
        if (!row.hostPort || !Number.isInteger(h) || h < 1 || h > 65535) {
          return 'Each port forward needs a host port between 1 and 65535'
        }
        if (!row.guestPort || !Number.isInteger(g) || g < 1 || g > 65535) {
          return 'Each port forward needs a guest port between 1 and 65535'
        }
        if (hostPorts.has(row.hostPort)) {
          return `Host port ${row.hostPort} is used by more than one port forward`
        }
        hostPorts.add(row.hostPort)
      }
    }
    return null
  }

  const goNext = () => {
    const msg = validateStep(wizardStep)
    if (msg) {
      setValidationError(msg)
      return
    }
    setValidationError('')
    setWizardStep((s) => Math.min(s + 1, WIZARD_STEPS.length - 1))
  }

  const goBack = () => {
    setValidationError('')
    setWizardStep((s) => Math.max(s - 1, 0))
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const msg = validateStep(0) || validateStep(1)
    if (msg) {
      setValidationError(msg)
      return
    }

    setLoading(true)
    setValidationError('')
    setSubmitError(null)

    try {
      const port_forwards: PortForwardSpec[] = networkMode === 'nat'
        ? portForwards.map((row) => ({
            host_port: parseInt(row.hostPort),
            guest_port: parseInt(row.guestPort),
            protocol: row.protocol,
          }))
        : []
      const guestIps = networkMode === 'direct'
        ? directGuestIps.split(',').map((s) => s.trim()).filter(Boolean)
        : []
      await createVM({
        name, image, cpus, memory, disk: diskGb,
        network_tap: networkMode === 'bridged',
        network_static_ip: networkMode === 'bridged' && staticIp,
        ...(networkMode === 'direct' ? { direct_uplink: directUplink.trim() } : {}),
        ...(guestIps.length ? { direct_guest_ips: guestIps } : {}),
        ...(tenant.trim() ? { tenant: tenant.trim() } : {}),
        ...(port_forwards.length ? { port_forwards } : {}),
      })
      if (showAdvanced) {
        try {
          await applyCreateAdvancedOptions(name, advanced)
        } catch (advErr) {
          const failed = advErr instanceof AdvancedOptionsError
            ? advErr.failures.map((f) => f.option).join(', ')
            : 'some options'
          toastFailure(toast, `VM created, but ${failed} could not be applied`, advErr)
          navigate(`/app/vms/${name}`)
          return
        }
      }
      toast.success(`VM '${name}' created`)
      navigate(`/app/vms/${name}`)
    } catch (err) {
      const msg = formatUserError(err)
      setSubmitError(msg)
      toastFailure(toast, 'Failed to create VM', err)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div>
      <button
        onClick={() => navigate('/app/vms')}
        className="flex items-center gap-2 mb-6 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors text-sm"
      >
        <ArrowLeft className="w-4 h-4" />
        Back to VMs
      </button>

      <div className="max-w-2xl">
        <PageHeader
          title="Create Virtual Machine"
          description="Configure and launch a new VM"
          icon={Server}
        />

        <WizardStepper
          steps={WIZARD_STEPS}
          current={wizardStep}
          onStep={(step) => {
            if (step < wizardStep) {
              setWizardStep(step)
              setValidationError('')
            }
          }}
        />

        {imagesError && wizardStep === 0 && (
          <ErrorBanner
            title="Could not load disk images"
            headline={imagesError}
            hints={hintsForError(imagesError, 'storage')}
            onRetry={() => setImagesReload((n) => n + 1)}
          />
        )}

        {submitError && (
          <ErrorBanner
            title="Could not create virtual machine"
            headline={submitError}
            hints={hintsForError(submitError, 'vm')}
          />
        )}

        <form onSubmit={handleSubmit} className="space-y-4 mt-4">
          {validationError && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
              {validationError}
            </div>
          )}

          {wizardStep === 0 && (
            <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-6 space-y-5">
              <h2 className="text-sm font-medium text-[var(--zf-muted)] uppercase tracking-wider">Basic Configuration</h2>

              <div>
                <label htmlFor="vm-name" className="block text-sm font-medium text-[var(--zf-ink)] mb-1.5">
                  VM Name
                </label>
                <input
                  id="vm-name"
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="my-virtual-machine"
                  className="w-full px-3.5 py-2.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-[var(--zf-link)]/50 focus:ring-1 focus:ring-[var(--zf-link)]/20 transition-colors text-sm"
                  required
                  autoFocus
                />
              </div>

              <div>
                <label htmlFor="vm-tenant" className="block text-sm font-medium text-[var(--zf-ink)] mb-1.5">
                  Tenant <span className="text-[var(--zf-muted)] font-normal">(optional)</span>
                </label>
                <input
                  id="vm-tenant"
                  type="text"
                  value={tenant}
                  onChange={(e) => setTenant(e.target.value)}
                  placeholder="acme"
                  className="w-full px-3.5 py-2.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-[var(--zf-link)]/50 focus:ring-1 focus:ring-[var(--zf-link)]/20 transition-colors text-sm"
                />
              </div>

            <div>
              <div className="flex items-center justify-between mb-1.5 gap-3 flex-wrap">
                <label htmlFor="vm-image" className="block text-sm font-medium text-[var(--zf-ink)]">
                  Disk image
                </label>
                <div className="flex items-center gap-3 flex-wrap">
                  <button
                    type="button"
                    onClick={() => setShowDownloadImage(true)}
                    className="flex items-center gap-1.5 text-xs font-medium text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] transition-colors shrink-0"
                  >
                    <Download className="w-3.5 h-3.5" />
                    Download an OS image
                  </button>
                  <button
                    type="button"
                    onClick={() => setShowGoldenImage(true)}
                    className="flex items-center gap-1.5 text-xs font-medium text-[var(--zf-link)] hover:text-[var(--zf-link-hover)] transition-colors shrink-0"
                  >
                    <Sparkles className="w-3.5 h-3.5" />
                    Create golden image from a VM
                  </button>
                </div>
              </div>

              {imagesLoading ? (
                <div className="grid grid-cols-2 sm:grid-cols-3 gap-2 mb-2">
                  {[0, 1, 2].map((i) => (
                    <div key={i} className="h-[4.25rem] rounded-lg bg-[var(--zf-canvas)] animate-pulse" />
                  ))}
                </div>
              ) : imageOptions.length > 0 ? (
                <div className="grid grid-cols-2 sm:grid-cols-3 gap-2 mb-2">
                  {imageOptions.map((opt) => {
                    const selected = image === opt.path
                    return (
                      <button
                        key={opt.path}
                        type="button"
                        onClick={() => setImage(opt.path)}
                        className={`relative text-left p-3 rounded-lg border transition-colors ${
                          selected
                            ? 'bg-[var(--zf-link)]/15 border-[var(--zf-link)]/40 ring-1 ring-[var(--zf-link)]/30'
                            : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)] hover:border-[var(--zf-hairline)]'
                        }`}
                      >
                        {selected && (
                          <div className="absolute top-2 right-2 w-4 h-4 rounded-full bg-[var(--zf-link)] flex items-center justify-center">
                            <Check className="w-2.5 h-2.5 text-[var(--zf-ink)]" />
                          </div>
                        )}
                        <div className="flex items-center gap-2 mb-1.5">
                          <div className="icon-tile icon-tile-sm icon-tile-blue shrink-0">
                            <HardDrive className="w-3.5 h-3.5" />
                          </div>
                          <span className="text-sm font-medium text-[var(--zf-ink)] truncate">{opt.name}</span>
                        </div>
                        <p className="text-xs text-[var(--zf-muted)]">
                          {opt.format.toUpperCase()} · {formatBytes(opt.size_bytes)}
                        </p>
                      </button>
                    )
                  })}
                </div>
              ) : (
                <p className="text-xs text-[var(--zf-muted)] mb-2">
                  No catalog images found — enter a path below, or create a golden image from an existing VM.
                </p>
              )}

              <input
                id="vm-image"
                type="text"
                value={image}
                onChange={(e) => setImage(e.target.value)}
                placeholder="Or enter a path on the host, e.g. /var/lib/zyvor-fabricd/images/ubuntu-24.04.qcow2"
                className="w-full px-3.5 py-2.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-[var(--zf-link)]/50 focus:ring-1 focus:ring-[var(--zf-link)]/20 transition-colors text-sm font-mono"
                required
              />
            </div>
            </div>
          )}

          {showGoldenImage && (
            <GoldenImageModal
              onClose={() => setShowGoldenImage(false)}
              onCreated={(path) => {
                setImage(path)
                setShowGoldenImage(false)
                setImagesReload((n) => n + 1)
              }}
            />
          )}

          {showDownloadImage && (
            <DownloadImageModal
              existingImages={imageOptions}
              onClose={() => setShowDownloadImage(false)}
              onDownloaded={(path) => {
                setImage(path)
                setShowDownloadImage(false)
                setImagesReload((n) => n + 1)
              }}
            />
          )}

          {wizardStep === 1 && (
            <>
              <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-6 space-y-5">
                <h2 className="text-sm font-medium text-[var(--zf-muted)] uppercase tracking-wider">Resources</h2>

                <div>
                  <label htmlFor="vm-cpus" className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-2">
                    <Cpu className="w-4 h-4 text-[var(--zf-muted)]" />
                    vCPUs
                  </label>
                  <div className="flex items-center gap-3">
                    <input
                      id="vm-cpus"
                      type="range"
                      min={1}
                      max={32}
                      value={cpus}
                      onChange={(e) => setCpus(parseInt(e.target.value))}
                      className="flex-1 accent-[var(--zf-link)]"
                    />
                    <div className="w-16 text-center">
                      <input
                        type="number"
                        value={cpus}
                        onChange={(e) => setCpus(Math.max(1, Math.min(32, parseInt(e.target.value) || 1)))}
                        min={1}
                        max={32}
                        className="w-full px-2 py-1.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-md text-center text-sm text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]/50"
                      />
                    </div>
                  </div>
                </div>

                <div>
                  <label className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-2">
                    <HardDrive className="w-4 h-4 text-[var(--zf-muted)]" />
                    Memory
                  </label>
                  <div className="grid grid-cols-3 sm:grid-cols-6 gap-2 mb-3">
                    {memoryPresets.map((preset) => (
                      <button
                        key={preset.value}
                        type="button"
                        onClick={() => setMemory(preset.value)}
                        className={`px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                          memory === preset.value
                            ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30'
                            : 'bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:border-[var(--zf-hairline)]'
                        }`}
                      >
                        {preset.label}
                      </button>
                    ))}
                  </div>
                  <div className="flex items-center gap-2">
                    <input
                      id="vm-memory"
                      type="number"
                      value={memory}
                      onChange={(e) => setMemory(parseInt(e.target.value) || 512)}
                      min={256}
                      step={256}
                      className="w-28 px-3 py-1.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-md text-sm text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]/50"
                    />
                    <span className="text-sm text-[var(--zf-muted)]">MB</span>
                    <span className="text-sm text-[var(--zf-muted)] ml-2">
                      ({(memory / 1024).toFixed(1)} GB)
                    </span>
                  </div>
                </div>
              </div>

              <div>
              <label htmlFor="vm-disk" className="block text-sm font-medium text-[var(--zf-ink)] mb-1.5">
                Root disk (GB)
              </label>
              <input
                id="vm-disk"
                type="number"
                min={1}
                max={2048}
                value={diskGb}
                onChange={(e) => setDiskGb(Math.max(1, parseInt(e.target.value) || 20))}
                className="w-28 px-3 py-1.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-md text-sm text-[var(--zf-ink)]"
              />
            </div>

              <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
            <button
              type="button"
              onClick={() => setShowAdvanced(!showAdvanced)}
              className="w-full flex items-center justify-between px-6 py-4 text-sm font-medium text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors"
            >
              <span className="uppercase tracking-wider">Advanced Options</span>
                  {showAdvanced ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
                </button>

            {showAdvanced && (
              <div className="px-6 pb-6 space-y-5 border-t border-[var(--zf-hairline)] pt-5">
                <p className="text-xs text-[var(--zf-muted)] bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2">
                  Applied after the VM is created (boot, display, CPU mode, and UEFI settings).
                </p>
                <div>
                      <label className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-2">
                        <Shield className="w-4 h-4 text-[var(--zf-muted)]" />
                        Firmware
                      </label>
                      <div className="flex gap-2">
                        {(['bios', 'uefi'] as const).map((fw) => (
                          <button
                            key={fw}
                            type="button"
                            onClick={() =>
                              setAdvanced({
                                ...advanced,
                                firmware: fw,
                                secureBoot: fw === 'bios' ? false : advanced.secureBoot,
                              })
                            }
                            className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                              advanced.firmware === fw
                                ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30'
                                : 'bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
                            }`}
                          >
                            {fw.toUpperCase()}
                          </button>
                        ))}
                      </div>
                    </div>

                    <div>
                      <label className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-2">
                        <Monitor className="w-4 h-4 text-[var(--zf-muted)]" />
                        Display Protocol
                      </label>
                      <div className="flex gap-2">
                        {(['vnc', 'spice'] as const).map((dt) => (
                          <button
                            key={dt}
                            type="button"
                            onClick={() => setAdvanced({ ...advanced, displayType: dt })}
                            className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                              advanced.displayType === dt
                                ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30'
                                : 'bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
                            }`}
                          >
                            {dt.toUpperCase()}
                          </button>
                        ))}
                      </div>
                    </div>

                    <div>
                      <label className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-2">
                        <Network className="w-4 h-4 text-[var(--zf-muted)]" />
                        Networking
                      </label>
                      <div className="flex gap-2">
                        <button
                          type="button"
                          onClick={() => setNetworkMode('nat')}
                          className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                            networkMode === 'nat'
                              ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30'
                              : 'bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
                          }`}
                        >
                          NAT (default)
                        </button>
                        <button
                          type="button"
                          onClick={() => setNetworkMode('bridged')}
                          className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                            networkMode === 'bridged'
                              ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30'
                              : 'bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
                          }`}
                        >
                          Bridged (DHCP)
                        </button>
                        <button
                          type="button"
                          onClick={() => setNetworkMode('direct')}
                          className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                            networkMode === 'direct'
                              ? 'bg-[var(--zf-link)]/20 text-[var(--zf-link)] border border-[var(--zf-link)]/30'
                              : 'bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
                          }`}
                        >
                          Direct uplink
                        </button>
                      </div>
                      <p className="text-xs text-[var(--zf-muted)] mt-2">
                        {networkMode === 'nat'
                          ? 'No host-routable IP — reach anything inside this VM (like SSH) by forwarding a port below.'
                          : networkMode === 'direct'
                            ? 'Bridge-less tap on a host NIC. Service Fabric is not attached to direct taps.'
                            : 'This VM gets its own real IP (visible on its Network tab once booted) — no port forwards needed.'}
                      </p>
                      {networkMode === 'direct' && (
                        <div className="mt-3 space-y-2">
                          <input
                            type="text"
                            value={directUplink}
                            onChange={(e) => setDirectUplink(e.target.value)}
                            placeholder="Uplink NIC (e.g. enp1s0)"
                            className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)]"
                          />
                          <input
                            type="text"
                            value={directGuestIps}
                            onChange={(e) => setDirectGuestIps(e.target.value)}
                            placeholder="Guest IPv4s, optional (comma-separated, up to 8)"
                            className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg px-3 py-2 text-sm text-[var(--zf-ink)]"
                          />
                        </div>
                      )}
                      {networkMode === 'bridged' && (
                        <label className="flex items-center gap-2 mt-3 text-sm text-[var(--zf-ink)]">
                          <input
                            type="checkbox"
                            checked={staticIp}
                            onChange={(e) => setStaticIp(e.target.checked)}
                            className="rounded border-[var(--zf-hairline)] bg-[var(--zf-surface)] text-[var(--zf-link)] focus:ring-[var(--zf-link)]/50"
                          />
                          Assign the IP statically via cloud-init
                        </label>
                      )}
                      <p className="text-xs text-[var(--zf-muted)] mt-1">
                        {networkMode === 'bridged' && staticIp
                          ? 'The address is configured directly at boot — no dependency on the guest running a working DHCP client, but the image must support cloud-init.'
                          : networkMode === 'bridged'
                            ? "The guest's own DHCP client requests the address — works without cloud-init, but only if the image actually runs one automatically on boot."
                            : ''}
                      </p>
                    </div>

                    {networkMode === 'nat' && (
                    <div>
                      <label className="flex items-center gap-2 text-sm font-medium text-[var(--zf-ink)] mb-2">
                        <Network className="w-4 h-4 text-[var(--zf-muted)]" />
                        Expose ports (host port → guest port)
                      </label>
                      <p className="text-xs text-[var(--zf-muted)] mb-2">
                        This VM uses NAT networking with no host-routable IP — a port must be forwarded here to
                        reach anything inside it (like SSH) from outside the host.
                      </p>
                      {portForwards.length === 0 && (
                        <button
                          type="button"
                          onClick={() => addPortForwardRow('22')}
                          className="mb-2 px-3 py-1.5 rounded-lg text-xs font-medium bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-ink)] hover:text-[var(--zf-ink)] hover:border-[var(--zf-hairline)] transition-colors"
                        >
                          + Expose SSH (22)
                        </button>
                      )}
                      <div className="space-y-2">
                        {portForwards.map((row, i) => (
                          <div key={i} className="flex items-center gap-2">
                            <input
                              type="number"
                              value={row.hostPort}
                              onChange={(e) => updatePortForwardRow(i, { hostPort: e.target.value })}
                              placeholder="Host port"
                              min={1}
                              max={65535}
                              className="w-28 px-2.5 py-1.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-md text-sm text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]/50"
                            />
                            <span className="text-[var(--zf-muted)] text-sm">→</span>
                            <input
                              type="number"
                              value={row.guestPort}
                              onChange={(e) => updatePortForwardRow(i, { guestPort: e.target.value })}
                              placeholder="Guest port"
                              min={1}
                              max={65535}
                              className="w-28 px-2.5 py-1.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-md text-sm text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]/50"
                            />
                            <select
                              value={row.protocol}
                              onChange={(e) => updatePortForwardRow(i, { protocol: e.target.value as 'tcp' | 'udp' })}
                              className="px-2 py-1.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-md text-sm text-[var(--zf-ink)]"
                            >
                              <option value="tcp">TCP</option>
                              <option value="udp">UDP</option>
                            </select>
                            <button
                              type="button"
                              onClick={() => removePortForwardRow(i)}
                              className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-red-600 hover:bg-red-50 transition-colors"
                              title="Remove"
                            >
                              <X className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        ))}
                      </div>
                      <button
                        type="button"
                        onClick={() => addPortForwardRow()}
                        className="mt-2 flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-[var(--zf-surface)] border border-[var(--zf-hairline)] text-[var(--zf-ink)] hover:text-[var(--zf-ink)] hover:border-[var(--zf-hairline)] transition-colors"
                      >
                        <Plus className="w-3.5 h-3.5" />
                        Add port forward
                      </button>
                    </div>
                    )}
                  </div>
                )}
              </div>
            </>
          )}

          {wizardStep === 2 && (
            <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] p-6 space-y-4">
              <h2 className="text-sm font-medium text-[var(--zf-muted)] uppercase tracking-wider">Review</h2>
              <dl className="grid grid-cols-1 gap-3 text-sm">
                <div className="flex justify-between gap-4 border-b border-[var(--zf-hairline)] pb-2">
                  <dt className="text-[var(--zf-muted)]">Name</dt>
                  <dd className="text-[var(--zf-ink)] font-medium">{name}</dd>
                </div>
                {tenant.trim() && (
                  <div className="flex justify-between gap-4 border-b border-[var(--zf-hairline)] pb-2">
                    <dt className="text-[var(--zf-muted)]">Tenant</dt>
                    <dd className="text-[var(--zf-ink)] font-medium">{tenant.trim()}</dd>
                  </div>
                )}
                <div className="flex justify-between gap-4 border-b border-[var(--zf-hairline)] pb-2">
                  <dt className="text-[var(--zf-muted)]">Image</dt>
                  <dd className="text-[var(--zf-ink)] font-mono text-xs text-right break-all">{image}</dd>
                </div>
                <div className="flex justify-between gap-4 border-b border-[var(--zf-hairline)] pb-2">
                  <dt className="text-[var(--zf-muted)]">vCPUs</dt>
                  <dd className="text-[var(--zf-ink)]">{cpus}</dd>
                </div>
                <div className="flex justify-between gap-4 border-b border-[var(--zf-hairline)] pb-2">
                  <dt className="text-[var(--zf-muted)]">Memory</dt>
                  <dd className="text-[var(--zf-ink)]">
                    {memory} MB ({(memory / 1024).toFixed(1)} GB)
                  </dd>
                </div>
                <div className="flex justify-between gap-4 border-b border-[var(--zf-hairline)] pb-2">
                  <dt className="text-[var(--zf-muted)]">Root disk</dt>
                  <dd className="text-[var(--zf-ink)]">{diskGb} GB</dd>
                </div>
                <div className={`flex justify-between gap-4 ${networkMode === 'nat' && portForwards.length ? 'border-b border-[var(--zf-hairline)] pb-2' : ''}`}>
                  <dt className="text-[var(--zf-muted)]">Networking</dt>
                  <dd className="text-[var(--zf-ink)]">
                    {networkMode === 'nat' ? 'NAT' : networkMode === 'direct' ? `Direct (${directUplink.trim() || 'uplink'})` : staticIp ? 'Bridged (static IP)' : 'Bridged (DHCP)'}
                  </dd>
                </div>
                {networkMode === 'nat' && portForwards.length > 0 && (
                  <div className="flex justify-between gap-4">
                    <dt className="text-[var(--zf-muted)]">Exposed ports</dt>
                    <dd className="text-[var(--zf-ink)] text-right">
                      {portForwards.map((row, i) => (
                        <div key={i} className="font-mono text-xs">
                          {row.hostPort} → {row.guestPort}/{row.protocol}
                        </div>
                      ))}
                    </dd>
                  </div>
                )}
              </dl>
            </div>
          )}

          <div className="flex gap-3 pt-2">
            {wizardStep > 0 && (
              <button
                type="button"
                onClick={goBack}
                className="flex-1 zf-btn zf-btn-ghost"
              >
                <ArrowLeft className="w-4 h-4" />
                Back
              </button>
            )}
            {wizardStep < WIZARD_STEPS.length - 1 ? (
              <button
                type="button"
                onClick={goNext}
                className="flex-1 zf-btn zf-btn-primary"
              >
                Next
                <ArrowRight className="w-4 h-4" />
              </button>
            ) : (
              <button
                type="submit"
                disabled={loading || !name || !image}
                className="flex-1 zf-btn zf-btn-primary"
              >
                {loading ? (
                  <>
                    <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                    Creating...
                  </>
                ) : (
                  'Create Virtual Machine'
                )}
              </button>
            )}
          </div>
        </form>
      </div>
    </div>
  )
}

function GoldenImageModal({ onClose, onCreated }: { onClose: () => void; onCreated: (path: string) => void }) {
  const toast = useToastContext()
  const [vms, setVms] = useState<VM[]>([])
  const [vmsLoading, setVmsLoading] = useState(true)
  const [vmName, setVmName] = useState('')
  const [name, setName] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [progress, setProgress] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    listVMs()
      .then((data) => {
        if (cancelled) return
        setVms(data)
        setVmName((prev) => prev || data[0]?.name || '')
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setVmsLoading(false)
      })
    return () => { cancelled = true }
  }, [])

  const handleSubmit = async () => {
    if (!VM_NAME_REGEX.test(name)) {
      setError('Image name must start with a letter or number, use only letters, numbers, dots, hyphens, underscores, and be 1–64 characters')
      return
    }
    setError(null)
    setSubmitting(true)
    setProgress(0)
    try {
      const { job_id } = await createImageFromVm(vmName, name)
      // Poll until the qemu-img convert job (backend: /images/from-vm/:name)
      // reaches a terminal state -- disk conversion can take a while for
      // large or busy VMs, so this isn't a fire-and-forget POST.
      let settledTicks = 0
      for (;;) {
        await new Promise((r) => setTimeout(r, 1200))
        const job = await getConvertJob(job_id)
        setProgress(job.progress)
        if (job.status === 'completed') {
          // The backend certifies the image with GuestKit's offline `doctor`
          // scoring right after conversion finishes, as a short follow-up
          // step -- give it a few extra ticks to attach before finalizing,
          // but don't block forever (e.g. guestkit missing in dev).
          if (job.boot_score === undefined && settledTicks < 4) {
            settledTicks += 1
            continue
          }
          const scoreNote = job.boot_score !== undefined
            ? ` — boot readiness ${Math.round(job.boot_score)}/100`
            : ''
          toast.success(`Golden image '${name}' created from '${vmName}'${scoreNote}`)
          onCreated(job.output_path)
          return
        }
        if (job.status === 'failed') {
          setError(job.error || 'Image conversion failed')
          setSubmitting(false)
          setProgress(null)
          return
        }
      }
    } catch (err) {
      toastFailure(toast, 'Failed to start golden image build', err)
      setError(formatUserError(err))
      setSubmitting(false)
      setProgress(null)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)] w-full max-w-md">
        <div className="flex items-center justify-between p-6 border-b border-[var(--zf-hairline)]">
          <div className="flex items-center gap-3">
            <div className="icon-tile icon-tile-md icon-tile-purple">
              <Sparkles className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-lg font-bold text-[var(--zf-ink)]">Create Golden Image</h2>
              <p className="text-xs text-[var(--zf-muted)]">Materialize a VM's current disk as a reusable catalog image</p>
            </div>
          </div>
          {!submitting && (
            <button type="button" onClick={onClose} className="p-2 hover:bg-white/[0.03] rounded transition text-[var(--zf-muted)] hover:text-[var(--zf-ink)]">
              <X className="w-4 h-4" />
            </button>
          )}
        </div>
        {/* Not a <form> -- this dialog renders inside the wizard's own outer
            <form>, and nested <form> elements are invalid HTML (the browser
            drops the inner tag, so a type="submit" button here would submit
            the OUTER wizard form and reload the page instead). */}
        <div className="p-6 space-y-4">
          {error && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
              {error}
            </div>
          )}

          {vmsLoading ? (
            <div className="h-10 rounded-lg bg-[var(--zf-surface)] animate-pulse" />
          ) : vms.length === 0 ? (
            <p className="text-sm text-[var(--zf-muted)]">No VMs exist yet — create a VM first, then come back here to save its disk as a golden image.</p>
          ) : (
            <>
              <div>
                <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Source VM</label>
                <select
                  value={vmName}
                  onChange={(e) => setVmName(e.target.value)}
                  disabled={submitting}
                  className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] focus:outline-none focus:border-[var(--zf-link)]/50 disabled:opacity-50"
                >
                  {vms.map((v) => (
                    <option key={v.name} value={v.name}>{v.name} ({v.state})</option>
                  ))}
                </select>
                <p className="text-xs text-[var(--zf-muted)] mt-1">
                  The image is an independent copy — the source VM can change or be deleted afterward without affecting it.
                </p>
              </div>
              <div>
                <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Image Name</label>
                <input
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && name && vmName && !submitting) handleSubmit()
                  }}
                  placeholder="e.g. web-server-golden"
                  disabled={submitting}
                  className="w-full bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded-lg py-2 px-4 text-[var(--zf-ink)] font-mono text-sm focus:outline-none focus:border-[var(--zf-link)]/50 disabled:opacity-50"
                  required
                  autoFocus
                />
              </div>
              {submitting && progress !== null && (
                <div>
                  <div className="h-1.5 rounded-full bg-[var(--zf-surface)] overflow-hidden">
                    <div
                      className="h-full bg-[var(--zf-link)] transition-all duration-500"
                      style={{ width: `${Math.max(5, progress)}%` }}
                    />
                  </div>
                  <p className="text-xs text-[var(--zf-muted)] mt-1.5">Converting disk image…</p>
                </div>
              )}
              <div className="flex justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={onClose}
                  disabled={submitting}
                  className="zf-btn zf-btn-ghost"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={handleSubmit}
                  disabled={submitting || !name || !vmName}
                  className="zf-btn zf-btn-primary"
                >
                  {submitting && <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />}
                  {submitting ? 'Creating…' : 'Create Image'}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}

const DISTRO_TILE_COLOR: Record<string, 'blue' | 'green' | 'purple' | 'orange' | 'red' | 'cyan'> = {
  ubuntu: 'orange',
  fedora: 'blue',
  debian: 'red',
  almalinux: 'purple',
  flatcar: 'cyan',
}

function DownloadImageModal({ existingImages, onClose, onDownloaded }: {
  existingImages: ImageInfo[]
  onClose: () => void
  onDownloaded: (path: string) => void
}) {
  const toast = useToastContext()
  const [catalog, setCatalog] = useState<CloudImage[]>([])
  const [catalogLoading, setCatalogLoading] = useState(true)
  const [selected, setSelected] = useState<CloudImage | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [statusLabel, setStatusLabel] = useState('')
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    listCloudImages()
      .then((data) => {
        if (cancelled) return
        setCatalog(data)
        setSelected((prev) => prev || data[0] || null)
      })
      .catch((err) => {
        if (!cancelled) setError(formatUserError(err))
      })
      .finally(() => {
        if (!cancelled) setCatalogLoading(false)
      })
    return () => { cancelled = true }
  }, [])

  const handleDownload = async () => {
    if (!selected) return
    setError(null)

    // Already sitting on disk from a previous download -- use it directly
    // instead of fetching it over the network again.
    const onDisk = existingImages.find((img) => img.name === selected.name)
    if (onDisk) {
      toast.success(`'${selected.name}' is already on disk — using the existing image`)
      onDownloaded(onDisk.path)
      return
    }

    setSubmitting(true)
    setStatusLabel('Starting download…')
    try {
      const job = await downloadCloudImage(selected.name)
      for (;;) {
        await new Promise((r) => setTimeout(r, 1500))
        const downloads = await listDownloads()
        const current = downloads.find((d) => d.id === job.id)
        if (!current) continue
        if (current.state === 'completed' && current.output_path) {
          toast.success(`Downloaded '${selected.name}'`)
          onDownloaded(current.output_path)
          return
        }
        if (current.state === 'failed') {
          setError(current.error || 'Download failed')
          setSubmitting(false)
          setStatusLabel('')
          return
        }
        setStatusLabel(current.state === 'building' ? 'Downloading…' : 'Starting download…')
      }
    } catch (err) {
      toastFailure(toast, 'Failed to start download', err)
      setError(formatUserError(err))
      setSubmitting(false)
      setStatusLabel('')
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)] w-full max-w-lg">
        <div className="flex items-center justify-between p-6 border-b border-[var(--zf-hairline)]">
          <div className="flex items-center gap-3">
            <div className="icon-tile icon-tile-md icon-tile-cyan">
              <Download className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-lg font-bold text-[var(--zf-ink)]">Download an OS Image</h2>
              <p className="text-xs text-[var(--zf-muted)]">Fetch a ready-made distro image straight into the catalog</p>
            </div>
          </div>
          {!submitting && (
            <button type="button" onClick={onClose} className="p-2 hover:bg-white/[0.03] rounded transition text-[var(--zf-muted)] hover:text-[var(--zf-ink)]">
              <X className="w-4 h-4" />
            </button>
          )}
        </div>
        <div className="p-6 space-y-4">
          {error && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
              {error}
            </div>
          )}

          {catalogLoading ? (
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
              {[0, 1, 2, 3, 4, 5].map((i) => (
                <div key={i} className="h-16 rounded-lg bg-[var(--zf-surface)] animate-pulse" />
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
              {catalog.map((img) => {
                const isSelected = selected?.name === img.name
                const onDisk = existingImages.some((e) => e.name === img.name)
                return (
                  <button
                    key={img.name}
                    type="button"
                    onClick={() => setSelected(img)}
                    disabled={submitting}
                    className={`relative text-left p-3 rounded-lg border transition-colors disabled:opacity-50 ${
                      isSelected
                        ? 'bg-[var(--zf-link)]/15 border-[var(--zf-link)]/40 ring-1 ring-[var(--zf-link)]/30'
                        : 'bg-[var(--zf-surface)] border-[var(--zf-hairline)] hover:border-[var(--zf-hairline)]'
                    }`}
                  >
                    {isSelected && (
                      <div className="absolute top-2 right-2 w-4 h-4 rounded-full bg-[var(--zf-link)] flex items-center justify-center">
                        <Check className="w-2.5 h-2.5 text-[var(--zf-ink)]" />
                      </div>
                    )}
                    <div className={`icon-tile icon-tile-sm icon-tile-${DISTRO_TILE_COLOR[img.distro] || 'blue'} mb-1.5`}>
                      <Server className="w-3.5 h-3.5" />
                    </div>
                    <p className="text-sm font-medium text-[var(--zf-ink)] truncate capitalize">{img.distro}</p>
                    <p className="text-xs text-[var(--zf-muted)]">{img.version} · {img.arch}</p>
                    {onDisk && (
                      <p className="text-xs font-medium text-emerald-600 mt-1">Already on disk</p>
                    )}
                  </button>
                )
              })}
            </div>
          )}

          {submitting && (
            <div className="flex items-center gap-2 text-sm text-[var(--zf-muted)]">
              <div className="w-3.5 h-3.5 border-2 border-[var(--zf-hairline)] border-t-[var(--zf-link)] rounded-full animate-spin" />
              {statusLabel}
            </div>
          )}

          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="zf-btn zf-btn-ghost"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleDownload}
              disabled={submitting || !selected}
              className="zf-btn zf-btn-primary"
            >
              {submitting && <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />}
              {submitting
                ? 'Downloading…'
                : selected && existingImages.some((e) => e.name === selected.name)
                  ? 'Use Existing'
                  : 'Download'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
