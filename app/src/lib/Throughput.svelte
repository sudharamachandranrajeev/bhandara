<script lang="ts">
  import Sparkline from './Sparkline.svelte'
  import type { Storage } from './storage.svelte'
  import { formatRate } from './format'

  let { storage, height = 64 }: { storage: Storage; height?: number } = $props()
</script>

{#if storage.hasThroughput}
  <section class="card">
    <header>
      <h2>Activity</h2>
      <div class="rates">
        <span class="read">↓ {formatRate(storage.read)}</span>
        <span class="write">↑ {formatRate(storage.write)}</span>
      </div>
    </header>
    <Sparkline read={storage.readHistory} write={storage.writeHistory} {height} />
  </section>
{/if}

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }
  .rates {
    display: flex;
    gap: 14px;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .read {
    color: var(--read);
  }
  .write {
    color: var(--write);
  }
</style>
