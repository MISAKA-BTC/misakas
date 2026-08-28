// Formatting helpers. Small, and worth having in one place: a model list that says "4.7 GB" in
// one column and "4700000000" in another is a list nobody trusts.

/** Bytes in the units a person reads. Binary units, because that is what memory is measured in. */
export function bytes(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  if (value < 1024) return `${value} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = value / 1024
  let unit = 0
  while (v >= 1024 && unit < units.length - 1) {
    v /= 1024
    unit++
  }
  return `${v.toFixed(v >= 100 ? 0 : digits)} ${units[unit]}`
}

/** Parameter counts: 3.8B, 671B, 14.2M. */
export function params(value: number | null | undefined): string {
  if (!value) return '—'
  if (value >= 1e12) return `${(value / 1e12).toFixed(1)}T`
  if (value >= 1e9) return `${(value / 1e9).toFixed(value >= 1e10 ? 0 : 1)}B`
  if (value >= 1e6) return `${(value / 1e6).toFixed(1)}M`
  return String(value)
}

/** Context lengths: 128K, 32K, 4096. */
export function tokens(value: number | null | undefined): string {
  if (!value) return '—'
  if (value >= 1024 && value % 1024 === 0) return `${value / 1024}K`
  return value.toLocaleString()
}

export function count(value: number | null | undefined): string {
  if (value === null || value === undefined) return '—'
  if (value >= 1e6) return `${(value / 1e6).toFixed(1)}M`
  if (value >= 1e3) return `${(value / 1e3).toFixed(1)}k`
  return String(value)
}

export function duration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '—'
  if (ms < 1000) return `${Math.round(ms)} ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`
  const minutes = Math.floor(ms / 60_000)
  const seconds = Math.round((ms % 60_000) / 1000)
  return `${minutes}m ${seconds}s`
}

/** Seconds remaining, as an ETA. */
export function eta(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) return '—'
  if (seconds < 60) return `${Math.round(seconds)}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`
  return `${Math.floor(seconds / 3600)}h ${Math.round((seconds % 3600) / 60)}m`
}

export function rate(bytesPerSecond: number): string {
  if (!bytesPerSecond || bytesPerSecond < 1) return '—'
  return `${bytes(bytesPerSecond, 1)}/s`
}

/** A 128-character digest, shortened for display. Never for comparison. */
export function shortHash(hash: string | null | undefined, head = 8, tail = 6): string {
  if (!hash) return '—'
  if (hash.length <= head + tail + 1) return hash
  return `${hash.slice(0, head)}…${hash.slice(-tail)}`
}

export function relativeTime(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return '—'
  const delta = Date.now() / 1000 - unixSeconds
  if (delta < 60) return 'just now'
  if (delta < 3600) return `${Math.floor(delta / 60)} min ago`
  if (delta < 86_400) return `${Math.floor(delta / 3600)} h ago`
  if (delta < 2_592_000) return `${Math.floor(delta / 86_400)} d ago`
  return new Date(unixSeconds * 1000).toLocaleDateString()
}
