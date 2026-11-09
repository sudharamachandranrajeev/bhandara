<script lang="ts">
  // Shodhana: build output that rebuilds itself, plus package caches.
  // Nothing is ticked at first; a test run lists exactly what would go; the
  // confirm box asks Trash or Delete permanently each time, Cancel focused.
  import {
    api,
    isAndroid,
    type Cache,
    type Candidate,
    type CleanMode,
    type CleanScan,
    type Receipt,
    type RemoveResult,
    type Rule,
    type RuleInfo,
    type ScanRoot,
    type Skip,
  } from './api'
  import { open } from '@tauri-apps/plugin-dialog'
  import { formatAge, formatBytes } from './format'
  import Icon from './Icon.svelte'

  const isWindows = /Windows/i.test(navigator.userAgent)

  let roots = $state<ScanRoot[]>([])
  let rules = $state<Record<string, RuleInfo>>({})
  let root = $state('')
  let scan = $state<CleanScan | null>(null)
  let scanning = $state(false)
  let progress = $state({ done: 0, total: 0 })
  let selected = $state<Set<number>>(new Set())
  let removed = $state<Set<number>>(new Set())
  let preview = $state(false)
  let working = $state(false)
  let result = $state<RemoveResult | null>(null)
  let error = $state('')
  let caches = $state<Cache[] | null>(null)
  let cacheBusy = $state('')
  let cacheAsk = $state('')
  let cacheLog = $state('')
  let receipts = $state<Receipt[]>([])
  let dialog = $state<HTMLDialogElement>()

  $effect(() => {
    api.scanRoots().then((r) => {
      roots = r
      root ||= r[0]?.path ?? ''
    })
    api.cleanRules().then((list) => (rules = Object.fromEntries(list.map((r) => [r.rule, r]))))
    loadCaches()
    loadReceipts()
    const off = api.onProgress('clean-progress', (p) => (progress = p))
    return () => {
      off.then((f) => f())
    }
  })

  /** A folder chosen with the dialog, shown as its own button. */
  const custom = $derived(root && !roots.some((r) => r.path === root) ? root : '')

  async function chooseFolder() {
    const picked = await open({ directory: true, defaultPath: root || undefined, title: 'Folder to search for build output' })
    if (typeof picked === 'string') root = picked
  }

  $effect(() => {
    if (preview && dialog && !dialog.open) dialog.showModal()
    if (!preview && dialog?.open) dialog.close()
  })

  const live = $derived(scan ? scan.candidates.filter((c) => !removed.has(c.id)) : [])
  const groups = $derived(
    Object.entries(
      live.reduce<Record<string, Candidate[]>>((acc, c) => {
        ;(acc[c.rule] ??= []).push(c)
        return acc
      }, {}),
    ).sort((a, b) => sum(b[1]) - sum(a[1])) as [Rule, Candidate[]][],
  )
  const chosen = $derived(live.filter((c) => selected.has(c.id)))
  const chosenBytes = $derived(sum(chosen))
  const removable = $derived(live.filter((c) => !c.skip))
  const kept = $derived(live.filter((c) => c.skip))

  function sum(list: Candidate[]) {
    return list.reduce((n, c) => n + c.bytes, 0)
  }

  function rel(path: string) {
    return scan && path.startsWith(scan.root) ? path.slice(scan.root.length).replace(/^[\\/]/, '') : path
  }

  function why(skip: Skip): string {
    switch (skip.reason) {
      case 'active':
        return skip.days === 0 ? 'Project changed today' : `Project changed ${skip.days} days ago`
      case 'gitTracked':
        return 'Git tracks files inside it'
      case 'running':
        return 'A program is running in this project'
      case 'synced':
        return `Synced by Syncthing (${skip.folder}) and not in its ignore list`
      case 'mountPoint':
        return 'It is or holds a mount point'
    }
  }

  async function find() {
    if (!root) return
    scanning = true
    error = ''
    result = null
    progress = { done: 0, total: 0 }
    selected = new Set()
    removed = new Set()
    try {
      scan = await api.cleanScan(root)
    } catch (e) {
      error = String(e)
    } finally {
      scanning = false
    }
  }

  function toggle(c: Candidate) {
    if (c.skip) return
    const next = new Set(selected)
    if (!next.delete(c.id)) next.add(c.id)
    selected = next
  }

  function selectGroup(list: Candidate[], on: boolean) {
    const next = new Set(selected)
    for (const c of list) if (!c.skip) on ? next.add(c.id) : next.delete(c.id)
    selected = next
  }

  async function remove(mode: CleanMode) {
    working = true
    error = ''
    try {
      const ids = chosen.map((c) => c.id)
      result = await api.cleanRemove(ids, mode)
      const gone = new Set(removed)
      for (const r of result.receipts) {
        const c = live.find((x) => x.path === r.path)
        if (c) gone.add(c.id)
      }
      removed = gone
      selected = new Set()
      preview = false
      loadReceipts()
    } catch (e) {
      error = String(e)
      preview = false
    } finally {
      working = false
    }
  }

  async function loadCaches() {
    caches = await api.cacheList().catch(() => [])
  }

  async function loadReceipts() {
    receipts = await api.receipts(30).catch(() => [])
  }

  async function runCache(cache: Cache) {
    cacheAsk = ''
    cacheBusy = cache.id
    cacheLog = ''
    try {
      const out = await api.cacheRun(cache.id)
      cacheLog = `${cache.label}: done.${out ? ` ${out.split('\n').slice(-2).join(' ')}` : ''}`
      await loadCaches()
    } catch (e) {
      cacheLog = String(e)
    } finally {
      cacheBusy = ''
    }
  }

  function copy(text: string) {
    navigator.clipboard?.writeText(text)
  }
</script>

<div class="clean">
  <section class="card">
    <h2>Build output</h2>
    <p class="dim lead">
      Folders that a build tool made and can make again. Each one is proven by a marker file, such as a pubspec.yaml next to build/. Folders a guard
      protects are listed but cannot be picked.
    </p>
    <div class="row">
      <div class="roots" role="group" aria-label="Folder to search">
        {#each roots as r (r.path)}
          <button class="root" class:active={root === r.path} onclick={() => (root = r.path)} disabled={scanning} title={r.path}>
            <Icon name={roots[0] === r ? 'home' : 'drive'} size={16} />
            {r.name}
          </button>
        {/each}
        {#if custom}
          <button class="root active" disabled={scanning} title={custom}>
            <Icon name="folder" size={16} />
            {custom.split(/[\\/]/).filter(Boolean).pop() ?? custom}
          </button>
        {/if}
        {#if !isAndroid}
          <button class="ghost" onclick={chooseFolder} disabled={scanning}>Choose folder…</button>
        {/if}
      </div>
      {#if scanning}
        <button class="ghost" onclick={() => api.cleanStop()}><Icon name="stop" /> Stop</button>
      {:else}
        <button class="primary" onclick={find} disabled={!root}><Icon name="refresh" /> {scan ? 'Search again' : 'Find build output'}</button>
      {/if}
    </div>
    {#if scanning}
      <p class="dim" role="status">
        {progress.total ? `Measuring ${progress.done} of ${progress.total} folders…` : 'Looking for build folders…'}
      </p>
    {/if}
  </section>

  {#if error}
    <p class="error" role="alert">{error} <button class="ghost" onclick={() => (error = '')} aria-label="Dismiss">✕</button></p>
  {/if}

  {#if result}
    <section class="card done" role="status">
      {#if result.receipts.length}
        {@const bytes = result.receipts.reduce((n, r) => n + r.bytes, 0)}
        {@const backed = result.receipts.reduce((n, r) => n + r.backedUp.length, 0)}
        <p>
          <strong>{result.receipts[0].mode === 'permanent' ? 'Removed' : 'Moved to Trash'}: {formatBytes(bytes)}</strong>
          from {result.receipts.length} folder{result.receipts.length === 1 ? '' : 's'}.
          {#if result.receipts[0].mode === 'trash'}The space comes back when the Trash is emptied.{/if}
          {#if backed}{backed} release package{backed === 1 ? ' was' : 's were'} copied to the backup folder first.{/if}
        </p>
      {/if}
      {#each result.failed as [path, why] (path)}
        <p class="warn">Not removed: {rel(path)}: {why}</p>
      {/each}
      {#if result.snapshot}
        {@const s = result.snapshot}
        <div class="snap">
          <p>
            <strong>Only {formatBytes(s.freed)} of {formatBytes(s.removed)} came back.</strong> This drive uses btrfs, and snapshots{s.snapshots !== null
              ? ` (${s.snapshots} found)`
              : ''} still hold the removed files. The space returns when those snapshots are deleted or expire. Bhandara never deletes snapshots.
          </p>
          {#each s.commands as cmd (cmd)}
            <div class="cmd"><code>{cmd}</code><button class="ghost" onclick={() => copy(cmd)} aria-label="Copy"><Icon name="copy" /></button></div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  {#if scan}
    <section class="card">
      <div class="line">
        <span>
          <strong>{formatBytes(sum(removable))}</strong> can be removed now · {formatBytes(sum(kept))} kept by guards · searched {formatAge(scan.scannedAt)}
          {#if scan.partial}<span class="warn"> · stopped early</span>{/if}
        </span>
        <div class="controls">
          <button class="ghost" onclick={() => selectGroup(live, true)} disabled={!removable.length}>Pick all allowed</button>
          <button class="ghost" onclick={() => (selected = new Set())} disabled={!selected.size}>Clear</button>
          <button class="primary" onclick={() => (preview = true)} disabled={!chosen.length}>Test run ({formatBytes(chosenBytes)})</button>
        </div>
      </div>

      {#if !live.length}
        <p class="dim">No build output found here.</p>
      {/if}

      {#each groups as [rule, list] (rule)}
        {@const info = rules[rule]}
        <details class="group" open>
          <summary>
            <span class="gname">{info?.label ?? rule}</span>
            <span class="dim">{list.length} · {formatBytes(sum(list))}</span>
          </summary>
          {#if info}
            <p class="dim small">{info.whySafe} {info.comesBack}</p>
          {/if}
          <ul>
            {#each list as c (c.id)}
              <li class:blocked={c.skip}>
                <label>
                  <input type="checkbox" checked={selected.has(c.id)} disabled={!!c.skip} onchange={() => toggle(c)} />
                  <span class="path" title={c.path}>{rel(c.path)}</span>
                  <span class="size">{formatBytes(c.bytes)}</span>
                </label>
                <span class="meta dim">
                  {#if c.skip}<span class="guard">{why(c.skip)}</span>{:else}project changed {formatAge(c.projectChanged)}{/if}
                  {#if c.releases.length}· {c.releases.length} release package{c.releases.length === 1 ? '' : 's'} inside{/if}
                </span>
              </li>
            {/each}
          </ul>
        </details>
      {/each}
    </section>
  {/if}

  <section class="card">
    <h2>Package caches</h2>
    <p class="dim lead">Each cache is cleaned by its own tool. Commands that need an administrator are shown for you to run in a terminal.</p>
    {#if caches === null}
      <p class="dim">Measuring caches…</p>
    {:else if !caches.length}
      <p class="dim">No known caches found.</p>
    {:else}
      <ul class="caches">
        {#each caches as c (c.id)}
          <li>
            <div class="cline">
              <span><strong>{c.label}</strong> {#if c.bytes !== null}<span class="dim">· {formatBytes(c.bytes)}</span>{/if}</span>
              {#if c.runnable}
                {#if cacheAsk === c.id}
                  <span class="ask">
                    <button class="ghost" onclick={() => (cacheAsk = '')}>Cancel</button>
                    <button class="danger" onclick={() => runCache(c)}>Run</button>
                  </span>
                {:else}
                  <button class="ghost" onclick={() => (cacheAsk = c.id)} disabled={!!cacheBusy}>{cacheBusy === c.id ? 'Cleaning…' : 'Clean'}</button>
                {/if}
              {:else}
                <button class="ghost" onclick={() => copy(c.command)}><Icon name="copy" /> Copy command</button>
              {/if}
            </div>
            <p class="dim small">{c.note}</p>
            <code>{c.command}</code>
          </li>
        {/each}
      </ul>
    {/if}
    {#if cacheLog}<p class="small" role="status">{cacheLog}</p>{/if}
    {#if isWindows}
      <div class="row">
        <button class="ghost" onclick={() => api.openSystemCleanup('storage-sense')}>Open Storage Sense</button>
        <button class="ghost" onclick={() => api.openSystemCleanup('disk-cleanup')}>Open Disk Cleanup</button>
      </div>
    {/if}
  </section>

  {#if receipts.length}
    <details class="card">
      <summary><h2>Receipts</h2></summary>
      <ul class="receipts">
        {#each receipts as r (r.at + r.path)}
          <li>
            <span class="dim">{new Date(r.at * 1000).toLocaleString()}</span>
            <span class="path" title={r.path}>{r.path}</span>
            <span>{formatBytes(r.bytes)} · {r.mode === 'permanent' ? 'deleted' : 'to Trash'}{r.scheduled ? ' · scheduled' : ''}</span>
          </li>
        {/each}
      </ul>
    </details>
  {/if}
</div>

<dialog bind:this={dialog} onclose={() => (preview = false)} aria-labelledby="clean-title">
  {#if preview}
    {@const releases = chosen.reduce((n, c) => n + c.releases.length, 0)}
    <h3 id="clean-title">Test run: {chosen.length} folder{chosen.length === 1 ? '' : 's'}, {formatBytes(chosenBytes)}</h3>
    <p class="dim small">Nothing has been removed yet. These exact folders would go:</p>
    <ul class="preview">
      {#each chosen as c (c.id)}
        <li><span class="path">{c.path}</span><span class="size">{formatBytes(c.bytes)}</span></li>
      {/each}
    </ul>
    {#if releases}
      <p class="small">{releases} release package{releases === 1 ? '' : 's'} (APK, AAB, installers) will be copied to the backup folder first, if that setting is on.</p>
    {/if}
    <p class="small">
      <strong>Move to Trash</strong> can be undone, but frees no space until the Trash is emptied.
      {#if !isAndroid}<strong>Delete permanently</strong> frees the space now and cannot be undone; the next build makes these folders again.{/if}
    </p>
    <div class="actions">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="ghost cancel" autofocus onclick={() => (preview = false)} disabled={working}>Cancel</button>
      {#if !isAndroid}
        <button class="ghost" onclick={() => remove('trash')} disabled={working}>Move to Trash</button>
      {/if}
      <button class="danger" onclick={() => remove('permanent')} disabled={working}>{working ? 'Working…' : 'Delete permanently'}</button>
    </div>
  {/if}
</dialog>

<style>
  .clean {
    display: grid;
    gap: 16px;
  }
  .lead {
    margin: 4px 0 12px;
    font-size: 13px;
  }
  .row,
  .line,
  .controls,
  .cline {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .line,
  .cline {
    justify-content: space-between;
  }
  .roots {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    flex: 1;
  }
  .root {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .root.active {
    border-color: var(--normal-a);
    color: var(--normal-a);
  }
  .dim {
    color: var(--dim);
  }
  .small {
    font-size: 12px;
  }
  .warn {
    color: var(--warning-a);
  }
  .error {
    margin: 0;
    padding: 10px 14px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--critical-a) 15%, transparent);
    color: var(--critical-a);
  }
  .group {
    margin-top: 12px;
    border-top: 1px solid var(--card-border);
    padding-top: 10px;
  }
  summary {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    cursor: pointer;
  }
  summary h2 {
    display: inline;
    margin: 0;
  }
  .gname {
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  li label {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }
  li.blocked label {
    cursor: default;
    opacity: 0.7;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .size {
    font-variant-numeric: tabular-nums;
    font-size: 13px;
    white-space: nowrap;
  }
  .meta {
    display: block;
    margin-left: 28px;
    font-size: 12px;
  }
  .guard {
    color: var(--warning-a);
  }
  .caches li {
    padding: 10px 0;
    border-top: 1px solid var(--card-border);
  }
  .caches p {
    margin: 4px 0;
  }
  code {
    font-size: 12px;
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--button);
    word-break: break-all;
  }
  .ask {
    display: flex;
    gap: 6px;
  }
  .done p {
    margin: 4px 0;
  }
  .snap {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--warning-a) 12%, transparent);
  }
  .cmd {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }
  .receipts li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 10px;
    font-size: 12px;
  }
  dialog {
    width: min(620px, calc(100vw - 32px));
    padding: 22px;
    border: 1px solid var(--card-border);
    border-radius: 20px;
    background: var(--bg);
    color: var(--text);
    box-shadow: 0 30px 80px -30px rgba(0, 0, 0, 0.6);
  }
  dialog::backdrop {
    background: rgba(5, 8, 20, 0.45);
    backdrop-filter: blur(4px);
  }
  h3 {
    margin: 0 0 8px;
    font-size: 18px;
  }
  .preview {
    max-height: 40vh;
    overflow: auto;
    margin: 8px 0;
  }
  .preview li {
    display: flex;
    gap: 10px;
  }
  .preview .path {
    white-space: normal;
    word-break: break-all;
    font-size: 12px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 18px;
    flex-wrap: wrap;
  }
  .cancel {
    background: var(--button);
  }
  .danger {
    padding: 9px 16px;
    border-radius: 10px;
    font-weight: 600;
    color: #fff;
    background: linear-gradient(135deg, var(--critical-a), var(--critical-b));
  }
</style>
