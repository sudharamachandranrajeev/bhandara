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
