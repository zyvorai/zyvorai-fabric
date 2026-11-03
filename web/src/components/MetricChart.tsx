// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts'

interface MetricChartProps {
  title: string
  data: Array<{ time: string; value: number }>
  color?: string
}

export default function MetricChart({ title, data, color = '#3b82f6' }: MetricChartProps) {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-lg p-6 border border-[var(--zf-hairline)]">
      <h3 className="text-lg font-semibold mb-4">{title}</h3>
      <ResponsiveContainer width="100%" height={200}>
        <LineChart data={data}>
          <CartesianGrid strokeDasharray="3 3" stroke="#d2d2d7" />
          <XAxis dataKey="time" stroke="#6e6e73" />
          <YAxis stroke="#6e6e73" />
          <Tooltip
            contentStyle={{
              backgroundColor: '#ffffff',
              border: '1px solid #d2d2d7',
              borderRadius: '0.5rem',
              color: '#1d1d1f',
              fontFamily: 'SF Pro Text, -apple-system, BlinkMacSystemFont, Helvetica Neue, Helvetica, Arial, sans-serif',
            }}
          />
          <Line type="monotone" dataKey="value" stroke={color === '#3b82f6' ? '#0066cc' : color} strokeWidth={2} dot={false} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  )
}
