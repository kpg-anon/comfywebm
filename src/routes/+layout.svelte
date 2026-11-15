<script lang="ts">
  import "$lib/theme.css";
  import type { Snippet } from "svelte";
  import { applyTheme, storedTheme } from "$lib/themes";

  // before the first paint of the app, so the window does not show the default theme first
  applyTheme(storedTheme(), false);

  let { children }: { children: Snippet } = $props();
</script>

{@render children()}
