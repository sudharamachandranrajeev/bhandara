<script lang="ts">
  import { deleteIsPermanent } from './api'
  import type { Files } from './files.svelte'
  import { formatBytes } from './format'

  let { files }: { files: Files } = $props()

  let dialog = $state<HTMLDialogElement>()

  // Opens as a modal whenever a delete is waiting; Cancel has the focus, so
  // Enter or Escape never deletes.
  $effect(() => {
    if (files.pending && dialog && !dialog.open) dialog.showModal()
    if (!files.pending && dialog?.open) dialog.close()
  })
</script>

<dialog bind:this={dialog} onclose={() => (files.pending = null)} aria-labelledby="confirm-title">
  {#if files.pending}
    {@const p = files.pending}
    <h3 id="confirm-title">{deleteIsPermanent ? 'Delete permanently?' : 'Move to Trash?'}</h3>
    <p class="name">{p.name}</p>
    <p class="path">{p.path}</p>
    <p class="size">{formatBytes(p.bytes)}{p.item.kind === 'dir' ? ', including everything inside' : ''}</p>
    {#if deleteIsPermanent}
      <p class="warn">Android has no Trash for these files. This cannot be undone.</p>
    {:else}
      <p class="dim">You can restore it from the Trash until the Trash is emptied.</p>
    {/if}
    <div class="actions">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="ghost cancel" autofocus onclick={() => (files.pending = null)} disabled={files.deleting}>Cancel</button>
      <button class="danger" onclick={() => files.confirmDelete()} disabled={files.deleting}>
        {files.deleting ? 'Deleting…' : deleteIsPermanent ? 'Delete' : 'Move to Trash'}
      </button>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 32px));
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
    margin: 0 0 12px;
    font-size: 18px;
  }
  p {
    margin: 4px 0;
  }
  .name {
    font-weight: 600;
    word-break: break-word;
  }
  .path,
  .dim {
    font-size: 12px;
    color: var(--dim);
    word-break: break-all;
  }
  .size {
    font-variant-numeric: tabular-nums;
  }
  .warn {
    margin-top: 10px;
    color: var(--critical-a);
    font-size: 13px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 18px;
  }
  .cancel {
    padding: 9px 16px;
    font-size: 14px;
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
