// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { FleetVM } from './fleet'

/** An invented fleet for pages that show what the console looks like before anyone has signed in (the sign-in backdrop and the
    marketing mock). It is never shown as real data: the pages that use it say so. */
export const SAMPLE_FLEET: FleetVM[] = [
  { name: 'web-01', state: 'running' },
  { name: 'web-02', state: 'running' },
  { name: 'api-01', state: 'running' },
  { name: 'api-02', state: 'running' },
  { name: 'db-primary', state: 'running' },
  { name: 'db-replica', state: 'running' },
  { name: 'ci-runner', state: 'paused' },
  { name: 'batch-7', state: 'stopped' },
]
