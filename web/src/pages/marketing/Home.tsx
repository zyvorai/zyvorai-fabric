// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router'
import { useAuth } from '../../contexts/AuthContext'
import MarketingLayout from '../../components/MarketingLayout'

export default function Home() {
  const { isAuthenticated } = useAuth()

  return (
    <MarketingLayout>
      <section className="mkt-hero">
        <p className="mkt-reveal text-[12px] font-semibold tracking-[0.08em] uppercase text-[var(--zf-secondary)] mb-5">
          Zyvor Fabric
        </p>
        <h1 className="mkt-reveal">Private cloud.<br />Beautifully simple.</h1>
        <p className="lede mkt-reveal-delay">
          One control plane for Linux VMs, networking, storage, and security — without the heavyweight stack.
        </p>
        <div className="mkt-cta-row mkt-reveal-delay-2">
          {isAuthenticated ? (
            <Link to="/app" className="zf-btn zf-btn-primary">
              Open console
            </Link>
          ) : (
            <Link to="/sign-in" className="zf-btn zf-btn-primary">
              Sign in
            </Link>
          )}
          <Link to="/product" className="zf-btn zf-btn-secondary">
            Learn more →
          </Link>
        </div>
        <div
          className="mkt-reveal-delay-2 mt-20 w-full max-w-4xl aspect-[16/9] rounded-[28px] overflow-hidden border border-[var(--zf-hairline)]"
          style={{
            background:
              'linear-gradient(160deg, #1d1d1f 0%, #2c2c2e 45%, #1d1d1f 100%)',
          }}
          aria-hidden
        >
          <div className="h-full w-full flex flex-col items-center justify-center text-[#f5f5f7] px-8">
            <div className="text-[11px] tracking-[0.14em] uppercase text-[#a1a1a6] mb-3">Console</div>
            <div className="text-3xl sm:text-4xl font-semibold tracking-[-0.04em] text-center">
              Fleet. Network. Storage.
            </div>
            <div className="mt-8 grid grid-cols-3 gap-3 w-full max-w-lg opacity-80">
              {[72, 48, 86].map((h, i) => (
                <div key={i} className="rounded-xl bg-white/10 border border-white/10 p-4">
                  <div className="h-1.5 rounded-full bg-white/25 mb-3" style={{ width: `${h}%` }} />
                  <div className="h-1.5 rounded-full bg-white/15 mb-2" />
                  <div className="h-1.5 rounded-full bg-white/10 w-2/3" />
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="mkt-section">
        <h2>Designed for operators.</h2>
        <p>
          Create VMs, wire networks, and enforce policy from a calm console — backed by a single Rust daemon and FluxVM.
        </p>
      </section>

      <section className="mkt-band">
        <h2>From bare metal to fleet.</h2>
        <p>
          Web console, CLI, Kubernetes operator, and Terraform — one API underneath. Merion gives an
          untrusted agent a real computer; you keep the keys and the CONNECT proof.
        </p>
        <div className="flex flex-wrap gap-3 mt-6">
          <Link to="/platform" className="zf-btn mkt-band-cta">
            See the platform
          </Link>
          <Link to="/merion" className="zf-btn zf-btn-secondary">
            Meet Merion →
          </Link>
        </div>
      </section>
    </MarketingLayout>
  )
}
