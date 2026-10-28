import { invoke } from '@tauri-apps/api/core'

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

export const api = {
  volumes: () => invoke<Volume[]>('volumes'),
  /** `null` when the platform exposes no disk counters. */
  throughput: () => invoke<Rates | null>('throughput'),
  largestFolders: (force = false) => invoke<FolderUsage>('largest_folders', { force }),
  openPath: (path: string) => invoke<void>('open_path', { path }),
  eject: (mount: string) => invoke<void>('eject', { mount }),
}
