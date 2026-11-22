// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

/** Parse JSON from a fetch Response; rejects HTML SPA fallthrough. */
export async function parseJsonResponse<T>(res: Response): Promise<T> {
  const contentType = res.headers?.get('content-type') ?? ''
  const text = await res.text()
  const trimmed = text.trimStart()
  const looksJson =
    contentType.includes('json') || trimmed.startsWith('{') || trimmed.startsWith('[')
  if (!looksJson) {
    const preview = text.slice(0, 120).replace(/\s+/g, ' ').trim()
    throw new Error(preview || 'The API returned a non-JSON response')
  }
  try {
    return JSON.parse(text) as T
  } catch {
    throw new Error('Failed to parse API response as JSON')
  }
}
