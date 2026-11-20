// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

export default function PageSkeleton() {
  return (
    <div className="space-y-6 animate-fade-in">
      <div className="h-8 w-48 skeleton" />
      <div className="h-12 w-full skeleton" />
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <div className="h-40 skeleton" />
        <div className="h-40 skeleton" />
      </div>
      <div className="h-64 skeleton" />
    </div>
  )
}
