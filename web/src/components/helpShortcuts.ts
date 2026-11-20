// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPod|iPad/i.test(navigator.platform)
const modKey = isMac ? '⌘' : 'Ctrl'

export const helpShortcuts: { keys: string[]; description: string }[] = [
  { keys: [modKey, 'K'], description: 'Command palette' },
  { keys: ['g', 'd'], description: 'Go to Dashboard' },
  { keys: ['g', 'v'], description: 'Go to Virtual Machines' },
  { keys: ['g', 'n'], description: 'Go to Network' },
  { keys: ['g', 's'], description: 'Go to Storage' },
  { keys: ['g', 'c'], description: 'Create VM' },
  { keys: ['g', 'l'], description: 'Go to Logs' },
  { keys: ['g', 'b'], description: 'Go to Backups' },
  { keys: ['g', 'i'], description: 'Go to Disk Images' },
  { keys: ['g', 'e'], description: 'Go to Live Metrics' },
  { keys: ['?'], description: 'Help (shortcuts & about)' },
]
