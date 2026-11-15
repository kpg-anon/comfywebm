<script lang="ts">
  // Size limit in MiB (1 MiB = 1048576 bytes). The byte count is the stored
  // value, so a preset's exact limit (for example 20000000) is kept until the
  // field is edited.
  import { MB } from "$lib/time";

  let { bytes = $bindable() }: { bytes: number } = $props();

  let editing = $state(false);
  let text = $state("");

  const shown = $derived(editing ? text : String(Math.round((bytes / MB) * 100) / 100));
</script>

<input
  class="field mono"
  type="text"
  inputmode="decimal"
  spellcheck="false"
  aria-label="Size limit in MiB"
  value={shown}
  onfocus={(e) => {
    editing = true;
    text = String(Math.round((bytes / MB) * 100) / 100);
    (e.currentTarget as HTMLInputElement).select();
  }}
  oninput={(e) => {
    text = (e.currentTarget as HTMLInputElement).value;
    const v = Number(text.replace(",", "."));
    if (Number.isFinite(v) && v > 0) bytes = Math.round(v * MB);
  }}
  onblur={() => (editing = false)}
  onkeydown={(e) => {
    if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
  }}
/>

<style>
  input {
    width: 90px;
  }
</style>
