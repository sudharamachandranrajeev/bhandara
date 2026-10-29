<script lang="ts">
  // Suchi: everything on a drive, browsed in the app.
  import ConfirmDelete from './ConfirmDelete.svelte'
  import Duplicates from './Duplicates.svelte'
  import FolderList from './FolderList.svelte'
  import type { Files, FilesTab } from './files.svelte'
  import { formatAge, formatBytes } from './format'
  import Icon from './Icon.svelte'
  import LargestList from './LargestList.svelte'
  import TypesList from './TypesList.svelte'

  let { files }: { files: Files } = $props()

  const tabs: { id: FilesTab; label: string }[] = [
    { id: 'folders', label: 'Folders' },
    { id: 'types', label: 'Types' },
    { id: 'largest', label: 'Largest files' },
    { id: 'duplicates', label: 'Duplicates' },
  ]

  const summary = $derived(files.summary)
  const progress = $derived(files.progress.total > 0 ? files.progress.done / files.progress.total : 0)
</script>

<div class="files">
  <div class="roots" role="group" aria-label="Scan">
    {#each files.roots as root (root.path)}
      {@const active = (files.scanning ? files.scanningRoot : summary?.root) === root.path}
      <button class="root" class:active onclick={() => files.show(root.path)} disabled={files.scanning} title={root.path}>
        <Icon name={files.roots[0] === root ? 'home' : 'drive'} size={16} />
        {root.name}
      </button>
    {/each}
  </div>

  {#if files.needsAccess}
    <section class="card access">
      <p>Bhandara needs <strong>All files access</strong> to list your files. It reads names and sizes, and changes nothing unless you delete something.</p>
      <button class="primary" onclick={() => files.requestAccess()}>Allow access</button>
    </section>
  {:else}
    <section class="card status">
      {#if files.scanning}
        <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax={files.progress.total} aria-valuenow={files.progress.done}>
          <div class="track"><div class="fill" style:width="{progress * 100}%"></div></div>
          <span>Scanning… {files.progress.done} of {files.progress.total} folders</span>
          <button class="ghost" onclick={() => files.stop()}><Icon name="stop" /> Stop</button>
        </div>
      {:else if summary}
        <div class="line">
          <span>
            <strong>{formatBytes(files.bytes(summary))}</strong> in {summary.count.toLocaleString()} files and {summary.folders.toLocaleString()} folders · scanned {formatAge(summary.scannedAt)}
          </span>
          <div class="controls">
            <div class="toggle" role="group" aria-label="Size shown">
              <button class:on={files.sizeMode === 'alloc'} onclick={() => files.setSizeMode('alloc')} title="Space the files take up on the drive">On disk</button>
              <button class:on={files.sizeMode === 'size'} onclick={() => files.setSizeMode('size')} title="The size the files report">Size</button>
            </div>
            <button class="ghost" onclick={() => files.scan()}><Icon name="refresh" /> Scan again</button>
          </div>
        </div>
        {#if summary.partial}
          <p class="warn">The scan was stopped early, so these numbers are incomplete.</p>
        {/if}
        {#if summary.unreadableCount > 0}
          <details class="unreadable">
            <summary>{summary.unreadableCount.toLocaleString()} folders could not be read</summary>
            <ul>
              {#each summary.unreadable as path (path)}<li>{path}</li>{/each}
            </ul>
          </details>
        {/if}
      {/if}
    </section>

    {#if files.notice}
      <p class="notice" role="status">
        {files.notice}
        <button class="ghost" onclick={() => (files.notice = '')} aria-label="Dismiss">✕</button>
      </p>
    {/if}
    {#if files.error}
      <p class="error" role="alert">
        {files.error}
        <button class="ghost" onclick={() => (files.error = '')} aria-label="Dismiss">✕</button>
      </p>
    {/if}

    {#if summary}
      <section class="card">
        <div class="tabs" role="tablist">
          {#each tabs as tab (tab.id)}
            <button role="tab" aria-selected={files.tab === tab.id} class:on={files.tab === tab.id} onclick={() => files.setTab(tab.id)}>{tab.label}</button>
          {/each}
        </div>
        <div class="panel" role="tabpanel">
          {#if files.tab === 'folders'}<FolderList {files} />
          {:else if files.tab === 'types'}<TypesList {files} />
          {:else if files.tab === 'largest'}<LargestList {files} />
          {:else}<Duplicates {files} />{/if}
        </div>
      </section>
    {/if}

    {#if files.excluded.length > 0}
      <details class="card excluded">
        <summary>{files.excluded.length} excluded {files.excluded.length === 1 ? 'folder' : 'folders'}</summary>
        <ul>
          {#each files.excluded as path (path)}
            <li>
              <span>{path}</span>
              <button class="ghost" onclick={() => files.include(path)}>Include again</button>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {/if}
</div>

<ConfirmDelete {files} />

<style>
  .files {
    display: grid;
    gap: 16px;
  }
  .roots {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .root {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-radius: 99px;
    background: var(--button);
  }
  .root.active {
    color: var(--normal-a);
    border-color: var(--normal-a);
    font-weight: 600;
  }
  .access {
    display: grid;
    gap: 12px;
  }
  .access p {
    margin: 0;
    color: var(--dim);
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    color: var(--dim);
  }
  .line strong {
    color: var(--text);
    font-size: 15px;
  }
  .controls {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .controls > .ghost,
  .progress .ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--button);
  }
  .toggle,
  .tabs {
    display: flex;
    gap: 4px;
    padding: 3px;
    border-radius: 12px;
    background: var(--button);
  }
  .toggle button,
  .tabs button {
    padding: 6px 12px;
    border-radius: 9px;
    font-size: 13px;
    background: transparent;
  }
  .toggle .on,
  .tabs .on {
    background: var(--card);
    border-color: var(--card-border);
    color: var(--normal-a);
    font-weight: 600;
  }
  .tabs {
    width: fit-content;
    max-width: 100%;
    overflow-x: auto;
  }
  .panel {
    margin-top: 16px;
  }
  .progress {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    color: var(--dim);
  }
  .track {
    height: 6px;
    border-radius: 99px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: linear-gradient(90deg, var(--normal-a), var(--normal-b));
    transition: width 0.4s ease-out;
  }
  .warn {
    margin: 10px 0 0;
    font-size: 13px;
    color: var(--warning-a);
  }
  details summary {
    cursor: pointer;
    font-size: 13px;
    color: var(--dim);
  }
  .unreadable {
    margin-top: 10px;
  }
  details ul {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: 12px;
    font-family: ui-monospace, 'Cascadia Code', monospace;
    color: var(--dim);
  }
  .excluded li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .excluded li span {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .notice,
  .error {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin: 0;
    padding: 8px 8px 8px 14px;
    border-radius: 12px;
    font-size: 13px;
  }
  .notice {
    background: color-mix(in srgb, var(--normal-a) 14%, transparent);
  }
  .error {
    background: color-mix(in srgb, var(--critical-a) 15%, transparent);
    color: var(--critical-a);
  }
  @media (max-width: 720px) {
    .progress {
      grid-template-columns: 1fr auto;
    }
    .progress .track {
      grid-column: 1 / -1;
    }
  }
</style>
