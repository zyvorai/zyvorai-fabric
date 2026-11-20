// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

export default function ReadOnlyNotice() {
  return (
    <div className="text-sm text-amber-400/90 bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2 mb-4">
      You are signed in as a viewer. Create, edit, and delete actions are disabled.
    </div>
  )
}
