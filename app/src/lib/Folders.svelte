<script lang="ts">
  import type { Storage } from './storage.svelte'
  import { formatBytes } from './format'

  let { storage }: { storage: Storage } = $props()

  const largest = $derived(storage.folders.folders[0]?.bytes || 1)
</script>

<section class="card">
  <header>
    <h2>Largest folders</h2>
    <button class="ghost" onclick={() => storage.refreshFolders(true)} disabled={storage.scanning}>
      {storage.scanning ? 'Scanning…' : 'Rescan'}
    </button>
  </header>
  {#if storage.folders.folders.length === 0}
    <p class="empty">{storage.scanning ? 'Measuring folders…' : 'Nothing to show'}</p>
  {:else}
    <ul>
      {#each storage.folders.folders as folder (folder.path)}
        <li>
          <button onclick={() => storage.open(folder.path)} title="Open {folder.path}">
            <span class="name">{folder.name}</span>
            <span class="size">{formatBytes(folder.bytes)}</span>
            <span class="bar" style:width="{(folder.bytes / largest) * 100}%"></span>
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
  li button {
    position: relative;
    width: 100%;
    display: flex;
    justify-content: space-between;
    padding: 9px 12px;
    border-radius: 10px;
    overflow: hidden;
    background: transparent;
  }
  li button:hover {
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
</style>
