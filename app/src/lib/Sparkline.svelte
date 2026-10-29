<script lang="ts">
  // Read and write throughput history as two soft area curves.
  import { Spring } from 'svelte/motion'

  let { read = [], write = [], height = 56 }: { read?: number[]; write?: number[]; height?: number } = $props()

  const WIDTH = 300
  // Below 1 MB/s the graph stays flat, so background noise reads as idle.
  const FLOOR = 1024 * 1024
  const peak = $derived(Math.max(FLOOR, ...read, ...write) * 1.15)
  // The scale glides to a new peak instead of jumping with every sample.
  const scale = Spring.of(() => peak, { stiffness: 0.06, damping: 0.9 })
  const max = $derived(Math.max(scale.current, ...read, ...write))

  function path(values: number[], closed: boolean): string {
    if (values.length < 2) return ''
    const step = WIDTH / (values.length - 1)
    const points = values.map((v, i) => [i * step, height - (v / max) * (height - 4) - 2])
    let d = `M${points[0][0]},${points[0][1]}`
    for (let i = 1; i < points.length; i++) {
      const [x0, y0] = points[i - 1]
      const [x1, y1] = points[i]
      const mid = (x0 + x1) / 2
      d += ` C${mid},${y0} ${mid},${y1} ${x1},${y1}`
    }
    return closed ? `${d} L${WIDTH},${height} L0,${height} Z` : d
  }
</script>

<svg viewBox="0 0 {WIDTH} {height}" preserveAspectRatio="none" style:height="{height}px" aria-hidden="true">
  <path class="area read" d={path(read, true)} />
  <path class="area write" d={path(write, true)} />
  <path class="line read" d={path(read, false)} />
  <path class="line write" d={path(write, false)} />
</svg>

<style>
  svg {
    width: 100%;
    display: block;
  }
  .area {
    opacity: 0.18;
  }
  .line {
    fill: none;
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .read.area {
    fill: var(--read);
  }
  .write.area {
    fill: var(--write);
  }
  .read.line {
    stroke: var(--read);
  }
  .write.line {
    stroke: var(--write);
  }
</style>
