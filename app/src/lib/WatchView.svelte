<script lang="ts">
  // Space watch: low-space alerts, what grew, and the optional scheduled clean.
  import { api, isAndroid, type CleanMode, type Growth, type WatchSettings, type WatchStatus } from './api'
  import type { Storage } from './storage.svelte'
  import { formatAge, formatBytes } from './format'

  let { storage }: { storage: Storage } = $props()

  const DAY = 86_400
  let settings = $state<WatchSettings | null>(null)
  let status = $state<WatchStatus | null>(null)
  let days = $state(7)
  let growth = $state<Growth | null>(null)
  let saved = $state('')
  let error = $state('')
  let sampling = $state(false)
  let askMode = $state(false)
  let mode = $state<CleanMode>('trash')

  $effect(() => {
    api.watchSettings().then((s) => (settings = s))
    refreshStatus()
  })

  $effect(() => {
    api.growth(days).then((g) => (growth = g)).catch(() => (growth = null))
  })

  const watched = $derived(storage.volumes.filter((v) => !v.removable))
  const now = () => Math.floor(Date.now() / 1000)

  async function refreshStatus() {
    status = await api.watchStatus().catch(() => null)
  }

  async function save(next: WatchSettings, message = 'Saved') {
    error = ''
    try {
      settings = await api.saveWatchSettings(next)
      saved = message
      setTimeout(() => (saved = ''), 2000)
    } catch (e) {
      error = String(e)
    }
  }

  function snooze(mount: string, seconds: number) {
    if (!settings) return
    save({ ...settings, snoozed: { ...settings.snoozed, [mount]: now() + seconds } }, 'Snoozed')
  }

  function toggleMute(mount: string) {
    if (!settings) return
    const muted = settings.muted.includes(mount) ? settings.muted.filter((m) => m !== mount) : [...settings.muted, mount]
    save({ ...settings, muted })
  }

  function enableSchedule() {
    if (!settings) return
    askMode = false
    save({ ...settings, schedule: { ...settings.schedule, enabled: true, mode } }, 'Scheduled clean is on')
  }

  async function sample() {
    sampling = true
    error = ''
    try {
      await api.takeSample()
      await refreshStatus()
      growth = await api.growth(days)
    } catch (e) {
      error = String(e)
    } finally {
      sampling = false
    }
  }

  function signed(n: number) {
    return `${n >= 0 ? '+' : '−'}${formatBytes(Math.abs(n))}`
  }
</script>

<div class="watch">
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <section class="card">
    <h2>Low-space alerts</h2>
    {#if settings}
      <div class="form">
        <label>Low below <input type="number" min="1" max="50" step="1" bind:value={settings.lowPercent} />% free</label>
        <label>or below <input type="number" min="0" max="10000" step="5" bind:value={settings.lowGb} /> GB free <span class="dim">(drives of 200 GB or more)</span></label>
        <label>Critical below <input type="number" min="1" max="49" step="1" bind:value={settings.criticalPercent} />% free</label>
        <button class="ghost" onclick={() => settings && save(settings)}>Save</button>
        {#if saved}<span class="ok" role="status">{saved}</span>{/if}
      </div>
      <p class="dim small">One alert each time a drive crosses into a worse level. It must gain 2 points of free space before it can alert again.</p>
      <ul class="vols">
        {#each watched as v (v.device)}
          {@const level = status?.levels[v.mount] ?? 'ok'}
          {@const until = settings.snoozed[v.mount] ?? 0}
          {@const muted = settings.muted.includes(v.mount)}
          <li>
            <span class="lvl {level}">{level === 'ok' ? 'OK' : level === 'low' ? 'Low' : 'Critical'}</span>
            <span class="vname">{v.name} <span class="dim">{v.mount}</span></span>
            <span class="dim">{formatBytes(v.avail)} free</span>
            <span class="vbtn">
              {#if until > now()}<span class="dim small">snoozed until {new Date(until * 1000).toLocaleDateString()}</span>{/if}
              <button class="ghost" onclick={() => snooze(v.mount, DAY)} disabled={muted}>Snooze 1 day</button>
              <button class="ghost" onclick={() => snooze(v.mount, 7 * DAY)} disabled={muted}>1 week</button>
              <button class="ghost" onclick={() => toggleMute(v.mount)}>{muted ? 'Turn alerts on' : 'Turn off'}</button>
            </span>
          </li>
        {/each}
      </ul>
      {#if status?.timer === false}
        <p class="warn small">
          The background watch is not running, so alerts only come from the bar colour. Install it with the steps in docs/space-watch.md (one user
          timer, no root).
        </p>
      {:else if isAndroid}
        <p class="dim small">On Android, levels update while Bhandara is open.</p>
      {/if}
    {:else}
      <p class="dim">Loading…</p>
    {/if}
  </section>

  <section class="card">
    <div class="line">
      <h2>What grew</h2>
      <div class="toggle" role="group" aria-label="Period">
        <button class:on={days === 7} onclick={() => (days = 7)}>7 days</button>
        <button class:on={days === 30} onclick={() => (days = 30)}>30 days</button>
      </div>
    </div>
    {#if growth && growth.since}
      <p class="dim small">Since {new Date(growth.since * 1000).toLocaleDateString()} ({formatAge(growth.since)}).</p>
      <ul class="grow">
        {#each growth.volumes as v (v.mount)}
          <li><span>{v.name}</span><span class:up={v.delta > 0}>{signed(v.delta)} used</span></li>
        {/each}
      </ul>
      {#if growth.folders.length}
        <h3>Folders that grew</h3>
        <ul class="grow">
          {#each growth.folders as f (f.path)}
            <li><span class="path" title={f.path}>{f.path}</span><span class="up">{signed(f.delta)}</span><span class="dim">{formatBytes(f.bytes)}</span></li>
          {/each}
        </ul>
      {:else}
        <p class="dim">No folder grew in this period.</p>
      {/if}
    {:else}
      <p class="dim">
        Not enough history yet. Bhandara records free space and the biggest folders once a day ({status?.samples ?? 0} sample{status?.samples === 1
          ? ''
          : 's'} so far); come back after two days.
      </p>
    {/if}
    <button class="ghost" onclick={sample} disabled={sampling}>{sampling ? 'Measuring home… this takes a minute' : 'Record a sample now'}</button>
  </section>

  {#if !isAndroid && settings}
    <section class="card">
      <h2>Scheduled clean</h2>
      <p class="dim small">
        Once a week, removes build output from projects untouched for {settings.schedule.days} days. The same guards apply; you get a notification with
        what was freed, and every folder is listed in the receipts.
      </p>
      {#if settings.schedule.enabled}
        <p>
          <strong>On</strong> · {settings.schedule.mode === 'permanent' ? 'deletes permanently' : 'moves to Trash'} · projects untouched for
          <input type="number" min="1" max="365" bind:value={settings.schedule.days} /> days
          <button class="ghost" onclick={() => settings && save(settings)}>Save</button>
        </p>
        <button class="ghost" onclick={() => settings && save({ ...settings, schedule: { ...settings.schedule, enabled: false } }, 'Scheduled clean is off')}>
          Turn off
        </button>
      {:else if askMode}
        <fieldset>
          <legend>How should the schedule remove folders?</legend>
          <label><input type="radio" bind:group={mode} value="trash" /> Move to Trash (undoable; frees space only when the Trash is emptied)</label>
          <label><input type="radio" bind:group={mode} value="permanent" /> Delete permanently (frees space now; cannot be undone)</label>
          <div class="row">
            <button class="ghost" onclick={() => (askMode = false)}>Cancel</button>
            <button class="primary" onclick={enableSchedule}>Turn on</button>
          </div>
        </fieldset>
      {:else}
        <p><strong>Off</strong></p>
        <button class="ghost" onclick={() => (askMode = true)}>Turn on…</button>
      {/if}
      <label class="check">
        <input type="checkbox" checked={settings.backupReleases} onchange={(e) => settings && save({ ...settings, backupReleases: e.currentTarget.checked })} />
        Copy release packages (APK, AAB, installers) to a backup folder before their build folder goes
      </label>
      <label class="check">
        Treat projects changed in the last
        <input type="number" min="1" max="365" bind:value={settings.activeDays} /> days as in use
        <button class="ghost" onclick={() => settings && save(settings)}>Save</button>
      </label>
    </section>
  {/if}
</div>

<style>
  .watch {
    display: grid;
    gap: 16px;
  }
  .form,
  .line,
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .line {
    justify-content: space-between;
  }
  input[type='number'] {
    width: 64px;
    margin: 0 4px;
    padding: 4px 6px;
    border-radius: 8px;
    border: 1px solid var(--card-border);
    background: var(--button);
    color: var(--text);
  }
  .dim {
    color: var(--dim);
  }
  .small {
    font-size: 12px;
  }
  .ok {
    color: var(--normal-a);
    font-size: 13px;
  }
  .warn {
    color: var(--warning-a);
  }
  .error {
    margin: 0;
    padding: 10px 14px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--critical-a) 15%, transparent);
    color: var(--critical-a);
  }
  ul {
    list-style: none;
    margin: 10px 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .vols li {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .vname {
    flex: 1;
    min-width: 140px;
  }
  .vbtn {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .lvl {
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: color-mix(in srgb, var(--normal-a) 15%, transparent);
    color: var(--normal-a);
  }
  .lvl.low {
    background: color-mix(in srgb, var(--warning-a) 18%, transparent);
    color: var(--warning-a);
  }
  .lvl.critical {
    background: color-mix(in srgb, var(--critical-a) 18%, transparent);
    color: var(--critical-a);
  }
  .toggle {
    display: flex;
    gap: 4px;
    padding: 3px;
    border-radius: 10px;
    background: var(--button);
  }
  .toggle button {
    padding: 4px 12px;
    background: transparent;
  }
  .toggle .on {
    background: var(--card);
    color: var(--normal-a);
    font-weight: 600;
  }
  h3 {
    margin: 12px 0 0;
    font-size: 14px;
  }
  .grow li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 12px;
    font-size: 13px;
  }
  .grow li:has(> :nth-child(2):last-child) {
    grid-template-columns: minmax(0, 1fr) auto;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .up {
    color: var(--warning-a);
    font-variant-numeric: tabular-nums;
  }
  fieldset {
    border: 1px solid var(--card-border);
    border-radius: 12px;
    padding: 10px 14px;
    display: grid;
    gap: 8px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: 13px;
    flex-wrap: wrap;
  }
</style>
