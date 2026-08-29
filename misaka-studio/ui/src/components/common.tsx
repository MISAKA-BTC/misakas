// Shared primitives and icons.
//
// Icons are inline SVG rather than an icon package: there are fourteen of them, they are 20 lines
// each, and a desktop app should not ship a thousand-icon font to draw a plus sign.

import { useEffect, useState, type ReactNode } from 'react'
import type { FitVerdict, Quantization } from '../lib/types'

export function Icon({ name, className = 'size-4' }: { name: IconName; className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.75} strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden>
      {PATHS[name]}
    </svg>
  )
}

export type IconName = keyof typeof PATHS

const PATHS = {
  chat: <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />,
  cube: (
    <>
      <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
      <path d="m3.3 7 8.7 5 8.7-5M12 22V12" />
    </>
  ),
  globe: (
    <>
      <circle cx="12" cy="12" r="10" />
      <path d="M2 12h20" />
      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
    </>
  ),
  gauge: (
    <>
      <path d="M12 14 8 9" />
      <path d="M3.3 17A9 9 0 1 1 20.7 17" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6 1.65 1.65 0 0 0 10 3.09V3a2 2 0 0 1 4 0v.09A1.65 1.65 0 0 0 15 4.6a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  send: <path d="m22 2-7 20-4-9-9-4z" />,
  stop: <rect x="6" y="6" width="12" height="12" rx="2" />,
  copy: (
    <>
      <rect x="9" y="9" width="13" height="13" rx="2" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </>
  ),
  check: <path d="M20 6 9 17l-5-5" />,
  refresh: (
    <>
      <path d="M21 12a9 9 0 1 1-3-6.7L21 8" />
      <path d="M21 3v5h-5" />
    </>
  ),
  trash: <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />,
  download: <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" />,
  search: (
    <>
      <circle cx="11" cy="11" r="8" />
      <path d="m21 21-4.3-4.3" />
    </>
  ),
  edit: <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4z" />,
  shield: <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />,
  x: <path d="M18 6 6 18M6 6l12 12" />,
  chevron: <path d="m9 18 6-6-6-6" />,
  power: (
    <>
      <path d="M18.36 6.64a9 9 0 1 1-12.73 0" />
      <path d="M12 2v10" />
    </>
  ),
  warning: (
    <>
      <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
      <path d="M12 9v4M12 17h.01" />
    </>
  ),
} as const

export function Spinner({ className = 'size-4' }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={`${className} animate-spin`} fill="none" aria-hidden>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  )
}

/** Copies text, and says so. The confirmation is the point — a copy button with no feedback gets
 *  pressed three times. */
export function CopyButton({ text, label = 'Copy', className = 'btn-ghost' }: { text: string; label?: string; className?: string }) {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), 1500)
    return () => clearTimeout(timer)
  }, [copied])

  return (
    <button
      type="button"
      className={className}
      title={label}
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text)
          setCopied(true)
        } catch {
          // Clipboard access can be denied; a button that silently does nothing is worse than one
          // that visibly fails.
          setCopied(false)
        }
      }}
    >
      <Icon name={copied ? 'check' : 'copy'} className="size-3.5" />
      <span className="sr-only">{label}</span>
    </button>
  )
}

export function QuantBadge({ quantization }: { quantization: Quantization | null }) {
  if (!quantization) return <span className="badge bg-ink-100 text-ink-500 dark:bg-ink-800 dark:text-ink-400">unquantized?</span>
  const tone: Record<Quantization['tier'], string> = {
    lossless: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300',
    recommended: 'bg-arc-500/15 text-arc-700 dark:text-arc-300',
    compact: 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300',
    aggressive: 'bg-red-100 text-red-800 dark:bg-red-950/60 dark:text-red-300',
    unknown: 'bg-ink-100 text-ink-600 dark:bg-ink-800 dark:text-ink-400',
  }
  return (
    <span className={`badge mono ${tone[quantization.tier]}`} title={`${quantization.family} · ${quantization.bits_per_weight ?? '?'} bits per weight`}>
      {quantization.label}
    </span>
  )
}

export function FitBadge({ fit, summary }: { fit: FitVerdict; summary: string }) {
  const tone =
    fit.verdict === 'fits'
      ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300'
      : fit.verdict === 'tight'
        ? 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300'
        : fit.verdict === 'partial_offload'
          ? 'bg-orange-100 text-orange-800 dark:bg-orange-950/60 dark:text-orange-300'
          : 'bg-red-100 text-red-800 dark:bg-red-950/60 dark:text-red-300'
  return (
    <span className={`badge ${tone}`} title={summary}>
      {summary}
    </span>
  )
}

export function Section({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <section className="card p-5">
      <h3 className="text-sm font-semibold">{title}</h3>
      {description && <p className="mt-1 text-xs text-ink-500 dark:text-ink-400">{description}</p>}
      <div className="mt-4 space-y-4">{children}</div>
    </section>
  )
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="text-xs font-medium text-ink-600 dark:text-ink-300">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-[0.7rem] text-ink-500 dark:text-ink-400">{hint}</span>}
    </label>
  )
}

/** A labelled slider that also shows its value — a slider whose number is invisible is a slider
 *  nobody can set deliberately. */
export function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
  hint,
  format,
}: {
  label: string
  value: number
  min: number
  max: number
  step: number
  onChange: (value: number) => void
  hint?: string
  format?: (value: number) => string
}) {
  return (
    <div>
      <div className="flex items-baseline justify-between">
        <span className="text-xs font-medium text-ink-600 dark:text-ink-300">{label}</span>
        <span className="mono text-xs text-ink-500 dark:text-ink-400">{format ? format(value) : value}</span>
      </div>
      <input
        type="range"
        className="mt-1.5 w-full accent-arc-600"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      {hint && <p className="mt-1 text-[0.7rem] text-ink-500 dark:text-ink-400">{hint}</p>}
    </div>
  )
}

export function Toggle({ label, checked, onChange, hint }: { label: string; checked: boolean; onChange: (v: boolean) => void; hint?: string }) {
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <input type="checkbox" className="mt-0.5 size-4 accent-arc-600" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>
        <span className="block text-xs font-medium text-ink-700 dark:text-ink-200">{label}</span>
        {hint && <span className="mt-0.5 block text-[0.7rem] text-ink-500 dark:text-ink-400">{hint}</span>}
      </span>
    </label>
  )
}

export function Stat({ label, value, sub }: { label: string; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className="card p-4">
      <div className="text-[0.7rem] uppercase tracking-wide text-ink-500 dark:text-ink-400">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums">{value}</div>
      {sub && <div className="mt-0.5 text-xs text-ink-500 dark:text-ink-400">{sub}</div>}
    </div>
  )
}

/** A horizontal meter. `value` and `max` in the same units; the caller formats the caption. */
export function Meter({ value, max, caption, tone = 'arc' }: { value: number; max: number; caption?: ReactNode; tone?: 'arc' | 'warn' }) {
  const percent = max > 0 ? Math.min(100, (value / max) * 100) : 0
  const bar = tone === 'warn' || percent > 90 ? 'bg-amber-500' : 'bg-arc-500'
  return (
    <div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-ink-200 dark:bg-ink-800">
        <div className={`h-full rounded-full ${bar} transition-[width] duration-500`} style={{ width: `${percent}%` }} />
      </div>
      {caption && <div className="mt-1 text-xs text-ink-500 dark:text-ink-400">{caption}</div>}
    </div>
  )
}

export function EmptyState({ icon, title, children }: { icon: IconName; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="rounded-2xl bg-ink-100 p-4 text-ink-400 dark:bg-ink-900 dark:text-ink-500">
        <Icon name={icon} className="size-7" />
      </div>
      <h3 className="text-base font-semibold">{title}</h3>
      <div className="max-w-md text-sm text-ink-500 dark:text-ink-400">{children}</div>
    </div>
  )
}
