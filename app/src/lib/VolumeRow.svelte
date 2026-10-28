<script lang="ts">
  import { canOpenPaths, type Volume } from './api'
  import type { Storage } from './storage.svelte'
  import { formatBytes, percent, shortMount } from './format'

  let { volume, storage }: { volume: Volume; storage: Storage } = $props()

  const meta = $derived(
    [
      volume.model,
      volume.fstype,
      volume.encrypted ? 'encrypted' : '',
      volume.kind === 'removable' || volume.kind === 'internal' ? shortMount(volume.mount) : '',
    ]
      .filter(Boolean)
      .join(' · '),
  )
</script>

<div class="row">
  <svelte:element
    this={canOpenPaths ? 'button' : 'div'}
    class="main"
    class:clickable={canOpenPaths}
    role={canOpenPaths ? undefined : 'group'}
    onclick={canOpenPaths ? () => storage.open(volume.mount) : undefined}
    title={canOpenPaths ? `Open ${volume.mount}` : volume.mount}
  >
    <div class="head">
      <span class="name">{volume.name}</span>
      <span class="pct {volume.level}">{percent(volume.fraction)}</span>
    </div>
    <div class="bar"><div class="fill {volume.level}" style:width={percent(volume.fraction)}></div></div>
    <div class="foot">
      <span class="meta">{meta}</span>
      <span class="free">{formatBytes(volume.avail)} free of {formatBytes(volume.used + volume.avail)}</span>
    </div>
  </svelte:element>
  {#if volume.removable}
    <button
      class="eject"
      onclick={() => storage.eject(volume)}
      disabled={storage.ejecting === volume.mount}
      title="Eject {volume.name}"
      aria-label="Eject {volume.name}">⏏</button
    >
  {/if}
</div>

<style>
  .row {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 14px;
    text-align: left;
    background: var(--button);
  }
  .head,
  .foot {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .name {
    font-weight: 600;
  }
  .pct {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .pct.warning {
    color: var(--warning-a);
  }
  .pct.critical {
    color: var(--critical-a);
  }
  .foot {
    font-size: 12px;
    color: var(--dim);
  }
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .free {
    flex: none;
  }
  .bar {
    height: 6px;
    border-radius: 99px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--normal-a), var(--normal-b));
    transition: width 0.8s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .fill.warning {
    background: linear-gradient(90deg, var(--warning-a), var(--warning-b));
  }
  .fill.critical {
    background: linear-gradient(90deg, var(--critical-a), var(--critical-b));
  }
  .eject {
    width: 44px;
    border-radius: 14px;
    font-size: 18px;
  }
</style>
