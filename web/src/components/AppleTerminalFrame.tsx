// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode, UIEventHandler, Ref, TextareaHTMLAttributes } from 'react'

/** Shared styles for monospace editors inside AppleTerminalFrame.
 *  Avoid Tailwind `selection:bg-[#0a…]` / `bg-[#0…]` class substrings — light-theme
 *  compatibility remaps those to canvas white and wash out the terminal. */
export const TERM_TEXTAREA_CLASS =
  'zf-terminal-input w-full border-0 px-3 py-2 text-[12px] leading-[1.45] focus:outline-none focus:ring-0 resize-y disabled:opacity-50 read-only:opacity-80'

export const TERM_FONT_STYLE = {
  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
} as const

type AppleTerminalFrameProps = {
  title: string
  live?: boolean
  trailing?: ReactNode
  /** Height / scroll container class for the body (default max-h). */
  bodyClassName?: string
  className?: string
  children?: ReactNode
  bodyRef?: Ref<HTMLDivElement>
  onBodyScroll?: UIEventHandler<HTMLDivElement>
  empty?: boolean
  emptyMessage?: string
}

/** macOS Terminal.app-style chrome: always black, even in light theme. */
export function AppleTerminalFrame({
  title,
  live,
  trailing,
  bodyClassName = 'max-h-[36rem] overflow-y-auto px-3 py-2',
  className = '',
  children,
  bodyRef,
  onBodyScroll,
  empty,
  emptyMessage = 'No output yet',
}: AppleTerminalFrameProps) {
  return (
    <div className={`zf-terminal rounded-xl overflow-hidden ${className}`.trim()}>
      <div className="zf-terminal-titlebar flex items-center gap-2 px-4 py-2.5">
        <span className="w-3 h-3 rounded-full bg-[#ff5f57]" aria-hidden />
        <span className="w-3 h-3 rounded-full bg-[#febc2e]" aria-hidden />
        <span className="w-3 h-3 rounded-full bg-[#28c840]" aria-hidden />
        <span className="zf-terminal-title ml-3 text-xs font-medium truncate tracking-wide min-w-0 flex-1">
          {title}
        </span>
        {live && (
          <span className="zf-terminal-live text-xs uppercase tracking-wider shrink-0">Live</span>
        )}
        {trailing}
      </div>
      <div
        ref={bodyRef}
        onScroll={onBodyScroll}
        className={`zf-terminal-body font-mono text-[12px] leading-[1.45] ${bodyClassName}`}
        style={TERM_FONT_STYLE}
      >
        {empty ? (
          <div className="zf-terminal-empty p-8 text-center text-sm">{emptyMessage}</div>
        ) : (
          children
        )}
      </div>
    </div>
  )
}

export const TERM_LEVEL_COLOR: Record<string, string> = {
  INFO: '#64d2ff',
  info: '#64d2ff',
  WARN: '#ffd60a',
  WARNING: '#ffd60a',
  warning: '#ffd60a',
  ERROR: '#ff453a',
  error: '#ff453a',
  CRITICAL: '#ff453a',
  DEBUG: '#8e8e93',
  debug: '#8e8e93',
}

type TerminalTextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & {
  title: string
  trailing?: ReactNode
  frameClassName?: string
}

/** Editable code/YAML/JSON surface with Terminal.app chrome. */
export function TerminalTextarea({
  title,
  trailing,
  frameClassName,
  className = '',
  style,
  spellCheck = false,
  ...textareaProps
}: TerminalTextareaProps) {
  return (
    <AppleTerminalFrame title={title} trailing={trailing} bodyClassName="p-0" className={frameClassName}>
      <textarea
        {...textareaProps}
        spellCheck={spellCheck}
        className={`${TERM_TEXTAREA_CLASS} ${className}`.trim()}
        style={{ ...TERM_FONT_STYLE, ...style }}
      />
    </AppleTerminalFrame>
  )
}
