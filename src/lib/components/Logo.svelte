<script lang="ts">
  // The app icon (src-tauri/icons/icon.svg) without its tile, in theme colors.
  let { size = 22 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="120 130 784 780" aria-hidden="true">
  <rect x="164" y="156" width="696" height="440" rx="72" fill="var(--bg-chip)" stroke="var(--accent)" stroke-width="64" />
  <path d="M440 270 L440 482 L620 376 Z" fill="var(--primary)" stroke="var(--primary)" stroke-width="28" stroke-linejoin="round" />
  <rect x="140" y="760" width="744" height="56" rx="28" fill="var(--border)" />
  <rect x="330" y="752" width="364" height="72" fill="var(--range)" opacity="0.45" />
  <rect x="300" y="700" width="60" height="176" rx="16" fill="var(--range)" />
  <rect x="664" y="700" width="60" height="176" rx="16" fill="var(--range)" />
  <rect x="498" y="684" width="28" height="208" rx="12" fill="var(--playhead)" />
</svg>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>
