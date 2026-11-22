// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

type VmRow = {
  name: string
  state: string
  cpus: number
  memory: number
}

type FabricEvent = {
  id: string
  event_type: string
  vm_name: string
  detail?: string | null
  timestamp: string
}

type ChatMessage = {
  role: 'user' | 'assistant'
  text: string
}

type Tab = 'dashboard' | 'events' | 'copilot'

export default function App() {
  const [tab, setTab] = useState<Tab>('dashboard')
  const [endpoint, setEndpoint] = useState('http://127.0.0.1:9095')
  const [token, setToken] = useState('')
  const [health, setHealth] = useState('')
  const [vms, setVms] = useState<VmRow[]>([])
  const [events, setEvents] = useState<FabricEvent[]>([])
  const [streamOn, setStreamOn] = useState(false)
  const [streamError, setStreamError] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [chat, setChat] = useState<ChatMessage[]>([])
  const [question, setQuestion] = useState('')
  const [copilotBusy, setCopilotBusy] = useState(false)

  const auth = { endpoint, token: token || null }

  const refresh = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const h = await invoke<string>('fabric_health', auth)
      setHealth(h)
      const list = await invoke<VmRow[]>('fabric_list_vms', auth)
      setVms(list)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [endpoint, token])

  useEffect(() => {
    let unlistenEvent: (() => void) | undefined
    let unlistenErr: (() => void) | undefined
    let unlistenStop: (() => void) | undefined

    ;(async () => {
      unlistenEvent = await listen<FabricEvent>('fabric-event', (ev) => {
        setEvents((prev) => [ev.payload, ...prev].slice(0, 200))
      })
      unlistenErr = await listen<string>('fabric-stream-error', (ev) => {
        setStreamError(String(ev.payload))
        setStreamOn(false)
      })
      unlistenStop = await listen('fabric-stream-stopped', () => {
        setStreamOn(false)
      })
    })()

    return () => {
      unlistenEvent?.()
      unlistenErr?.()
      unlistenStop?.()
      invoke('fabric_stop_events').catch(() => {})
    }
  }, [])

  async function toggleStream() {
    setStreamError('')
    if (streamOn) {
      await invoke('fabric_stop_events')
      setStreamOn(false)
      return
    }
    try {
      await invoke('fabric_start_events', auth)
      setStreamOn(true)
    } catch (e) {
      setStreamError(String(e))
    }
  }

  async function askCopilot() {
    const q = question.trim()
    if (!q) return
    setQuestion('')
    setChat((prev) => [...prev, { role: 'user', text: q }])
    setCopilotBusy(true)
    try {
      const answer = await invoke<string>('fabric_copilot', {
        req: { endpoint, token: token || null, question: q },
      })
      setChat((prev) => [...prev, { role: 'assistant', text: answer }])
    } catch (e) {
      setChat((prev) => [...prev, { role: 'assistant', text: `Error: ${e}` }])
    } finally {
      setCopilotBusy(false)
    }
  }

  return (
    <div className="app">
      <header>
        <h1>Machina</h1>
        <p className="tagline">Zyvor Fabric · Infrastructure Workbench (v0.1)</p>
        <nav className="tabs">
          {(['dashboard', 'events', 'copilot'] as Tab[]).map((t) => (
            <button
              key={t}
              type="button"
              className={tab === t ? 'tab active' : 'tab'}
              onClick={() => setTab(t)}
            >
              {t === 'dashboard' ? 'Dashboard' : t === 'events' ? 'Live events' : 'AI Copilot'}
            </button>
          ))}
        </nav>
      </header>

      <section className="panel connect">
        <label>
          Endpoint
          <input value={endpoint} onChange={(e) => setEndpoint(e.target.value)} />
        </label>
        <label>
          Token (optional)
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="JWT from /api/auth/login"
          />
        </label>
        <div className="row">
          <button type="button" onClick={refresh} disabled={loading}>
            {loading ? 'Connecting…' : 'Connect'}
          </button>
          {tab === 'events' && (
            <button type="button" className={streamOn ? 'secondary danger' : 'secondary'} onClick={toggleStream}>
              {streamOn ? 'Stop SSE stream' : 'Start SSE stream'}
            </button>
          )}
        </div>
        {error && <p className="error">{error}</p>}
        {health && <p className="health">Health: {health}</p>}
      </section>

      {tab === 'dashboard' && (
        <section className="panel">
          <h2>Virtual machines</h2>
          {vms.length === 0 ? (
            <p className="muted">No VMs loaded — connect to a Fabric cluster.</p>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>State</th>
                  <th>CPUs</th>
                  <th>Memory (MB)</th>
                </tr>
              </thead>
              <tbody>
                {vms.map((vm) => (
                  <tr key={vm.name}>
                    <td>{vm.name}</td>
                    <td>{vm.state}</td>
                    <td>{vm.cpus}</td>
                    <td>{vm.memory}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {tab === 'events' && (
        <section className="panel">
          <h2>Live events (SSE)</h2>
          {streamError && <p className="error">{streamError}</p>}
          {!streamOn && events.length === 0 && (
            <p className="muted">Start the SSE stream to watch VM lifecycle events in real time.</p>
          )}
          <ul className="event-list">
            {events.map((ev) => (
              <li key={ev.id}>
                <span className="ev-time">{ev.timestamp}</span>
                <span className={`ev-type ev-${ev.event_type}`}>{ev.event_type}</span>
                <span className="ev-vm">{ev.vm_name}</span>
                {ev.detail && <span className="ev-detail">{ev.detail}</span>}
              </li>
            ))}
          </ul>
        </section>
      )}

      {tab === 'copilot' && (
        <section className="panel copilot">
          <h2>AI Infrastructure Copilot (v0.1)</h2>
          <p className="muted">
            Rule-based assistant over live Fabric APIs — including per-VM metrics for performance questions. Local LLM integration planned for v0.4.
          </p>
          <div className="chat-log">
            {chat.length === 0 && (
              <p className="muted">Try: &quot;List VMs&quot;, &quot;Any unhealthy VMs?&quot;, &quot;What changed recently?&quot;</p>
            )}
            {chat.map((m, i) => (
              <div key={i} className={`chat-bubble ${m.role}`}>
                <strong>{m.role === 'user' ? 'You' : 'Machina'}</strong>
                <pre>{m.text}</pre>
              </div>
            ))}
          </div>
          <div className="chat-input row">
            <input
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && !copilotBusy && askCopilot()}
              placeholder="Why is my VM slow?"
              disabled={copilotBusy}
            />
            <button type="button" onClick={askCopilot} disabled={copilotBusy}>
              {copilotBusy ? '…' : 'Ask'}
            </button>
          </div>
        </section>
      )}

      <footer>
        <a href="https://zyvor.dev" target="_blank" rel="noreferrer">
          zyvor.dev
        </a>
      </footer>
    </div>
  )
}
