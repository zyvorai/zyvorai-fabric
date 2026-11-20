// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { formatUserError } from './apiError'

type ToastLike = {
  error: (message: string, duration?: number) => string
}

/** Show a toast with a sanitized API/daemon error message. */
export function toastFailure(toast: ToastLike, label: string, e: unknown): void {
  toast.error(`${label}: ${formatUserError(e)}`)
}
