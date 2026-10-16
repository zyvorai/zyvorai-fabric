// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest'
import { DICTIONARIES, MerionKey, translate } from './merion'

const keys = Object.keys(DICTIONARIES.en) as MerionKey[]

describe('keep strings', () => {
  it('has a Chinese string for every English one, and no empty ones', () => {
    for (const k of keys) {
      expect(DICTIONARIES['zh-CN'][k], k).toBeTruthy()
      expect(DICTIONARIES.en[k], k).toBeTruthy()
    }
    expect(Object.keys(DICTIONARIES['zh-CN']).sort()).toEqual([...keys].sort())
  })

  it('keeps the same placeholders in both languages', () => {
    const names = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()
    for (const k of keys) expect(names(DICTIONARIES['zh-CN'][k]), k).toEqual(names(DICTIONARIES.en[k]))
  })

  it('fills placeholders and leaves unknown ones visible', () => {
    expect(translate('en', 'home.run', { title: 'CSV clean' })).toBe('Run CSV clean')
    expect(translate('zh-CN', 'home.batchDone', { ok: 2, count: 3 })).toBe('已完成 2 / 3')
    expect(translate('en', 'home.run')).toBe('Run {title}')
  })

  it('keeps the product name untranslated', () => {
    expect(DICTIONARIES['zh-CN']['keep.title']).toBe('Keep')
    expect(DICTIONARIES['zh-CN']['home.connects']).toContain('连接')
  })
})
