<script lang="ts">
  import { getCurrentWindow, Window } from '@tauri-apps/api/window'
  import Folders from './lib/Folders.svelte'
  import Ring from './lib/Ring.svelte'
  import Throughput from './lib/Throughput.svelte'
  import VolumeRow from './lib/VolumeRow.svelte'
  import { Storage } from './lib/storage.svelte'
  import { formatBytes, percent } from './lib/format'

  // The tray flyout and the dashboard share this page; the window label picks the layout.
  const flyout = getCurrentWindow().label === 'flyout'
  const storage = new Storage()

  $effect(() => storage.start())

  async function openDashboard() {
    const main = await Window.getByLabel('main')
    await main?.unminimize()
    await main?.show()
    await main?.setFocus()
    await getCurrentWindow().hide()
  }
</script>

<main class:flyout>
  {#if storage.system}
    {@const system = storage.system}
    <section class="card hero">
      <Ring fraction={system.fraction} level={system.level} size={flyout ? 92 : 148} stroke={flyout ? 10 : 14}>
        <span class="big">{percent(system.fraction)}</span>
        <span class="small">used</span>
      </Ring>
      <div class="summary">
        <h1>{system.name}</h1>
        <p class="free"><strong>{formatBytes(system.avail)}</strong> free</p>
        <p class="dim">{formatBytes(system.used)} used of {formatBytes(system.used + system.avail)}</p>
        {#if storage.busy}<p class="activity"><span class="dot"></span>Disk active</p>{/if}
      </div>
    </section>
  {:else if !storage.error}
    <section class="card hero"><p class="dim">Reading volumes…</p></section>
  {/if}

  <div class="grid">
    <section class="card volumes">
      <h2>Volumes</h2>
      <div class="list">
        {#each storage.volumes as volume (volume.device)}
          <VolumeRow {volume} {storage} />
        {/each}
      </div>
    </section>

    <div class="side">
      <Throughput {storage} height={flyout ? 40 : 72} />
      {#if !flyout}<Folders {storage} />{/if}
    </div>
  </div>

  {#if storage.error}
    <p class="error" role="alert">{storage.error}</p>
  {/if}

  {#if flyout}
    <button class="primary" onclick={openDashboard}>Open dashboard</button>
  {/if}
</main>

<style>
  main {
    max-width: 1100px;
    margin: 0 auto;
    padding: 24px;
    display: grid;
    gap: 16px;
  }
  main.flyout {
    padding: 12px;
    gap: 10px;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 28px;
  }
  .flyout .hero {
    gap: 18px;
  }
  .big {
    font-size: 30px;
    font-weight: 700;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  .flyout .big {
    font-size: 20px;
  }
  .small {
    font-size: 11px;
    color: var(--dim);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  h1 {
    margin: 0 0 6px;
    font-size: 24px;
    letter-spacing: -0.01em;
  }
  .flyout h1 {
    font-size: 18px;
  }
  .summary p {
    margin: 2px 0;
  }
  .free strong {
    font-size: 18px;
  }
  .dim {
    color: var(--dim);
  }
  .activity {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--normal-a);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--normal-a);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      transform: scale(0.55);
      opacity: 0.6;
    }
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }
  .flyout .grid,
  .side {
    display: grid;
    gap: 10px;
  }
  .flyout .grid {
    grid-template-columns: 1fr;
  }
  .side {
    gap: 16px;
  }
  .list {
    display: grid;
    gap: 8px;
    margin-top: 10px;
  }
  .error {
    margin: 0;
    padding: 10px 14px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--critical-a) 15%, transparent);
    color: var(--critical-a);
  }
  @media (max-width: 720px) {
    main {
      padding: 16px;
    }
    .grid {
      grid-template-columns: 1fr;
    }
    .hero {
      gap: 18px;
    }
  }
</style>
