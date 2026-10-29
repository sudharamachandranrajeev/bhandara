import { api, type HealthStatus, type Volume } from './api'

/** Two minutes of one-second samples. */
const HISTORY = 120
/** Share of each new sample in the smoothed rate: lower is calmer. */
const SMOOTHING = 0.3

/** Live storage state, polled while any view is mounted. */
export class Storage {
  volumes = $state<Volume[]>([])
  readHistory = $state<number[]>([])
  writeHistory = $state<number[]>([])
  read = $state(0)
  write = $state(0)
  /** False when the platform exposes no throughput counters. */
  hasThroughput = $state(true)
  error = $state('')
  ejecting = $state('')
  health = $state<HealthStatus | null>(null)
  installingHealth = $state(false)

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

  async sampleThroughput() {
    const rates = await api.throughput().catch(() => null)
    this.hasThroughput = rates !== null
    if (!rates) return
    // Disk counters jump from one second to the next; an exponential
    // moving average turns that into a readable trend.
    const first = this.readHistory.length === 0
    this.read = first ? rates.read : this.read + SMOOTHING * (rates.read - this.read)
    this.write = first ? rates.write : this.write + SMOOTHING * (rates.write - this.write)
    this.readHistory = [...this.readHistory, this.read].slice(-HISTORY)
    this.writeHistory = [...this.writeHistory, this.write].slice(-HISTORY)
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

  async refreshHealth() {
    this.health = await api.health().catch(() => null)
  }

  async installHealth() {
    this.installingHealth = true
    try {
      await api.installHealth()
      // The helper's first run takes a few seconds.
      setTimeout(() => this.refreshHealth(), 5000)
    } catch (e) {
      this.error = String(e)
    } finally {
      this.installingHealth = false
    }
  }

  /** Starts polling; returns a function that stops it. */
  start(): () => void {
    this.refreshVolumes().then(() => this.sampleThroughput())
    this.refreshHealth()
    const timers = [
      setInterval(() => this.refreshHealth(), 60_000),
      setInterval(() => this.sampleThroughput(), 1000),
      setInterval(() => this.refreshVolumes(), 5000),
    ]
    return () => {
      timers.forEach(clearInterval)
    }
  }
}
