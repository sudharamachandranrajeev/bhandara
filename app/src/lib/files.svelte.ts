import {
  ACCESS_NEEDED,
  api,
  deleteIsPermanent,
  type DuplicateGroup,
  type FileFilter,
  type FileView,
  type FolderView,
  type Item,
  type Progress,
  type ScanRoot,
  type Summary,
  type Totals,
  type TypeStat,
} from './api'

export type FilesTab = 'folders' | 'types' | 'largest' | 'duplicates'
/** `alloc`: space used on disk. `size`: what the files say they hold. */
export type SizeMode = 'alloc' | 'size'

export interface PendingDelete {
  item: Item
  name: string
  path: string
  bytes: number
}

const LARGEST_LIMIT = 300

/** The Files view (Suchi): one scanned folder tree and what is shown of it. */
export class Files {
  roots = $state<ScanRoot[]>([])
  summary = $state<Summary | null>(null)
  /** The scan root's own listing, for the overview card. */
  top = $state<FolderView | null>(null)
  /** The folder being browsed. */
  folder = $state<FolderView | null>(null)
  tab = $state<FilesTab>('folders')
  sizeMode = $state<SizeMode>('alloc')

  scanning = $state(false)
  scanningRoot = $state('')
  progress = $state<Progress>({ done: 0, total: 0 })
  /** Android: nothing can be scanned until all-files access is granted. */
  needsAccess = $state(false)
  error = $state('')
  notice = $state('')

  types = $state<TypeStat[]>([])
  filter = $state<FileFilter>({ minSize: 0 })
  largest = $state<FileView[]>([])
  duplicates = $state<DuplicateGroup[] | null>(null)
  findingDuplicates = $state(false)
  duplicateProgress = $state<Progress>({ done: 0, total: 0 })

  excluded = $state<string[]>([])
  /** Waiting for the user to confirm in the delete dialog. */
  pending = $state<PendingDelete | null>(null)
  deleting = $state(false)

  bytes = (totals: Pick<Totals, 'size' | 'alloc'>) => totals[this.sizeMode]

  setSizeMode(mode: SizeMode) {
    this.sizeMode = mode
    if (this.tab === 'largest') this.loadLargest()
  }

  /** Loads what is already scanned and listens for progress; returns a stop function. */
  start(): () => void {
    const stops = [
      api.onProgress('scan-progress', (p) => (this.progress = p)),
      api.onProgress('duplicates-progress', (p) => (this.duplicateProgress = p)),
    ]
    this.load()
    const onVisible = () => {
      // Coming back from Android's access settings screen: try again.
      if (document.visibilityState === 'visible' && this.needsAccess) this.scan()
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => {
      stops.forEach((stop) => stop.then((unlisten) => unlisten()))
      document.removeEventListener('visibilitychange', onVisible)
    }
  }

  private async load() {
    try {
      this.roots = await api.scanRoots()
      this.excluded = await api.exclusions()
      this.summary = await api.scanSummary()
      if (this.summary) await this.afterChange(0)
      else await this.scan()
    } catch (e) {
      this.error = String(e)
    }
  }

  /** Shows `root`, scanning it unless it is already the scanned folder. */
  async show(root: string) {
    this.tab = 'folders'
    // Volumes apps cannot read (Android's /data) fall back to the first root.
    if (this.roots.length && !this.roots.some((r) => r.path === root)) root = this.roots[0].path
    if (this.summary?.root === root && !this.scanning) await this.openFolder(0)
    else await this.scan(root)
  }

  async scan(root = this.summary?.root ?? this.roots[0]?.path) {
    if (!root || this.scanning) return
    this.scanning = true
    this.scanningRoot = root
    this.progress = { done: 0, total: 0 }
    this.error = ''
    try {
      this.summary = await api.scan(root)
      this.needsAccess = false
      this.duplicates = null
      await this.afterChange(0)
    } catch (e) {
      if (e === ACCESS_NEEDED) this.needsAccess = true
      else this.error = String(e)
    } finally {
      this.scanning = false
    }
  }

  stop() {
    api.stopScan()
  }

  requestAccess() {
    api.requestStorageAccess().catch((e) => (this.error = String(e)))
  }

  async openFolder(id: number) {
    try {
      this.folder = await api.folder(id)
      this.tab = 'folders'
    } catch (e) {
      this.error = String(e)
    }
  }

  async setTab(tab: FilesTab) {
    this.tab = tab
    if (tab === 'types') this.types = await api.fileTypes().catch(() => [])
    if (tab === 'largest') await this.loadLargest()
  }

  async loadLargest() {
    this.largest = await api.files({ ...this.filter, limit: LARGEST_LIMIT, onDisk: this.sizeMode === 'alloc' }).catch(() => [])
  }

  /** Lists the files of one type in the Largest tab. */
  async showType(ext: string) {
    this.filter = { ...this.filter, ext }
    await this.setTab('largest')
  }

  async findDuplicates() {
    this.findingDuplicates = true
    this.duplicateProgress = { done: 0, total: 0 }
    try {
      this.duplicates = await api.duplicates()
    } catch (e) {
      this.error = String(e)
    } finally {
      this.findingDuplicates = false
    }
  }

  async exportDuplicates() {
    if (!this.duplicates) return
    try {
      const path = await api.exportDuplicates(this.duplicates.map((g) => g.files.map((f) => f.id)))
      this.notice = `Saved the list to ${path}`
    } catch (e) {
      this.error = String(e)
    }
  }

  async rescan(id: number) {
    if (this.scanning) return
    this.scanning = true
    this.scanningRoot = ''
    try {
      const fresh = await api.rescan(id)
      await this.afterChange(this.folder?.id === id ? fresh : (this.folder?.id ?? 0))
    } catch (e) {
      this.error = String(e)
    } finally {
      this.scanning = false
    }
  }

  open(item: Item) {
    api.openItem(item).catch((e) => (this.error = String(e)))
  }

  async exclude(id: number) {
    try {
      this.excluded = await api.exclude(id)
      this.notice = 'Excluded. It will be left out from the next scan.'
    } catch (e) {
      this.error = String(e)
    }
  }

  async include(path: string) {
    this.excluded = await api.include(path).catch(() => this.excluded)
  }

  askDelete(item: Item, name: string, path: string, bytes: number) {
    this.pending = { item, name, path, bytes }
  }

  async confirmDelete() {
    const pending = this.pending
    if (!pending) return
    this.deleting = true
    try {
      await api.deleteItem(pending.item)
      this.notice = `${pending.name} ${deleteIsPermanent ? 'was deleted' : 'is in the Trash'}`
      if (this.duplicates && pending.item.kind === 'file') {
        this.duplicates = this.duplicates
          .map((g) => ({ ...g, files: g.files.filter((f) => f.id !== pending.item.id) }))
          .filter((g) => g.files.length > 1)
          .map((g) => ({ ...g, wasted: g.size * (g.files.length - 1) }))
      }
      await this.afterChange(this.folder?.id ?? 0)
    } catch (e) {
      this.error = `Couldn't delete ${pending.name}: ${e}`
    } finally {
      this.deleting = false
      this.pending = null
    }
  }

  /** Reloads everything shown after a scan, rescan or delete. */
  private async afterChange(folderId: number) {
    this.summary = await api.scanSummary()
    this.top = await api.folder(0).catch(() => null)
    this.folder = await api.folder(folderId).catch(() => this.top)
    if (this.tab === 'types') this.types = await api.fileTypes().catch(() => [])
    if (this.tab === 'largest') await this.loadLargest()
  }
}
