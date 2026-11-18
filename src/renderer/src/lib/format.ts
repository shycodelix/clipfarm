export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

export function isMac(): boolean {
  const platform = window.clipfarm?.platform ?? 'browser'
  if (platform === 'browser') return /Mac|iPhone|iPad/.test(navigator.platform)
  return platform === 'darwin'
}

export function timecode(seconds: number, fps = 30): string {
  const safe = Math.max(0, Number.isFinite(seconds) ? seconds : 0)
  const totalFrames = Math.round(safe * fps)
  const frames = totalFrames % fps
  const totalSeconds = Math.floor(totalFrames / fps)
  const s = totalSeconds % 60
  const m = Math.floor(totalSeconds / 60) % 60
  const h = Math.floor(totalSeconds / 3600)
  const pad = (value: number): string => String(value).padStart(2, '0')
  return `${pad(h)}:${pad(m)}:${pad(s)}:${pad(frames)}`
}

export function clock(seconds: number): string {
  const safe = Math.max(0, Number.isFinite(seconds) ? seconds : 0)
  const s = Math.floor(safe % 60)
  const m = Math.floor(safe / 60) % 60
  const h = Math.floor(safe / 3600)
  const pad = (value: number): string => String(value).padStart(2, '0')
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`
}

export function shortDuration(seconds: number): string {
  if (!seconds) return '—'
  const totalMinutes = Math.floor(seconds / 60)
  if (totalMinutes < 1) return `${seconds.toFixed(1)}s`
  return `${totalMinutes}m ${Math.round(seconds % 60)}s`
}

export function bytes(value: number): string {
  if (!value) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let size = value
  let unit = 0
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024
    unit += 1
  }
  return `${size.toFixed(size >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`
}

/** Turn "Mod+Shift+E" into the platform's key glyphs. */
export function shortcut(label: string): string {
  const mac = isMac()
  return label
    .split('+')
    .map((part) => {
      switch (part) {
        case 'Mod':
          return mac ? '⌘' : 'Ctrl'
        case 'Shift':
          return mac ? '⇧' : 'Shift'
        case 'Alt':
          return mac ? '⌥' : 'Alt'
        case 'Ctrl':
          return mac ? '⌃' : 'Ctrl'
        default:
          return part
      }
    })
    .join(mac ? '' : '+')
}

export function initials(name: string): string {
  return name
    .split(/[\s—_]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => word[0]?.toUpperCase() ?? '')
    .join('')
}

/** Stable pastel gradient per asset, used for thumbnails and clip bodies. */
export function gradientFor(seed: string): [string, string] {
  let hash = 0
  for (let index = 0; index < seed.length; index += 1) {
    hash = (hash * 31 + seed.charCodeAt(index)) % 360
  }
  const hue = hash
  return [`hsl(${hue} 42% 46%)`, `hsl(${(hue + 38) % 360} 38% 30%)`]
}
