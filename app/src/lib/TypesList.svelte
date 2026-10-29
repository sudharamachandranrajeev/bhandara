<script lang="ts">
  import type { Files } from './files.svelte'
  import { formatBytes, share } from './format'

  let { files }: { files: Files } = $props()

  const types = $derived([...files.types].sort((a, b) => files.bytes(b) - files.bytes(a)))
  const total = $derived(types.reduce((sum, t) => sum + files.bytes(t), 0))
  const largest = $derived(Math.max(1, ...types.slice(0, 1).map((t) => files.bytes(t))))
</script>

<p class="hint">Click a type to list its files of 1 MB or more.</p>
<ul>
  {#each types.slice(0, 60) as type (type.ext)}
    {@const bytes = files.bytes(type)}
    <li>
      <button onclick={() => files.showType(type.ext)} disabled={type.listed === 0} title={type.listed === 0 ? 'All files of this type are under 1 MB' : undefined}>
        <span class="ext">{type.ext ? `.${type.ext}` : 'no extension'}</span>
        <span class="count">{type.count.toLocaleString()} files</span>
        <span class="bytes">{formatBytes(bytes)}</span>
        <span class="pct">{share(bytes, total)}</span>
        <span class="bar" style:width="{(bytes / largest) * 100}%"></span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .hint {
    margin: 0 0 10px;
    font-size: 12px;
    color: var(--dim);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  button {
    position: relative;
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 90px 44px;
    gap: 12px;
    align-items: center;
    padding: 10px 12px 12px;
    border-radius: 12px;
    text-align: left;
    background: transparent;
    overflow: hidden;
    font-variant-numeric: tabular-nums;
  }
  button:disabled {
    opacity: 0.7;
  }
  .ext {
    font-weight: 600;
    font-family: ui-monospace, 'Cascadia Code', monospace;
  }
  .count,
  .pct {
    font-size: 12px;
    color: var(--dim);
  }
  .bytes {
    text-align: right;
    font-weight: 600;
  }
  .pct {
    text-align: right;
  }
  .bar {
    position: absolute;
    left: 12px;
    bottom: 4px;
    max-width: calc(100% - 24px);
    height: 3px;
    border-radius: 99px;
    background: linear-gradient(90deg, var(--read), var(--normal-b));
    opacity: 0.75;
  }
</style>
