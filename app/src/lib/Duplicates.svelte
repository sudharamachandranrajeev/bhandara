<script lang="ts">
  import FileRow from './FileRow.svelte'
  import type { Files } from './files.svelte'
  import { formatBytes } from './format'

  let { files }: { files: Files } = $props()

  const wasted = $derived((files.duplicates ?? []).reduce((sum, g) => sum + g.wasted, 0))
  const progress = $derived(files.duplicateProgress.total ? files.duplicateProgress.done / files.duplicateProgress.total : 0)
</script>

<div class="head">
  <p>
    {#if files.duplicates}
      <strong>{files.duplicates.length.toLocaleString()}</strong> sets of identical files.
      Keeping one of each frees <strong>{formatBytes(wasted)}</strong>.
    {:else}
      Finds files of 1 MB or more with exactly the same content. Hard links are not counted as copies.
    {/if}
  </p>
  <div class="actions">
    {#if files.duplicates && files.duplicates.length > 0}
      <button class="ghost" onclick={() => files.exportDuplicates()}>Export CSV</button>
    {/if}
    <button class="primary small" onclick={() => files.findDuplicates()} disabled={files.findingDuplicates || files.scanning}>
      {files.findingDuplicates ? 'Comparing…' : files.duplicates ? 'Search again' : 'Find duplicates'}
    </button>
  </div>
</div>

{#if files.findingDuplicates && files.duplicateProgress.total > 0}
  <div class="progress">
    <div class="track"><div class="fill" style:width="{progress * 100}%"></div></div>
    <span>{files.duplicateProgress.done.toLocaleString()} of {files.duplicateProgress.total.toLocaleString()} size groups compared</span>
    <button class="ghost" onclick={() => files.stop()}>Stop</button>
  </div>
{/if}

{#if files.duplicates}
  {#each files.duplicates.slice(0, 200) as group (group.files.map((f) => f.id).join())}
    <section class="group">
      <h3>{group.files.length} copies · {formatBytes(group.size)} each · {formatBytes(group.wasted)} extra</h3>
      <ul>
        {#each group.files as file (file.id)}
          <FileRow {file} {files} canDelete={group.files.length > 1} />
        {/each}
      </ul>
    </section>
  {/each}
{/if}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    margin-bottom: 12px;
  }
  .head p {
    margin: 0;
    font-size: 13px;
    color: var(--dim);
  }
  .head strong {
    color: var(--text);
  }
  .actions {
    display: flex;
    gap: 8px;
    flex: none;
  }
  .actions .ghost {
    background: var(--button);
  }
  .primary.small {
    padding: 8px 14px;
    border-radius: 10px;
  }
  .progress {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
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
    transition: width 0.3s ease-out;
  }
  .group {
    margin-top: 10px;
    padding: 10px;
    border-radius: 14px;
    background: var(--button);
  }
  h3 {
    margin: 2px 8px 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--dim);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
</style>
