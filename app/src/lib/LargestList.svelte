<script lang="ts">
  import FileRow from './FileRow.svelte'
  import type { Files } from './files.svelte'

  let { files }: { files: Files } = $props()

  const MB = 1024 * 1024
  const sizes = [
    { label: 'Any size', value: 0 },
    { label: '100 MB+', value: 100 * MB },
    { label: '1 GB+', value: 1024 * MB },
  ]
  const YEAR = 365 * 86_400
  const ages = [
    { label: 'Any age', value: 0 },
    { label: 'Untouched 1 year+', value: 1 },
    { label: 'Untouched 3 years+', value: 3 },
  ]

  let ageYears = $state(0)

  function setFilter(change: { minSize?: number; years?: number; ext?: string | undefined }) {
    if (change.years !== undefined) ageYears = change.years
    files.filter = {
      ...files.filter,
      ...('minSize' in change ? { minSize: change.minSize } : {}),
      ...('ext' in change ? { ext: change.ext } : {}),
      modifiedBefore: ageYears ? Math.floor(Date.now() / 1000) - ageYears * YEAR : undefined,
    }
    files.loadLargest()
  }
</script>

<div class="filters">
  <div class="group" role="group" aria-label="Size">
    {#each sizes as size}
      <button class="chip" class:on={(files.filter.minSize ?? 0) === size.value} onclick={() => setFilter({ minSize: size.value })}>{size.label}</button>
    {/each}
  </div>
  <div class="group" role="group" aria-label="Age">
    {#each ages as age}
      <button class="chip" class:on={ageYears === age.value} onclick={() => setFilter({ years: age.value })}>{age.label}</button>
    {/each}
  </div>
  {#if files.filter.ext !== undefined}
    <button class="chip on" onclick={() => setFilter({ ext: undefined })} title="Show all types">
      {files.filter.ext ? `.${files.filter.ext}` : 'no extension'} ✕
    </button>
  {/if}
</div>

{#if files.largest.length === 0}
  <p class="empty">No files of 1 MB or more match.</p>
{:else}
  <ul>
    {#each files.largest as file (file.id)}
      <FileRow {file} {files} />
    {/each}
  </ul>
{/if}

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-bottom: 12px;
  }
  .group {
    display: flex;
    gap: 4px;
    padding: 3px;
    border-radius: 12px;
    background: var(--button);
  }
  .chip {
    padding: 5px 10px;
    border-radius: 9px;
    font-size: 12px;
    background: transparent;
  }
  .chip.on {
    background: var(--card);
    border-color: var(--card-border);
    color: var(--normal-a);
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  .empty {
    color: var(--dim);
    font-size: 13px;
  }
</style>
