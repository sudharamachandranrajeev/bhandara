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

export interface Progress {
  done: number
  total: number
}

export interface Totals {
  /** Apparent size. */
  size: number
  /** Space used on disk; cloud-only placeholders count as 0. */
  alloc: number
  /** Files at any depth. */
  count: number
}

export type Item = { kind: 'dir' | 'file'; id: number }

export interface Entry extends Totals {
  item: Item
  name: string
  /** Unix seconds. */
  modified: number
}

export interface FolderView extends Totals {
  id: number
  name: string
  path: string
  crumbs: { id: number; name: string }[]
  modified: number
  mount: boolean
  entries: Entry[]
  /** Files under 1 MB directly in this folder, which are not listed. */
  small: Totals
}

export interface TypeStat extends Totals {
  ext: string
  listed: number
}

export interface FileView {
  id: number
  name: string
  path: string
  ext: string
  size: number
  alloc: number
  modified: number
}

export interface FileFilter {
  ext?: string
  minSize?: number
  modifiedBefore?: number
  limit?: number
  /** Filter and sort by space used on disk instead of apparent size. */
  onDisk?: boolean
}

export interface Summary extends Totals {
  root: string
  folders: number
  listedFiles: number
  partial: boolean
  unreadable: string[]
  unreadableCount: number
  scannedAt: number
}

export interface ScanRoot {
  name: string
  path: string
}

export interface DuplicateGroup {
  size: number
  wasted: number
  files: FileView[]
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

/** `scan` rejects with this when Android all-files access is missing. */
export const ACCESS_NEEDED = 'ACCESS_NEEDED'

export const isAndroid = /Android/i.test(navigator.userAgent)
/** Android cannot open a folder in another app. */
export const canOpenPaths = !isAndroid
/** Android has no trash for ordinary files, so delete is permanent there. */
export const deleteIsPermanent = isAndroid

export const api = {
  volumes: () => invoke<Volume[]>('volumes'),
  /** `null` when the platform exposes no disk counters. */
  throughput: () => invoke<Rates | null>('throughput'),
  requestStorageAccess: () => invoke<void>('request_storage_access'),
  eject: (mount: string) => invoke<void>('eject', { mount }),
  health: () => invoke<HealthStatus>('health'),
  installHealth: () => invoke<void>('install_health'),
  scanRoots: () => invoke<ScanRoot[]>('scan_roots'),
  scan: (root: string) => invoke<Summary>('scan', { root }),
  stopScan: () => invoke<void>('stop_scan'),
  scanSummary: () => invoke<Summary | null>('scan_summary'),
  folder: (id: number) => invoke<FolderView>('folder', { id }),
  fileTypes: () => invoke<TypeStat[]>('file_types'),
  files: (filter: FileFilter) => invoke<FileView[]>('files', { filter }),
  duplicates: () => invoke<DuplicateGroup[]>('duplicates'),
  exportDuplicates: (groups: number[][]) => invoke<string>('export_duplicates', { groups }),
  rescan: (id: number) => invoke<number>('rescan', { id }),
  deleteItem: (item: Item) => invoke<Totals>('delete_item', { item }),
  openItem: (item: Item) => invoke<void>('open_item', { item }),
  openRoot: (path: string) => invoke<void>('open_root', { path }),
  exclusions: () => invoke<string[]>('exclusions'),
  exclude: (id: number) => invoke<string[]>('exclude', { id }),
  include: (path: string) => invoke<string[]>('include', { path }),
  onProgress: (event: 'scan-progress' | 'duplicates-progress', handler: (progress: Progress) => void): Promise<UnlistenFn> =>
    listen<Progress>(event, (e) => handler(e.payload)),
}
