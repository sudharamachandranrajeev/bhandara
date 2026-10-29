import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type Kind = 'system' | 'home' | 'internal' | 'removable' | 'boot'
export type Level = 'normal' | 'warning' | 'critical'

export interface Volume {
  device: string
  kname: string
  name: string
  kind: Kind
  mount: string
  mounts: string[]
  fstype: string
  label: string
  model: string
  transport: string
  removable: boolean
  encrypted: boolean
  size: number
  used: number
  avail: number
  fraction: number
  level: Level
}

export interface Rates {
  read: number
  write: number
}

export interface Folder {
  path: string
  name: string
  bytes: number
}

export interface FolderUsage {
  total: number
  folders: Folder[]
}

export interface ScanProgress {
  done: number
  total: number
}

export type Verdict = 'good' | 'watch' | 'replace'

export interface DriveHealth {
  device: string
  model: string
  serial: string
  protocol: string
  rotational: boolean
  verdict: Verdict
  reasons: string[]
  temperature: number | null
  powerOnHours: number | null
  /** Percent of rated life used. */
  wear: number | null
  /** Bytes written over the drive's life. */
  written: number | null
}

export interface HealthReport {
  /** Unix seconds. */
  generated: number
  drives: DriveHealth[]
  errors: string[]
}

export interface HealthStatus {
  state: 'ok' | 'stale' | 'missing' | 'unsupported'
  report: HealthReport | null
  canInstall: boolean
}

/** `largestFolders` rejects with this when Android all-files access is missing. */
export const ACCESS_NEEDED = 'ACCESS_NEEDED'

/** Android cannot open a folder in another app, so rows are not clickable there. */
export const canOpenPaths = !/Android/i.test(navigator.userAgent)

export const api = {
  volumes: () => invoke<Volume[]>('volumes'),
  /** `null` when the platform exposes no disk counters. */
  throughput: () => invoke<Rates | null>('throughput'),
  largestFolders: (force = false) => invoke<FolderUsage>('largest_folders', { force }),
  requestStorageAccess: () => invoke<void>('request_storage_access'),
  onScanProgress: (handler: (progress: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>('folders-progress', (event) => handler(event.payload)),
  openPath: (path: string) => invoke<void>('open_path', { path }),
  eject: (mount: string) => invoke<void>('eject', { mount }),
  health: () => invoke<HealthStatus>('health'),
  installHealth: () => invoke<void>('install_health'),
}
