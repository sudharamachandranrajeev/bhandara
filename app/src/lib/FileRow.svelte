<script lang="ts">
  // One listed file with its folder, size, age, and actions.
  import { canOpenPaths, type FileView } from './api'
  import type { Files } from './files.svelte'
  import { formatAge, formatBytes } from './format'
  import Icon from './Icon.svelte'

  let { file, files, canDelete = true }: { file: FileView; files: Files; canDelete?: boolean } = $props()

  const folderPath = $derived(file.path.slice(0, Math.max(0, file.path.length - file.name.length - 1)))
</script>

<li class="row">
  <span class="icon"><Icon name="file" size={18} /></span>
  <span class="text">
    <span class="name">{file.name}</span>
    <!-- Right-to-left keeps the end of a long path visible; the marks keep its slashes in place. -->
    <span class="meta" title={file.path}>{'\u200e' + folderPath + '\u200e'}</span>
  </span>
  <span class="nums">
    <span class="bytes">{formatBytes(files.bytes(file))}</span>
    <span class="age">{formatAge(file.modified)}</span>
  </span>
  <span class="actions">
    {#if canOpenPaths}
      <button class="ghost icon-btn" onclick={() => files.open({ kind: 'file', id: file.id })} title="Show in its folder" aria-label="Show {file.name} in its folder">
        <Icon name="open" />
      </button>
    {/if}
    <button
      class="ghost icon-btn delete"
      onclick={() => files.askDelete({ kind: 'file', id: file.id }, file.name, file.path, files.bytes(file))}
      disabled={!canDelete}
      title={canDelete ? 'Delete' : 'Keep at least one copy'}
      aria-label="Delete {file.name}"
    >
      <Icon name="trash" />
    </button>
  </span>
</li>

<style>
  .row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 12px;
  }
  .row:hover {
    background: var(--hover);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 10px;
    color: var(--dim);
    background: var(--button);
  }
  .text {
    display: grid;
    min-width: 0;
  }
  .name,
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-weight: 500;
  }
  .meta,
  .age {
    font-size: 12px;
    color: var(--dim);
  }
  .meta {
    direction: rtl;
    text-align: left;
  }
  .nums {
    display: grid;
    justify-items: end;
    font-variant-numeric: tabular-nums;
  }
  .bytes {
    font-weight: 600;
  }
  .actions {
    display: flex;
    gap: 2px;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    padding: 7px;
    color: var(--dim);
  }
  .icon-btn:hover:not(:disabled) {
    color: var(--text);
  }
  .delete:hover:not(:disabled) {
    color: var(--critical-a);
  }
</style>
