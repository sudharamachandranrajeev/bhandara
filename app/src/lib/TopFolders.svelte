<script lang="ts">
  // Overview card: the biggest items in the scanned folder; each opens in the Files view.
  import type { Files } from './files.svelte'
  import { formatBytes } from './format'
  import Icon from './Icon.svelte'

  let { files, onopen }: { files: Files; onopen: () => void } = $props()

  const entries = $derived(files.top ? [...files.top.entries].sort((a, b) => files.bytes(b) - files.bytes(a)).slice(0, 8) : [])
  const largest = $derived(Math.max(1, ...entries.map((e) => files.bytes(e))))
  const progress = $derived(files.progress.total > 0 ? files.progress.done / files.progress.total : 0)

  function openEntry(kind: 'dir' | 'file', id: number) {
    if (kind === 'dir') files.openFolder(id)
    else files.openFolder(0)
    onopen()
  }
</script>

<section class="card">
  <header>
    <h2>Largest in {files.top?.name ?? 'your files'}</h2>
    <button class="ghost" onclick={onopen}>All files</button>
  </header>

  {#if files.scanning && !files.top}
    <div class="progress">
      <div class="track"><div class="fill" style:width="{progress * 100}%"></div></div>
      <span>Scanning… {files.progress.done} of {files.progress.total} folders</span>
    </div>
  {:else if files.needsAccess}
    <p class="empty">Bhandara needs All files access to measure your folders.</p>
    <button class="primary" onclick={() => files.requestAccess()}>Allow access</button>
  {:else if entries.length === 0}
    <p class="empty">Nothing to show</p>
  {:else}
    <ul>
      {#each entries as entry (entry.item.kind + entry.item.id)}
        <li>
          <button class="item" onclick={() => openEntry(entry.item.kind, entry.item.id)}>
            <Icon name={entry.item.kind === 'dir' ? 'folder' : 'file'} size={16} />
            <span class="name">{entry.name}</span>
            <span class="size">{formatBytes(files.bytes(entry))}</span>
            <span class="bar" style:width="{(files.bytes(entry) / largest) * 100}%"></span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  ul {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .item {
    position: relative;
    width: 100%;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-radius: 10px;
    overflow: hidden;
    text-align: left;
    background: transparent;
    color: var(--dim);
  }
  .item:hover {
    background: var(--hover);
  }
  .name {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .size {
    font-variant-numeric: tabular-nums;
  }
  .bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    background: linear-gradient(90deg, var(--normal-a), var(--normal-b));
    opacity: 0.7;
    transition: width 0.8s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .empty {
    color: var(--dim);
    margin: 12px 0;
  }
  .progress {
    display: grid;
    gap: 6px;
    margin-top: 12px;
    font-size: 12px;
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
</style>
