// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import eslint from '@eslint/js'
import tseslint from 'typescript-eslint'
import reactHooks from 'eslint-plugin-react-hooks'

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    plugins: {
      'react-hooks': reactHooks,
    },
    rules: {
      // Classic hooks rules only. eslint-plugin-react-hooks >=7 folds React
      // Compiler lint into `recommended`, which currently fails ~50 pages on
      // patterns this app already uses (setState-in-effect, etc.). Re-enable
      // compiler rules once pages are migrated.
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' }],
    },
  },
  {
    // the .mjs files are standalone Node tooling (screenshots, a11y gate, mock API)
    ignores: ['dist/**', '*.mjs', 'mock-api.ts'],
  }
)
