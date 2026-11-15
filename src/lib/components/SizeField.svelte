<script lang="ts">
  // Size limit in MiB (1 MiB = 1048576 bytes). The byte count is the stored
  // value, so a preset's exact limit (for example 20000000) is kept until the
  // field is edited.
  import { MB } from "$lib/time";

  let { bytes = $bindable() }: { bytes: number } = $props();

  let editing = $state(false);
  let text = $state("");

  const shown = $derived(editing ? text : String(Math.round((bytes / MB) * 100) / 100));

  // A number is MiB. A unit may follow: MB, KB, GB are decimal (10^6 ...), as
  // Discord and most sites count; MiB, KiB, GiB (or M, K, G) are binary.
  const UNITS: Record<string, number> = {
    "": MB, m: MB, mib: MB, mb: 1e6,
    k: 1024, kib: 1024, kb: 1e3,
    g: MB * 1024, gib: MB * 1024, gb: 1e9,
    b: 1,
  };
  function parse(t: string): number | null {
    const m = /^\s*([0-9]*[.,]?[0-9]+)\s*([a-z]*)\s*$/i.exec(t);
    if (!m) return null;
    const v = Number(m[1].replace(",", "."));
    const u = UNITS[m[2].toLowerCase()];
    if (!Number.isFinite(v) || v <= 0 || u === undefined) return null;
    return Math.round(v * u);
  }
  const bad = $derived(editing && parse(text) === null);
</script>

<input
  class="field mono"
  type="text"
  inputmode="decimal"
  spellcheck="false"
  aria-label="Size limit in MiB"
  class:bad
  title="A number is MiB. Add MB for decimal megabytes (10 MB = 10,000,000 bytes), or KB, GB. Up and Down change it by 0.5 MiB."
  value={shown}
  onfocus={(e) => {
    editing = true;
    text = String(Math.round((bytes / MB) * 100) / 100);
    (e.currentTarget as HTMLInputElement).select();
  }}
  oninput={(e) => {
    text = (e.currentTarget as HTMLInputElement).value;
    const b = parse(text);
    if (b !== null) bytes = b;
  }}
  onblur={() => (editing = false)}
  onkeydown={(e) => {
    if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
    else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const step = (e.shiftKey ? 5 : 0.5) * MB;
      const next = Math.max(step, Math.round((bytes + (e.key === "ArrowUp" ? step : -step)) / step) * step);
      bytes = next;
      text = String(Math.round((next / MB) * 100) / 100);
    }
  }}
/>

<style>
  input {
    width: 90px;
  }
  input.bad {
    border-color: var(--red);
  }
</style>
