<script lang="ts">
  import type { Verdict } from './api'
  import type { Storage } from './storage.svelte'
  import { formatBytes } from './format'

  let { storage, compact = false }: { storage: Storage; compact?: boolean } = $props()

  const status = $derived(storage.health)
  const drives = $derived(status?.report?.drives ?? [])
  const windows = /Windows/i.test(navigator.userAgent)

  const label: Record<Verdict, string> = { good: 'Good', watch: 'Watch', replace: 'Replace' }
  const hours = new Intl.NumberFormat()

  function checkedAgo(generated: number): string {
    const minutes = Math.max(0, Math.round((Date.now() / 1000 - generated) / 60))
    if (minutes < 1) return 'just now'
    if (minutes < 120) return `${minutes} min ago`
    return `${Math.round(minutes / 60)} h ago`
  }
</script>

{#if status && status.state !== 'unsupported'}
  <section class="card" class:compact>
    <header>
      <h2>Drive health</h2>
      {#if status.report && status.state === 'ok'}
        <span class="checked">Checked {checkedAgo(status.report.generated)}</span>
      {/if}
    </header>

    {#if status.state === 'missing' || status.state === 'stale'}
      <div class="setup">
        <p>
          {#if status.state === 'stale'}
            The health check has stopped running, so these readings may be out of date.
          {:else}
            Drive health needs a small helper that reads SMART data with admin rights. Bhandara itself never asks for admin.
          {/if}
        </p>
        {#if status.canInstall}
          <button class="primary" onclick={() => storage.installHealth()} disabled={storage.installingHealth}>
            {storage.installingHealth ? 'Turning on…' : 'Turn on health monitoring'}
          </button>
        {:else if !windows}
          <p class="hint">Install smartmontools and enable <code>bhandara-health.timer</code>; the README has the steps.</p>
        {/if}
      </div>
    {/if}

    {#if drives.length > 0}
      <ul>
        {#each drives as drive (drive.serial || drive.device)}
          <li class={drive.verdict}>
            <div class="title">
              <span class="dot" aria-hidden="true"></span>
              <span class="model" title={drive.device}>{drive.model || drive.device}</span>
              <span class="verdict">{label[drive.verdict]}</span>
            </div>
            {#if drive.reasons.length > 0}
              <ul class="reasons">
                {#each compact ? drive.reasons.slice(0, 1) : drive.reasons as reason}
                  <li>{reason}</li>
                {/each}
              </ul>
            {/if}
            {#if !compact}
              <dl>
                {#if drive.wear !== null}
                  <div class="wear">
                    <dt>Life used</dt>
                    <dd>{drive.wear}%</dd>
                    <div class="track"><div class="fill" style:width="{Math.min(drive.wear, 100)}%"></div></div>
                  </div>
                {/if}
                {#if drive.temperature !== null}<div><dt>Temperature</dt><dd>{drive.temperature} °C</dd></div>{/if}
                {#if drive.powerOnHours !== null}<div><dt>Powered on</dt><dd>{hours.format(drive.powerOnHours)} h</dd></div>{/if}
                {#if drive.written !== null}<div><dt>Data written</dt><dd>{formatBytes(drive.written)}</dd></div>{/if}
                <div><dt>Type</dt><dd>{drive.protocol === 'NVMe' ? 'NVMe SSD' : drive.rotational ? 'Hard disk' : drive.protocol === 'ATA' ? 'SATA SSD' : drive.protocol}</dd></div>
              </dl>
            {/if}
          </li>
        {/each}
      </ul>
    {:else if status.state === 'ok'}
      <p class="empty">No drives reported SMART data.</p>
    {/if}

    {#if !compact && status.report && status.report.errors.length > 0}
      <details>
        <summary>{status.report.errors.length} drive{status.report.errors.length > 1 ? 's' : ''} could not be read</summary>
        <ul class="errors">
          {#each status.report.errors as error}<li>{error}</li>{/each}
        </ul>
      </details>
    {/if}
  </section>
{/if}

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .checked,
  .empty,
  .hint,
  .setup p {
    color: var(--dim);
    font-size: 12px;
  }
  .setup {
    display: grid;
    gap: 10px;
    margin-top: 10px;
  }
  .setup p {
    margin: 0;
    font-size: 13px;
  }
  ul {
    list-style: none;
    margin: 10px 0 0;
    padding: 0;
    display: grid;
    gap: 10px;
  }
  li.good {
    --tone-a: var(--normal-a);
    --tone-b: var(--normal-b);
  }
  li.watch {
    --tone-a: var(--warning-a);
    --tone-b: var(--warning-b);
  }
  li.replace {
    --tone-a: var(--critical-a);
    --tone-b: var(--critical-b);
  }
  ul > li {
    padding: 12px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--tone-a) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--tone-a) 22%, transparent);
  }
  .compact ul > li {
    padding: 8px 10px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    width: 9px;
    height: 9px;
    flex: none;
    border-radius: 50%;
    background: linear-gradient(135deg, var(--tone-a), var(--tone-b));
    box-shadow: 0 0 10px var(--tone-a);
  }
  .model {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .verdict {
    font-size: 12px;
    font-weight: 600;
    padding: 2px 9px;
    border-radius: 99px;
    color: var(--tone-a);
    background: color-mix(in srgb, var(--tone-a) 16%, transparent);
  }
  ul.reasons {
    margin: 6px 0 0 17px;
    gap: 2px;
    font-size: 13px;
  }
  ul.reasons li {
    padding: 0;
    border: none;
    background: none;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
    gap: 10px;
    margin: 12px 0 0;
  }
  dt {
    font-size: 11px;
    color: var(--dim);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  dd {
    margin: 2px 0 0;
    font-variant-numeric: tabular-nums;
  }
  .wear {
    grid-column: 1 / -1;
  }
  .wear dt,
  .wear dd {
    display: inline;
  }
  .wear dd {
    margin-left: 6px;
  }
  .track {
    height: 6px;
    margin-top: 6px;
    border-radius: 99px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--tone-a), var(--tone-b));
    transition: width 0.8s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  details {
    margin-top: 10px;
    font-size: 12px;
    color: var(--dim);
  }
  ul.errors {
    gap: 4px;
  }
  ul.errors li {
    padding: 6px 8px;
    border: none;
    background: var(--hover);
    word-break: break-word;
  }
  code {
    font-size: 12px;
  }
</style>
