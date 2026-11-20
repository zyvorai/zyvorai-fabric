// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { updateBootConfig, updateDisplay, updateCPUConfig } from '../api/devices'
import { enableUefi } from '../api/firmware'

export interface CreateAdvancedOptions {
  firmware: 'bios' | 'uefi'
  secureBoot: boolean
  cpuMode: 'host-passthrough' | 'host-model' | 'custom'
  displayType: 'vnc' | 'spice'
  bootOrder: string[]
}

/** One option failing to apply (e.g. UEFI on a driver backend that doesn't support it yet) shouldn't silently skip the rest. */
export class AdvancedOptionsError extends Error {
  constructor(public failures: Array<{ option: string; error: unknown }>) {
    super(failures.map((f) => f.option).join(', '))
    this.name = 'AdvancedOptionsError'
  }
}

export async function applyCreateAdvancedOptions(
  vmName: string,
  advanced: CreateAdvancedOptions,
): Promise<void> {
  const tasks: Array<{ option: string; run: () => Promise<unknown> }> = [
    {
      option: 'Boot configuration',
      run: () =>
        updateBootConfig(vmName, {
          firmware: advanced.firmware,
          secure_boot: advanced.secureBoot,
          boot_order: advanced.bootOrder,
        }),
    },
    { option: 'Display protocol', run: () => updateDisplay(vmName, { type: advanced.displayType }) },
    { option: 'CPU mode', run: () => updateCPUConfig(vmName, { mode: advanced.cpuMode }) },
  ]
  if (advanced.firmware === 'uefi') {
    tasks.push({ option: 'UEFI firmware', run: () => enableUefi(vmName, { secure_boot: advanced.secureBoot }) })
  }

  const results = await Promise.allSettled(tasks.map((t) => t.run()))
  const failures = results
    .map((r, i) => (r.status === 'rejected' ? { option: tasks[i].option, error: r.reason } : null))
    .filter((f): f is { option: string; error: unknown } => f !== null)

  if (failures.length > 0) throw new AdvancedOptionsError(failures)
}
