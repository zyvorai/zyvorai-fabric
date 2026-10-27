// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * Zyvor suite branding — text only.
 * Brand line: zyvor.dev · © 2026 (all orange, no footer bar).
 */
import React from 'react';

export const ZYVOR_URL = 'https://zyvor.dev';
export const ZYVOR_BRAND = 'Zyvor';
export const ZYVOR_COPY = '© 2026';
export const ZYVOR_LINE = `zyvor.dev · ${ZYVOR_COPY}`;

const ORANGE = '#f97316';

const linkStyle: React.CSSProperties = {
  color: ORANGE,
  textDecoration: 'none',
  fontWeight: 600,
};

const linkHover = (e: React.MouseEvent<HTMLAnchorElement>) => {
  e.currentTarget.style.color = '#fb923c';
};

const linkLeave = (e: React.MouseEvent<HTMLAnchorElement>) => {
  e.currentTarget.style.color = ORANGE;
};

const orangeSep = (
  <span aria-hidden style={{ color: ORANGE }}>
    {' '}
    ·{' '}
  </span>
);

function ZyvorDevLink({ className = '' }: { className?: string }) {
  return (
    <a
      href={ZYVOR_URL}
      target="_blank"
      rel="noopener noreferrer"
      className={className}
      style={linkStyle}
      onMouseEnter={linkHover}
      onMouseLeave={linkLeave}
    >
      zyvor.dev
    </a>
  );
}

type BrandProps = {
  /** @deprecated Ignored in footer — product name is not shown. */
  product?: string;
  className?: string;
  style?: React.CSSProperties;
  includeCopyright?: boolean;
};

/** Orange brand line: zyvor.dev · © 2026 (no background bar). */
export function ZyvorBrandLine({
  className = '',
  style,
  includeCopyright = true,
}: BrandProps) {
  return (
    <span
      className={`zyvor-brand-line whitespace-normal ${className}`.trim()}
      style={{
        fontSize: '12px',
        lineHeight: 1.5,
        color: ORANGE,
        ...style,
      }}
    >
      <ZyvorDevLink />
      {includeCopyright ? (
        <>
          {orangeSep}
          <span style={{ color: ORANGE, fontWeight: 500 }}>{ZYVOR_COPY}</span>
        </>
      ) : null}
    </span>
  );
}

/** @deprecated Use ZyvorBrandLine */
export function ZyvorInline(props: BrandProps) {
  return <ZyvorBrandLine {...props} />;
}

type FooterProps = {
  className?: string;
  /** Host OS pretty name (e.g. Rocky Linux 9.4) — shown when provided. */
  hostOs?: string;
  /** @deprecated Ignored — footer is zyvor.dev · © 2026 only. */
  product?: string;
};

/** Page footer — transparent; orange brand line only. */
export function ZyvorFooter({ className = '', hostOs }: FooterProps) {
  return (
    <footer
      className={`zyvor-footer shrink-0 py-3 text-center bg-transparent border-0 ${className}`.trim()}
      style={{ marginTop: 'auto' }}
      role="contentinfo"
    >
      <ZyvorBrandLine />
      {hostOs ? (
        <div
          className="mt-1 text-xs text-[#6e6e73]"
          title="Daemon host operating system"
        >
          {hostOs}
        </div>
      ) : null}
    </footer>
  );
}

/** @deprecated Use ZyvorFooter or ZyvorBrandLine. */
export function ZyvorHelpStrip(_props: BrandProps) {
  return null;
}

/** Header: zyvor.dev link only. */
export function ZyvorLogoMark({ className = '' }: { className?: string }) {
  return (
    <a
      href={ZYVOR_URL}
      target="_blank"
      rel="noopener noreferrer"
      title="zyvor.dev"
      className={className}
      style={{
        fontWeight: 600,
        fontSize: '13px',
        color: ORANGE,
        textDecoration: 'none',
      }}
      onMouseEnter={linkHover}
      onMouseLeave={linkLeave}
    >
      zyvor.dev
    </a>
  );
}

export default ZyvorFooter;
