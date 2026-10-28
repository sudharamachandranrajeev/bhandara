<script lang="ts">
  import { Spring } from 'svelte/motion'
  import type { Level } from './api'

  let { fraction = 0, level = 'normal', size = 120, stroke = 12, children }: {
    fraction?: number
    level?: Level
    size?: number
    stroke?: number
    children?: import('svelte').Snippet
  } = $props()

  const shown = Spring.of(() => Math.min(1, Math.max(0, fraction)), { stiffness: 0.08, damping: 0.6 })
  const radius = $derived((size - stroke) / 2)
  const circumference = $derived(2 * Math.PI * radius)
  const id = `ring-${Math.random().toString(36).slice(2)}`
</script>

<div class="ring" style:width="{size}px" style:height="{size}px">
  <svg viewBox="0 0 {size} {size}" aria-hidden="true">
    <defs>
      <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="var(--{level}-a)" />
        <stop offset="1" stop-color="var(--{level}-b)" />
      </linearGradient>
    </defs>
    <circle class="track" cx={size / 2} cy={size / 2} r={radius} stroke-width={stroke} />
    <circle
      class="fill"
      cx={size / 2}
      cy={size / 2}
      r={radius}
      stroke-width={stroke}
      stroke="url(#{id})"
      stroke-dasharray={circumference}
      stroke-dashoffset={circumference * (1 - shown.current)}
    />
  </svg>
  <div class="center">{@render children?.()}</div>
</div>

<style>
  .ring {
    position: relative;
    flex: none;
  }
  svg {
    width: 100%;
    height: 100%;
    transform: rotate(-90deg);
  }
  circle {
    fill: none;
  }
  .track {
    stroke: var(--track);
  }
  .fill {
    stroke-linecap: round;
    filter: drop-shadow(0 0 6px var(--glow));
  }
  .center {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    text-align: center;
  }
</style>
