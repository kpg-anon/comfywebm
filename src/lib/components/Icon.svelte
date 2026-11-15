<script lang="ts">
  import { ICONS, type IconName } from "$lib/icons";

  let { name, size = 16 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <!-- static markup from icons.ts, never user input -->
  {@html ICONS[name]}
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
