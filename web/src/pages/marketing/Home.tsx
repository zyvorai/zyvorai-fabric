// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router'
import { useAuth } from '../../contexts/AuthContext'
import MarketingLayout from '../../components/MarketingLayout'
import { ConsoleMock } from '../../components/ConsoleMock'
import { AuroraBackdrop } from '../../components/ui'

export default function Home() {
  const { isAuthenticated } = useAuth()

  return (
    <MarketingLayout>
      <section className="mkt-hero relative overflow-hidden">
        <AuroraBackdrop />
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
        <div className="mkt-reveal-delay-2 mt-16 w-full max-w-4xl">
          <ConsoleMock />
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
        <div className="flex flex-wrap justify-center gap-3 mt-6">
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
