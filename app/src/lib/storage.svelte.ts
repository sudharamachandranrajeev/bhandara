import { api, type FolderUsage, type Volume } from './api'

const HISTORY = 60

/** Live storage state, polled while any view is mounted. */
export class Storage {
  volumes = $state<Volume[]>([])
  folders = $state<FolderUsage>({ total: 0, folders: [] })
  readHistory = $state<number[]>([])
  writeHistory = $state<number[]>([])
  read = $state(0)
  write = $state(0)
  /** False when the platform exposes no throughput counters. */
  hasThroughput = $state(true)
  scanning = $state(false)
  error = $state('')
  ejecting = $state('')

  system = $derived(this.volumes.find((v) => v.kind === 'system') ?? this.volumes[0])
  busy = $derived(this.read + this.write > 256 * 1024)

  async refreshVolumes() {
    try {
      this.volumes = await api.volumes()
      this.error = ''
    } catch (e) {
      this.error = String(e)
    }
  }

  async refreshFolders(force = false) {
    if (this.scanning) return
    this.scanning = true
    try {
      this.folders = await api.largestFolders(force)
    } catch (e) {
      this.error = String(e)
    } finally {
      this.scanning = false
    }
  }

  async sampleThroughput() {
    const rates = await api.throughput().catch(() => null)
    this.hasThroughput = rates !== null
    if (!rates) return
    this.read = rates.read
    this.write = rates.write
    this.readHistory = [...this.readHistory, rates.read].slice(-HISTORY)
    this.writeHistory = [...this.writeHistory, rates.write].slice(-HISTORY)
  }

  async eject(volume: Volume) {
    this.ejecting = volume.mount
    try {
      await api.eject(volume.mount)
    } catch (e) {
      this.error = `Couldn't eject ${volume.name}: ${e}`
    } finally {
      this.ejecting = ''
      await this.refreshVolumes()
    }
  }

  open(path: string) {
    api.openPath(path).catch((e) => (this.error = String(e)))
  }

  /** Starts polling; returns a function that stops it. */
  start(): () => void {
    this.refreshVolumes().then(() => this.sampleThroughput())
    this.refreshFolders()
    const timers = [
      setInterval(() => this.sampleThroughput(), 1000),
      setInterval(() => this.refreshVolumes(), 5000),
      setInterval(() => this.refreshFolders(), 60_000),
    ]
    return () => timers.forEach(clearInterval)
  }
}
