// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { apiGet, apiPostVoid, apiDelete } from './client'
import { API_BASE_URL } from './config'

// USB Passthrough
export interface USBDevice {
  bus: number
  device: number
  vendor_id: string
  product_id: string
  vendor_name: string
  product_name: string
  speed: string
  attached_to?: string
}

export interface USBAttachRequest {
  vendor_id: string
  product_id: string
}

export async function listUSBDevices(): Promise<USBDevice[]> {
  return apiGet<USBDevice[]>(`${API_BASE_URL}/system/usb-devices`)
}

export async function attachUSB(vmName: string, req: USBAttachRequest): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/devices/usb`, req)
}

export async function detachUSB(vmName: string, vendorId: string, productId: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/vms/${vmName}/devices/usb/${vendorId}:${productId}`)
}

// PCI Passthrough
export interface PCIDevice {
  address: string
  vendor_id: string
  device_id: string
  vendor_name: string
  device_name: string
  class_name: string
  iommu_group: number
  driver: string
  numa_node: number
  attached_to?: string
}

export interface PCIAttachRequest {
  address: string
  rombar?: boolean
}

export async function listPCIDevices(): Promise<PCIDevice[]> {
  return apiGet<PCIDevice[]>(`${API_BASE_URL}/system/pci-devices`)
}

export async function attachPCI(vmName: string, req: PCIAttachRequest): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/devices/pci`, req)
}

export async function detachPCI(vmName: string, address: string): Promise<void> {
  return apiDelete(`${API_BASE_URL}/vms/${vmName}/devices/pci/${encodeURIComponent(address)}`)
}

// Display/Graphics
export interface DisplayConfig {
  type: 'vnc' | 'spice'
  listen_address: string
  port: number
  tls_port?: number
  password?: string
  keymap?: string
}

export async function getDisplay(vmName: string): Promise<DisplayConfig> {
  return apiGet<DisplayConfig>(`${API_BASE_URL}/vms/${vmName}/display`)
}

export async function updateDisplay(vmName: string, config: Partial<DisplayConfig>): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/display`, config)
}

// Boot Configuration
export interface BootConfig {
  boot_order: string[]
  firmware: 'bios' | 'uefi'
  secure_boot: boolean
  kernel?: string
  initrd?: string
  kernel_args?: string
}

export async function getBootConfig(vmName: string): Promise<BootConfig> {
  return apiGet<BootConfig>(`${API_BASE_URL}/vms/${vmName}/boot`)
}

export async function updateBootConfig(vmName: string, config: Partial<BootConfig>): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/boot`, config)
}

// CPU Model & Features
export interface CPUModelConfig {
  model: string
  mode: 'custom' | 'host-model' | 'host-passthrough'
  features_add: string[]
  features_remove: string[]
  vendor?: string
}

export interface CPUModel {
  name: string
  vendor: string
  features: string[]
}

export async function listCPUModels(): Promise<CPUModel[]> {
  return apiGet<CPUModel[]>(`${API_BASE_URL}/system/cpu-models`)
}

export async function getCPUConfig(vmName: string): Promise<CPUModelConfig> {
  return apiGet<CPUModelConfig>(`${API_BASE_URL}/vms/${vmName}/cpu-model`)
}

export async function updateCPUConfig(vmName: string, config: Partial<CPUModelConfig>): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/cpu-model`, config)
}

// VM Watchdog
export interface WatchdogConfig {
  model: 'i6300esb' | 'ib700'
  action: 'reset' | 'shutdown' | 'poweroff' | 'pause' | 'none'
}

export async function getWatchdog(vmName: string): Promise<WatchdogConfig | null> {
  return apiGet<WatchdogConfig | null>(`${API_BASE_URL}/vms/${vmName}/watchdog`)
}

export async function setWatchdog(vmName: string, config: WatchdogConfig): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/watchdog`, config)
}

// VM Serial Console
export interface SerialConfig {
  type: 'pty' | 'unix' | 'tcp'
  path?: string
  source_host?: string
  source_port?: number
}

export async function listSerials(vmName: string): Promise<SerialConfig[]> {
  return apiGet<SerialConfig[]>(`${API_BASE_URL}/vms/${vmName}/serials`)
}

export async function addSerial(vmName: string, config: SerialConfig): Promise<void> {
  return apiPostVoid(`${API_BASE_URL}/vms/${vmName}/serials`, config)
}
