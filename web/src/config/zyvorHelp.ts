// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { ZYVOR_COPY, ZYVOR_URL } from '../components/ZyvorBrand'

export { ZYVOR_URL, ZYVOR_COPY }

export const ZYVOR_HELP = {
  platform: ZYVOR_URL,
  docs: 'https://zyvor.dev/docs',
  docsIntro: 'https://zyvor.dev/docs/intro',
  products: 'https://zyvor.dev/docs/products',
  contact: 'https://zyvor.dev/contact',
  demo: 'https://zyvor.dev/demo',
  suite: 'https://zyvor.dev/docs/intro#suite-product-guides',
  sales: 'mailto:sales@zyvor.dev',
  info: 'mailto:info@zyvor.dev',
} as const

export type ProductHelpMeta = {
  name: string
  tagline: string
  version: string
  productUrl: string
}

/** Primary product identity — Zyvor Fabric control plane. */
export const ZYVOR_FABRIC_HELP: ProductHelpMeta = {
  name: 'Zyvor Fabric',
  tagline: 'Systemd-native private cloud control plane',
  version: '0.1.0',
  productUrl: 'https://zyvor.dev',
}

/** Technical daemon name (systemd unit, API host). */
export const ZYVOR_FABRIC_DAEMON = 'zyvor-fabricd'
