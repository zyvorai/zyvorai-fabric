// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

/** Canonical icon size scale -- use instead of hand-typing w-N h-N pairs. */
export const ICON_SIZE = {
  xs: 'w-3.5 h-3.5', // 14px -- inline-with-text: table cells, badges, dense metadata rows
  sm: 'w-4 h-4', // 16px -- default: buttons, nav items, list-row icons
  md: 'w-5 h-5', // 20px -- section/card headers, standalone action icons
  lg: 'w-6 h-6', // 24px -- page-level hero icons, empty states, feature callouts
} as const
