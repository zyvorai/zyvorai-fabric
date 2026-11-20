// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

export const stateColors: Record<string, string> = {
  running: 'bg-green-500',
  stopped: 'bg-red-500',
  paused: 'bg-yellow-500',
  unknown: 'bg-gray-500',
}

export function getStateColor(state: string): string {
  return stateColors[state] || stateColors.unknown
}
