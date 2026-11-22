// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

/** Stable daemon `error_code` values → short user-facing labels. */
export const API_ERROR_LABELS: Record<string, string> = {
  operation_failed: 'The operation failed on the server',
  not_found: 'The requested resource was not found',
  invalid_request: 'The request was invalid',
  forbidden: 'You do not have permission for this action',
  libvirt_connection: 'Could not connect to libvirt on the host',
  internal_error: 'An internal server error occurred',
  unauthorized: 'Authentication required or your session expired',
}

const ERROR_CODE_RE =
  /\((operation_failed|not_found|invalid_request|forbidden|libvirt_connection|internal_error|unauthorized)\)\s*$/i

/** Human label for a stable API error code. */
export function friendlyErrorCode(code: string): string {
  return API_ERROR_LABELS[code] ?? code.replace(/_/g, ' ')
}

/** Strip HTML pages and trim noisy API text for UI display. */
export function sanitizeErrorText(text: string): string {
  const t = text.trim()
  if (!t) return ''

  if (/<!DOCTYPE\s+html/i.test(t) || /<html[\s>]/i.test(t) || t.includes('</html>')) {
    return (
      'The API returned an HTML error page instead of JSON. ' +
      'This usually means a reverse proxy failure or the API service is down.'
    )
  }

  if (t.includes('<') && t.includes('>')) {
    const stripped = t.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim()
    if (stripped.length > 0 && stripped.length < t.length * 0.6) {
      return stripped.length > 500 ? `${stripped.slice(0, 497)}…` : stripped
    }
  }

  return t.length > 600 ? `${t.slice(0, 597)}…` : t
}

/**
 * Build a user-facing message from an HTTP error body (JSON `{ error, error_code }` or plain/HTML).
 */
export function formatHttpErrorBody(status: number, statusText: string, text: string): string {
  const raw = text.trim()
  const statusLabel = `HTTP ${status}${statusText ? ` ${statusText}` : ''}`

  if (!raw) {
    return `Request failed (${statusLabel})`
  }

  if (/<!DOCTYPE\s+html/i.test(raw) || /^<\s*html/i.test(raw)) {
    return (
      `Request failed (${statusLabel}): the server returned an HTML error page instead of JSON. ` +
      'Check that Zyvor Fabric (zyvor-fabricd) is running and reachable on this host.'
    )
  }

  try {
    const j = JSON.parse(raw) as { error?: string; message?: string; error_code?: string }
    const code = typeof j.error_code === 'string' ? j.error_code : undefined
    const rawMsg =
      typeof j.error === 'string' ? j.error : typeof j.message === 'string' ? j.message : ''
    const clean = sanitizeErrorText(rawMsg)

    if (clean) {
      if (code && (clean === code || clean === friendlyErrorCode(code))) {
        return friendlyErrorCode(code)
      }
      if (code && !clean.toLowerCase().includes(code.replace(/_/g, ' '))) {
        return `${clean} (${friendlyErrorCode(code)})`
      }
      return clean
    }
    if (code) {
      return friendlyErrorCode(code)
    }
  } catch {
    /* not JSON */
  }

  const sanitized = sanitizeErrorText(raw)
  if (sanitized !== raw) {
    return sanitized
  }
  if (raw.length > 400) {
    return `Request failed (${statusLabel}): ${raw.slice(0, 200)}…`
  }
  return `Request failed (${statusLabel}): ${raw}`
}

/** Format any thrown value for toasts and banners. */
export function formatUserError(e: unknown): string {
  if (e instanceof Error) {
    const msg = sanitizeErrorText(e.message)
    const m = msg.match(ERROR_CODE_RE)
    if (m?.[1] && msg.replace(ERROR_CODE_RE, '').trim().length < 8) {
      return friendlyErrorCode(m[1])
    }
    return msg || 'Unknown error'
  }
  return sanitizeErrorText(String(e)) || 'Unknown error'
}

export type ParsedApiError = {
  message: string
  code?: string
}

export function parseUserError(e: unknown): ParsedApiError {
  const message = formatUserError(e)
  const m = message.match(ERROR_CODE_RE)
  return { message: message.replace(ERROR_CODE_RE, '').trim() || message, code: m?.[1] }
}
