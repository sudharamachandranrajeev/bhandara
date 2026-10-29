<script lang="ts" module>
  // Line icons drawn on a 24-unit grid; colour follows the text.
  export type IconName = 'folder' | 'file' | 'drive' | 'home' | 'open' | 'refresh' | 'hide' | 'trash' | 'back' | 'stop' | 'copy'
</script>

<script lang="ts">
  let { name, size = 16 }: { name: IconName; size?: number } = $props()

  const paths: Record<IconName, string> = {
    folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    file: 'M6 3h8l4 4v14H6zM14 3v4h4',
    drive: 'M3 14h18v5H3zM5 14l2-9h10l2 9M17 16.5h.01',
    home: 'M4 11l8-7 8 7v9h-5v-6H9v6H4z',
    open: 'M14 4h6v6M20 4l-9 9M18 14v5H5V6h5',
    refresh: 'M20 11a8 8 0 1 0-2 5.3M20 5v6h-6',
    hide: 'M3 3l18 18M10.6 6.1A9.7 9.7 0 0 1 12 6c5 0 9 6 9 6a15 15 0 0 1-2.6 3.2M6.6 6.6C4.3 8.1 3 12 3 12s4 6 9 6a8.8 8.8 0 0 0 4-1',
    trash: 'M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3',
    back: 'M15 5l-7 7 7 7',
    stop: 'M7 7h10v10H7z',
    copy: 'M8 8h11v11H8zM5 16V5h11',
  }
</script>

<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <path d={paths[name]} />
</svg>

<style>
  svg {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
