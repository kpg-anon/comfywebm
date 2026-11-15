<script lang="ts">
  import { fmtTime, parseTime } from "$lib/time";

  interface Props {
    label: string;
    value: number | null;
    /** called with the parsed seconds when the field is committed (Enter or leaving it) */
    onset?: (secs: number) => void;
    placeholder?: string;
    readonly?: boolean;
  }

  let { label, value, onset, placeholder = "", readonly = false }: Props = $props();

  let editing = $state(false);
  let text = $state("");
  let bad = $state(false);

  const shown = $derived(editing ? text : value === null ? "" : fmtTime(value));

  function commit() {
    if (!editing) return;
    editing = false;
    if (text.trim() === "" || text === (value === null ? "" : fmtTime(value))) return;
    const v = parseTime(text);
    if (v === null) {
      bad = true;
      setTimeout(() => (bad = false), 900);
      return;
    }
    onset?.(v);
  }
</script>

<label class="tf">
  <span class="lbl">{label}</span>
  <input
    class="field mono"
    class:bad
    type="text"
    spellcheck="false"
    {placeholder}
    readonly={readonly || !onset}
    value={shown}
    onfocus={(e) => {
      if (readonly || !onset) return;
      editing = true;
      text = value === null ? "" : fmtTime(value);
      (e.currentTarget as HTMLInputElement).select();
    }}
    oninput={(e) => (text = (e.currentTarget as HTMLInputElement).value)}
    onblur={commit}
    onkeydown={(e) => {
      if (e.key === "Enter") {
        commit();
        (e.currentTarget as HTMLInputElement).blur();
      } else if (e.key === "Escape") {
        editing = false;
        e.preventDefault();
        (e.currentTarget as HTMLInputElement).blur();
      }
    }}
  />
</label>

<style>
  .tf {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .lbl {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
  }
  input {
    width: 100%;
    height: 24px;
    font-size: 12px;
  }
  input[readonly] {
    background: transparent;
    border-color: var(--border-muted);
  }
</style>
