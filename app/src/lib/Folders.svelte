<script lang="ts">
  import { canOpenPaths } from './api'
  import type { Storage } from './storage.svelte'
  import { formatBytes } from './format'

  let { storage }: { storage: Storage } = $props()

  const largest = $derived(storage.folders.folders[0]?.bytes || 1)
  const progress = $derived(storage.scanProgress.total > 0 ? storage.scanProgress.done / storage.scanProgress.total : 0)
</script>

<section class="card">
  <header>
    <h2>Largest folders</h2>
    {#if !storage.needsAccess}
      <button class="ghost" onclick={() => storage.refreshFolders(true)} disabled={storage.scanning}>
        {storage.scanning ? 'Scanning…' : 'Rescan'}
      </button>
    {/if}
  </header>

  {#if storage.scanning && storage.scanProgress.total > 0}
    <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax={storage.scanProgress.total} aria-valuenow={storage.scanProgress.done}>
      <div class="track"><div class="fill" style:width="{progress * 100}%"></div></div>
      <span>{storage.scanProgress.done} of {storage.scanProgress.total} folders measured</span>
    </div>
  {/if}

  {#if storage.needsAccess}
    <div class="access">
      <p>Bhandara needs <strong>All files access</strong> to measure your folders. It only reads sizes and never changes files.</p>
      <button class="primary" onclick={() => storage.requestAccess()}>Allow access</button>
    </div>
  {:else if storage.folders.folders.length === 0}
    <p class="empty">{storage.scanning ? 'Measuring folders…' : 'Nothing to show'}</p>
  {:else}
    <ul>
      {#each storage.folders.folders as folder (folder.path)}
        <li>
          <svelte:element
            this={canOpenPaths ? 'button' : 'div'}
            class="item"
            class:clickable={canOpenPaths}
            role={canOpenPaths ? undefined : 'listitem'}
            onclick={canOpenPaths ? () => storage.open(folder.path) : undefined}
            title={folder.path}
          >
            <span class="name">{folder.name}</span>
            <span class="size">{formatBytes(folder.bytes)}</span>
            <span class="bar" style:width="{(folder.bytes / largest) * 100}%"></span>
          </svelte:element>
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
    display: flex;
    justify-content: space-between;
    padding: 9px 12px;
    border-radius: 10px;
    overflow: hidden;
    background: transparent;
  }
  .clickable:hover {
    background: var(--hover);
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
  .size {
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }
  .empty {
    color: var(--dim);
    margin: 12px 0 4px;
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
    border-radius: inherit;
    background: linear-gradient(90deg, var(--normal-a), var(--normal-b));
    transition: width 0.4s ease-out;
  }
  .access {
    display: grid;
    gap: 12px;
    margin-top: 10px;
  }
  .access p {
    margin: 0;
    color: var(--dim);
  }
</style>
