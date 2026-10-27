// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { FormEvent, useEffect, useMemo, useState } from 'react'
import { Link, Navigate, useNavigate } from 'react-router'
import {
  ArrowRight,
  ChevronLeft,
  Eye,
  EyeOff,
  Loader2,
  Lock,
  User,
} from 'lucide-react'
import { useAuth } from '../contexts/AuthContext'
import { formatUserError } from '../utils/apiError'
import { ZyvorLockup } from '../components/ZyvorMark'
import {
  PremiumLoginShell,
  LoginError,
  LoginField,
  LoginSubmit,
  LoginRemember,
} from '../components/PremiumLoginShell'

type LoginStep = 'identify' | 'password'

export type FabricInstanceInfo = {
  product: string
  product_id: string
  version: string
  hostname: string
  deploy_mode: string
  deploy_label: string
  kubernetes: boolean
  kubernetes_namespace?: string | null
  listen?: string | null
}

const SAVE_KEY = 'zyvor-fabric-saved-login'

function inferDeployFromBrowser(): Pick<
  FabricInstanceInfo,
  'deploy_mode' | 'deploy_label' | 'kubernetes'
> {
  const port = window.location.port
  if (port === '30095') {
    return {
      deploy_mode: 'kubernetes',
      deploy_label: 'Kubernetes · NodePort 30095',
      kubernetes: true,
    }
  }
  if (port === '9095' || port === '') {
    return {
      deploy_mode: 'host',
      deploy_label: 'Host service · :9095',
      kubernetes: false,
    }
  }
  return {
    deploy_mode: 'unknown',
    deploy_label: `Web · :${port || 'default'}`,
    kubernetes: false,
  }
}

async function fetchInstance(): Promise<FabricInstanceInfo> {
  const fallbackHost = window.location.hostname || 'localhost'
  const inferred = inferDeployFromBrowser()
  try {
    const res = await fetch('/api/instance', { credentials: 'same-origin' })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const data = (await res.json()) as Partial<FabricInstanceInfo>
    return {
      product: data.product || 'Zyvor Fabric',
      product_id: data.product_id || 'zyvor-fabric',
      version: data.version || 'dev',
      hostname: data.hostname || fallbackHost,
      deploy_mode: data.deploy_mode || inferred.deploy_mode,
      deploy_label: data.deploy_label || inferred.deploy_label,
      kubernetes: Boolean(data.kubernetes ?? inferred.kubernetes),
      kubernetes_namespace: data.kubernetes_namespace ?? null,
      listen: data.listen ?? null,
    }
  } catch {
    return {
      product: 'Zyvor Fabric',
      product_id: 'zyvor-fabric',
      version: 'dev',
      hostname: fallbackHost,
      ...inferred,
      kubernetes_namespace: null,
      listen: null,
    }
  }
}

export default function SignIn() {
  const { login, isAuthenticated, loading } = useAuth()
  const navigate = useNavigate()

  const saved = (() => {
    try {
      const raw = localStorage.getItem(SAVE_KEY)
      if (!raw) return null
      const parsed = JSON.parse(raw) as { username?: string }
      return { username: parsed?.username || '' }
    } catch {
      return null
    }
  })()

  const [step, setStep] = useState<LoginStep>(saved?.username ? 'password' : 'identify')
  const [username, setUsername] = useState(saved?.username || '')
  const [password, setPassword] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [rememberMe, setRememberMe] = useState(!!saved?.username)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [instance, setInstance] = useState<FabricInstanceInfo | null>(null)

  useEffect(() => {
    let alive = true
    fetchInstance().then((info) => {
      if (alive) setInstance(info)
    })
    return () => {
      alive = false
    }
  }, [])

  const pills = useMemo(() => {
    const base = [
      { label: 'Zyvor', tone: 'orange' as const },
      { label: 'KVM', tone: 'emerald' as const },
      { label: 'FluxVM', tone: 'violet' as const },
      { label: 'CLI + Web', tone: 'sky' as const },
    ]
    if (instance?.kubernetes) {
      return [{ label: 'Kubernetes', tone: 'sky' as const }, ...base]
    }
    return [{ label: 'Bare metal', tone: 'amber' as const }, ...base]
  }, [instance?.kubernetes])

  if (!loading && isAuthenticated) {
    return <Navigate to="/app" replace />
  }

  const scrollToForm = () => {
    document.getElementById('login-sign-in')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  const handleContinue = (e: FormEvent) => {
    e.preventDefault()
    if (!username.trim()) return
    setError(null)
    setStep('password')
  }

  const handleBack = () => {
    setStep('identify')
    setPassword('')
    setShowPassword(false)
    setError(null)
  }

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!username.trim() || !password) return
    setError(null)
    setSubmitting(true)
    try {
      await login(username.trim(), password)
      if (rememberMe) {
        localStorage.setItem(SAVE_KEY, JSON.stringify({ username: username.trim() }))
      } else {
        localStorage.removeItem(SAVE_KEY)
      }
      navigate('/app', { replace: true })
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setSubmitting(false)
    }
  }

  const instanceMeta = instance ? (
    <>
      <span className="login-instance-chip" data-kind="product">
        <strong>{instance.product}</strong>
      </span>
      <span className="login-instance-chip" title="Host">
        {instance.hostname}
      </span>
      <span
        className="login-instance-chip"
        data-kind={instance.kubernetes ? 'k8s' : undefined}
        title="Deploy mode"
      >
        {instance.deploy_label}
      </span>
      {instance.kubernetes_namespace ? (
        <span className="login-instance-chip" data-kind="k8s" title="Namespace">
          ns/{instance.kubernetes_namespace}
        </span>
      ) : null}
      <span className="login-instance-chip" title="Version">
        v{instance.version}
      </span>
    </>
  ) : null

  const formInstance = instance ? (
    <dl className="login-form-instance">
      <dt>Project</dt>
      <dd>{instance.product}</dd>
      <dt>System</dt>
      <dd>
        {instance.hostname}
        {instance.listen ? ` · ${instance.listen}` : ''}
      </dd>
      <dt>Deploy</dt>
      <dd>
        {instance.deploy_label}
        {instance.kubernetes_namespace ? ` · ${instance.kubernetes_namespace}` : ''}
      </dd>
    </dl>
  ) : null

  const storeCta = (
    <>
      <a
        href="#login-sign-in"
        className="login-cta-primary"
        onClick={(e) => {
          e.preventDefault()
          scrollToForm()
        }}
      >
        Sign in
      </a>
      <Link to="/" className="login-cta-secondary">
        Learn more
      </Link>
    </>
  )

  const panelSubtitle =
    step === 'password' ? (
      <>
        Enter the password for <span className="login-apple-host">{username.trim()}</span>
      </>
    ) : (
      'Sign in to Zyvor Fabric'
    )

  const chapterNote = instance
    ? `${instance.product} · ${instance.deploy_label} · ${instance.hostname}`
    : 'Zyvor Fabric · sign in to continue'

  return (
    <PremiumLoginShell
      logo={
        <a
          href="https://zyvor.dev"
          target="_blank"
          rel="noopener noreferrer"
          className="login-zyvor-mark"
          aria-label="zyvor.dev"
          title="zyvor.dev"
        >
          <span className="inline-flex items-center gap-2">
            <ZyvorLockup
              markClassName="w-9 h-9"
              wordmarkClassName="text-[1.35rem] text-[var(--zf-ink)]"
            />
            <span className="text-[1.15rem] font-medium text-[var(--zf-muted)] tracking-[-0.02em] leading-none">
              Fabric
            </span>
          </span>
        </a>
      }
      productName="Zyvor Fabric"
      productWordmark=""
      heroTitle="Private cloud. One daemon."
      heroSubheadline="VMs, networking, storage, and security — driven by FluxVM on Linux KVM."
      accent="orange"
      pills={pills}
      instanceMeta={instanceMeta}
      heroCta={storeCta}
      chapterNote={chapterNote}
      panelSubtitle={panelSubtitle}
      panelHint={
        step === 'identify' ? (
          instance?.kubernetes ? (
            <>
              Signing into <span className="font-mono">{instance.product}</span> on Kubernetes
              {instance.kubernetes_namespace ? (
                <>
                  {' '}
                  (<span className="font-mono">ns/{instance.kubernetes_namespace}</span>)
                </>
              ) : null}
              . Retrieve password from Secret{' '}
              <span className="font-mono">zyvor-fabric-secrets</span>
              {' '}
              (<span className="font-mono">kubectl get secret … -o jsonpath=…</span>). Labs with{' '}
              <span className="font-mono">FABRIC_LAB_DEFAULTS=1</span> may use{' '}
              <span className="font-mono">admin</span> / <span className="font-mono">Admin@321</span>.
            </>
          ) : (
            <>
              Signing into <span className="font-mono">{instance?.product ?? 'Zyvor Fabric'}</span> on
              this host. Retrieve password from{' '}
              <span className="font-mono">/var/lib/zyvor-fabricd/.admin_password</span>
              {instance?.deploy_mode === 'host' ? (
                <>
                  {' '}
                  (<span className="font-mono">sudo cat /var/lib/zyvor-fabricd/.admin_password</span>).
                </>
              ) : null}{' '}
              Labs with <span className="font-mono">FABRIC_LAB_DEFAULTS=1</span> may use{' '}
              <span className="font-mono">admin</span> / <span className="font-mono">Admin@321</span>.
            </>
          )
        ) : null
      }
      footer={
        <p className="login-hint" style={{ marginTop: 0, marginBottom: '1.25rem' }}>
          © 2026 Zyvor ·{' '}
          <a
            href="https://zyvor.dev"
            target="_blank"
            rel="noopener noreferrer"
            className="login-zyvor-link"
          >
            zyvor.dev
          </a>
        </p>
      }
      showSignInChapter
    >
      {step === 'identify' ? (
        <form
          key="identify"
          onSubmit={handleContinue}
          autoComplete="on"
          aria-label="Account"
          className="login-apple-step text-left"
          noValidate
        >
          {formInstance}
          {error ? <LoginError message={error} /> : null}
          <div className="login-apple-fields">
            <LoginField label="Username" id="username">
              <User className="login-field-icon" />
              <input
                id="username"
                name="username"
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="login-input"
                placeholder="admin"
                autoComplete="username"
                autoFocus
                required
              />
            </LoginField>
          </div>
          <LoginSubmit loading={false} disabled={!username.trim()}>
            <span>Continue</span>
            <ArrowRight className="h-4 w-4" />
          </LoginSubmit>
        </form>
      ) : (
        <form
          key="password"
          onSubmit={onSubmit}
          autoComplete="on"
          aria-label="Password"
          className="login-apple-step text-left"
          noValidate
        >
          <button type="button" onClick={handleBack} className="login-apple-identity" aria-label="Change account">
            <ChevronLeft aria-hidden className="h-4 w-4 shrink-0" />
            <span className="truncate">{username.trim()}</span>
          </button>
          <input type="text" name="username" value={username} autoComplete="username" readOnly hidden />
          {formInstance}
          {error ? <LoginError message={error} /> : null}
          <div className="login-apple-fields">
            <LoginField label="Password" id="password">
              <Lock className="login-field-icon" />
              <input
                id="password"
                name="password"
                type={showPassword ? 'text' : 'password'}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="login-input pr-11"
                placeholder="Password"
                autoComplete="current-password"
                autoFocus
                required
                disabled={submitting}
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="absolute right-3.5 top-1/2 -translate-y-1/2 text-zinc-400 hover:text-zinc-700 transition-colors"
                aria-label={showPassword ? 'Hide password' : 'Show password'}
              >
                {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </button>
            </LoginField>
          </div>
          <LoginRemember
            checked={rememberMe}
            onChange={(checked) => {
              setRememberMe(checked)
              if (!checked) localStorage.removeItem(SAVE_KEY)
            }}
            label="Remember me on this device"
          />
          <LoginSubmit loading={submitting} disabled={!password}>
            {submitting ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>Signing in…</span>
              </>
            ) : (
              <>
                <span>Sign In</span>
                <ArrowRight className="h-4 w-4" />
              </>
            )}
          </LoginSubmit>
        </form>
      )}
    </PremiumLoginShell>
  )
}
