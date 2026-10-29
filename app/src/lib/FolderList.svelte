<script lang="ts">
  import { canOpenPaths, type Entry } from './api'
  import type { Files } from './files.svelte'
  import { formatAge, formatBytes, share } from './format'
  import Icon from './Icon.svelte'

  let { files }: { files: Files } = $props()

  const folder = $derived(files.folder)
  const entries = $derived(folder ? [...folder.entries].sort((a, b) => files.bytes(b) - files.bytes(a)) : [])
  const largest = $derived(Math.max(1, ...entries.map((e) => files.bytes(e)), folder ? files.bytes(folder.small) : 0))
  const total = $derived(folder ? files.bytes(folder) : 0)

  function pathOf(entry: Entry): string {
    if (!folder) return entry.name
    const sep = folder.path.includes('\\') ? '\\' : '/'
    return folder.path.endsWith(sep) ? folder.path + entry.name : folder.path + sep + entry.name
  }

  function askDelete(entry: Entry) {
    files.askDelete(entry.item, entry.name, pathOf(entry), files.bytes(entry))
  }
</script>

{#if folder}
  <nav class="crumbs" aria-label="Folder path">
    {#if folder.crumbs.length > 0}
      <button class="ghost up" onclick={() => files.openFolder(folder.crumbs[folder.crumbs.length - 1].id)} aria-label="Up one folder">
        <Icon name="back" />
      </button>
    {/if}
    {#each folder.crumbs as crumb (crumb.id)}
      <button class="ghost crumb" onclick={() => files.openFolder(crumb.id)}>{crumb.name}</button>
      <span class="sep" aria-hidden="true">/</span>
    {/each}
    <span class="here">{folder.name}</span>
  </nav>

  <header class="current">
    <div>
      <p class="total">{formatBytes(total)}</p>
      <p class="dim">
        {folder.count.toLocaleString()} files{#if folder.modified} · changed {formatAge(folder.modified)}{/if}
      </p>
    </div>
    <div class="actions">
      {#if folder.crumbs.length > 0}
        <button class="ghost" onclick={() => files.rescan(folder.id)} disabled={files.scanning} title="Scan this folder again">
          <Icon name="refresh" /> Rescan
        </button>
      {/if}
      {#if canOpenPaths}
        <button class="ghost" onclick={() => files.open({ kind: 'dir', id: folder.id })} title="Open in the file manager">
          <Icon name="open" /> Open
        </button>
      {/if}
    </div>
  </header>

  {#if entries.length === 0 && folder.small.count === 0}
    <p class="empty">This folder is empty.</p>
  {/if}

  <ul class="rows">
    {#each entries as entry (entry.item.kind + entry.item.id)}
      {@const bytes = files.bytes(entry)}
      {@const isDir = entry.item.kind === 'dir'}
      <li class="row">
        <svelte:element
          this={isDir ? 'button' : 'div'}
          class="main"
          class:dir={isDir}
          onclick={isDir ? () => files.openFolder(entry.item.id) : undefined}
          title={pathOf(entry)}
          role={isDir ? undefined : 'group'}
        >
          <span class="icon" class:dir={isDir}><Icon name={isDir ? 'folder' : 'file'} size={18} /></span>
          <span class="text">
            <span class="name">{entry.name}</span>
            <span class="meta">
              {#if isDir}{entry.count.toLocaleString()} files · {/if}{formatAge(entry.modified)}
            </span>
          </span>
          <span class="nums">
            <span class="bytes">{formatBytes(bytes)}</span>
            <span class="pct">{share(bytes, total)}</span>
          </span>
          <span class="bar" style:width="{(bytes / largest) * 100}%"></span>
        </svelte:element>
        <div class="row-actions">
          {#if canOpenPaths}
            <button class="ghost icon-btn" onclick={() => files.open(entry.item)} title={isDir ? 'Open in the file manager' : 'Show in its folder'} aria-label="Open {entry.name}">
              <Icon name="open" />
            </button>
          {/if}
          {#if isDir}
            <button class="ghost icon-btn" onclick={() => files.exclude(entry.item.id)} title="Leave out of future scans" aria-label="Exclude {entry.name}">
              <Icon name="hide" />
            </button>
          {/if}
          <button class="ghost icon-btn delete" onclick={() => askDelete(entry)} title="Delete" aria-label="Delete {entry.name}">
            <Icon name="trash" />
          </button>
        </div>
      </li>
    {/each}
    {#if folder.small.count > 0}
      {@const bytes = files.bytes(folder.small)}
      <li class="row">
        <div class="main small" role="group">
          <span class="icon"><Icon name="copy" size={18} /></span>
          <span class="text">
            <span class="name">{folder.small.count.toLocaleString()} smaller files</span>
            <span class="meta">Files under 1 MB are counted, not listed</span>
          </span>
          <span class="nums">
            <span class="bytes">{formatBytes(bytes)}</span>
            <span class="pct">{share(bytes, total)}</span>
          </span>
          <span class="bar" style:width="{(bytes / largest) * 100}%"></span>
        </div>
        <div class="row-actions"></div>
      </li>
    {/if}
  </ul>
{/if}

<style>
  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    font-size: 13px;
  }
  .crumb {
    color: var(--dim);
    font-size: 13px;
    padding: 4px 6px;
  }
  .up {
    display: inline-flex;
    padding: 5px;
    margin-right: 4px;
  }
  .sep {
    color: var(--dim);
    opacity: 0.6;
  }
  .here {
    font-weight: 600;
    padding: 4px 6px;
  }
  .current {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 12px;
    margin: 10px 0 14px;
  }
  .current p {
    margin: 0;
  }
  .total {
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  .dim,
  .empty {
    color: var(--dim);
    font-size: 13px;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--button);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .row {
    display: flex;
    align-items: stretch;
    gap: 4px;
    border-radius: 12px;
  }
  .main {
    position: relative;
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px 12px;
    border-radius: 12px;
    text-align: left;
    background: transparent;
    overflow: hidden;
  }
  .main.dir:hover {
    background: var(--hover);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    color: var(--dim);
    background: var(--button);
  }
  .icon.dir {
    color: var(--normal-a);
    background: color-mix(in srgb, var(--normal-a) 14%, transparent);
  }
  .text {
    display: grid;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }
  .small .name {
    color: var(--dim);
  }
  .meta {
    font-size: 12px;
    color: var(--dim);
  }
  .nums {
    display: grid;
    justify-items: end;
    font-variant-numeric: tabular-nums;
  }
  .bytes {
    font-weight: 600;
  }
  .pct {
    font-size: 12px;
    color: var(--dim);
  }
  .bar {
    position: absolute;
    left: 12px;
    bottom: 4px;
    max-width: calc(100% - 24px);
    height: 3px;
    border-radius: 99px;
    background: linear-gradient(90deg, var(--normal-a), var(--normal-b));
    opacity: 0.75;
    transition: width 0.7s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .row-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 96px;
    justify-content: flex-end;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .row:hover .row-actions,
  .row:focus-within .row-actions {
    opacity: 1;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    padding: 7px;
    color: var(--dim);
  }
  .icon-btn:hover {
    color: var(--text);
  }
  .delete:hover {
    color: var(--critical-a);
  }
  /* Touch screens have no hover: keep actions visible but compact. */
  @media (hover: none) {
    .row-actions {
      opacity: 1;
      min-width: 0;
    }
  }
</style>
