<script lang="ts" generics="T extends string | number">
  interface Option {
    value: T;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value: T;
    options: Option[];
    label: string;
    /** when given, called instead of writing `value` (for setters with side effects) */
    onchange?: (v: T) => void;
  }

  let { value = $bindable(), options, label, onchange }: Props = $props();

  function pick(v: T) {
    if (onchange) onchange(v);
    else value = v;
  }
</script>

<div class="seg" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <button
      class:on={value === o.value}
      role="radio"
      aria-checked={value === o.value}
      disabled={o.disabled}
      onclick={() => pick(o.value)}>{o.label}</button
    >
  {/each}
</div>

<style>
  .seg {
    display: flex;
    padding: 2px;
    gap: 2px;
    border-radius: var(--radius);
    background: var(--bg-chip);
    border: 1px solid var(--border);
  }
  button {
    flex: 1;
    height: 22px;
    padding: 0 6px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    color: var(--text-primary);
  }
  button.on {
    background: var(--accent-bg);
    color: var(--accent-strong);
    box-shadow: inset 0 0 0 1px var(--accent-line);
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
    text-decoration: line-through;
  }
</style>
