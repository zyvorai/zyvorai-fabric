// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPost, apiPostVoid, apiPutVoid, apiDelete } from './client'

export interface PortForwardSpec {
  host_port: number
  guest_port: number
  protocol: 'tcp' | 'udp'
}

export interface VM {
  name: string
  state: 'running' | 'stopped' | 'paused' | 'starting' | 'stopping' | 'failed' | 'unknown'
  cpus: number
  memory: number
  image: string
  ip?: string
  pid?: number
  tags?: string[]
  port_forwards?: PortForwardSpec[]
  network_tap?: boolean
  network_static_ip?: boolean
  direct_uplink?: string
  direct_mode?: string
  direct_guest_ips?: string[]
  /** Why the VM ended up in the 'failed' state, if it did. Cleared on the next successful start. */
  last_error?: string
}

export interface CreateVMRequest {
  name: string
  image: string
  cpus: number
  memory: number
  /** Disk size in GB (daemon default: 20). */
  disk?: number
  /**
   * Host-port -> guest-port forwards for this VM's usermode networking
   * (e.g. exposing guest port 22 for SSH). Only applied on the VM's next
   * (re)creation in FluxVM -- usermode/slirp networking has no way to
   * add a forward to an already-running instance.
   */
  port_forwards?: PortForwardSpec[]
  /**
   * Use bridged (tap + private network namespace + DHCP) networking
   * instead of the default NAT networking. A bridged VM gets a real,
   * externally-reachable IP (visible on its Network tab) instead of
   * needing explicit port_forwards.
   */
  network_tap?: boolean
  /**
   * Only meaningful when network_tap is set: configure the guest's address
   * statically via cloud-init instead of relying on its own DHCP client
   * (not every image runs one automatically on boot).
   */
  network_static_ip?: boolean
  /**
   * Host NIC for a bridge-less FluxVM tap (`l2-uplink`). Mutually exclusive
   * with `network_tap` and port forwards. Applied on first start.
   */
  direct_uplink?: string
  /** `l2-uplink` (default) or `peer-veth`. */
  direct_mode?: string
  /** Guest IPv4s for `l2-uplink` ARP steering. At most 8. */
  direct_guest_ips?: string[]
  /** Optional tenant id — stored as `labels.tenant` and passed to FluxVM. */
  tenant?: string
  labels?: Record<string, string>
}

export interface VMMetrics {
  cpu_usage: number
  memory_usage: number
  disk_usage: number
  network_rx: number
  network_tx: number
}

const API_BASE = '/api'

export async function listVMs(): Promise<VM[]> {
  const data = await apiGet<VM[] | { items: VM[] }>(`${API_BASE}/vms`)
  return Array.isArray(data) ? data : data.items || []
}

export async function getVM(name: string): Promise<VM> {
  return apiGet<VM>(`${API_BASE}/vms/${name}`)
}

export async function createVM(req: CreateVMRequest): Promise<VM> {
  return apiPost<VM>(`${API_BASE}/vms`, req)
}

export async function deleteVM(name: string): Promise<void> {
  return apiDelete(`${API_BASE}/vms/${name}`)
}

export async function startVM(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/vms/${name}/start`)
}

export async function stopVM(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/vms/${name}/stop`)
}

export async function restartVM(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/vms/${name}/restart`)
}

export async function pauseVM(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/vms/${name}/pause`)
}

export async function resumeVM(name: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/vms/${name}/resume`)
}

export async function getMetrics(name: string): Promise<VMMetrics> {
  return apiGet<VMMetrics>(`${API_BASE}/vms/${name}/metrics`)
}

export interface VMLogEntry {
  timestamp: string
  hostname: string
  unit: string
  message: string
  priority: string
}

export interface VMLogResponse {
  entries: VMLogEntry[]
  count: number
}

/** Real boot/runtime console output for this VM (FluxVM's captured
    console log), not the audit trail of API actions taken against it. */
export async function getVMLogs(name: string, opts?: { lines?: number; grep?: string }): Promise<VMLogResponse> {
  const params = new URLSearchParams()
  if (opts?.lines) params.set('lines', String(opts.lines))
  if (opts?.grep) params.set('grep', opts.grep)
  const qs = params.toString()
  return apiGet<VMLogResponse>(`${API_BASE}/vms/${name}/logs${qs ? `?${qs}` : ''}`)
}

/**
 * Expose a guest port on this VM's usermode networking. If the VM is
 * currently running, the backend destroys and relaunches it (usermode
 * networking can't add a forward live) -- expect this to take a few
 * seconds and briefly show the VM as starting again.
 */
export async function addPortForward(
  name: string,
  forward: { hostPort: number; guestPort: number; protocol?: 'tcp' | 'udp' },
): Promise<VM> {
  return apiPost<VM>(`${API_BASE}/vms/${name}/port-forwards`, {
    host_port: forward.hostPort,
    guest_port: forward.guestPort,
    protocol: forward.protocol ?? 'tcp',
  })
}

/**
 * Stop exposing a previously-forwarded guest port. Same restart-on-running-VM
 * caveat as addPortForward -- usermode networking can't drop a forward live.
 */
export async function removePortForward(name: string, hostPort: number): Promise<void> {
  return apiDelete(`${API_BASE}/vms/${name}/port-forwards/${hostPort}`)
}

export async function cloneVM(sourceName: string, targetName: string, options?: {
  includeSnapshots?: boolean
  linkedClone?: boolean
}): Promise<void> {
  return apiPostVoid(`${API_BASE}/vms/${sourceName}/clone`, {
    target_name: targetName,
    include_snapshots: options?.includeSnapshots ?? false,
    linked_clone: options?.linkedClone ?? false,
  })
}

// Tag Management
export async function addTag(vmName: string, tag: string): Promise<void> {
  return apiPostVoid(`${API_BASE}/vms/${vmName}/tags`, { tag })
}

export async function removeTag(vmName: string, tag: string): Promise<void> {
  return apiDelete(`${API_BASE}/vms/${vmName}/tags/${tag}`)
}

export async function updateTags(vmName: string, tags: string[]): Promise<void> {
  return apiPutVoid(`${API_BASE}/vms/${vmName}/tags`, { tags })
}
