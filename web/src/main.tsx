// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './styles/main.css'
import { getToken, setToken } from './api/client'

// `npm run dev:mock` opens signed in; the mock API accepts any token.
if (import.meta.env.MODE === 'mock' && !getToken()) setToken('mock-token')

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
