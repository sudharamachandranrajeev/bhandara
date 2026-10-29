// Display helpers, matching omarchy/bhandara.bar/Model.js so every front end
// shows the same numbers.

const UNITS = ['B', 'kB', 'MB', 'GB', 'TB', 'PB']

export function formatBytes(bytes: number): string {
  let value = Number(bytes) || 0
  let unit = 0
  while (Math.abs(value) >= 1000 && unit < UNITS.length - 1) {
    value /= 1000
    unit++
  }
  if (unit === 0) return `${Math.round(value)} B`
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${UNITS[unit]}`
}

export function formatRate(bytesPerSecond: number): string {
  const value = Math.max(0, Number(bytesPerSecond) || 0)
  return value < 1000 ? '0 B/s' : `${formatBytes(value)}/s`
}

export function percent(fraction: number): string {
  return `${Math.round((Number(fraction) || 0) * 100)}%`
}

/** `/run/media/<user>/STICK` reads as `STICK`. */
export function shortMount(path: string): string {
  const match = /^\/run\/media\/[^/]+\/(.+)$/.exec(path)
  return match ? match[1] : path
}

const AGE = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

/** "today", "3 days ago", "5 months ago", "2 years ago". */
export function formatAge(unixSeconds: number): string {
  if (!unixSeconds) return ''
  const days = Math.floor((Date.now() / 1000 - unixSeconds) / 86_400)
  if (days < 1) return 'today'
  if (days < 31) return AGE.format(-days, 'day')
  if (days < 365) return AGE.format(-Math.floor(days / 30.4), 'month')
  return AGE.format(-Math.floor(days / 365), 'year')
}

/** Share of `whole`, as "42%" or "<1%". */
export function share(part: number, whole: number): string {
  if (!whole) return ''
  const pct = (part / whole) * 100
  return pct > 0 && pct < 1 ? '<1%' : `${Math.round(pct)}%`
}
