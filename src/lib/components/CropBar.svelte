<script lang="ts">
  // Crop-mode controls: aspect lock, numeric x/y/w/h, full frame, clear.
  import { app } from "$lib/state.svelte";
  import { ASPECTS, MORE_ASPECTS, ratioOf } from "$lib/crop";
  import type { Crop } from "$lib/types";
  import Select from "./Select.svelte";

  const inMore = $derived(MORE_ASPECTS.some((m) => m.value === app.aspect));
  const custom = $derived(!ASPECTS.includes(app.aspect) && !inMore);
  let customText = $state("");
  let customBad = $state(false);

  function applyCustom() {
    const t = customText.trim();
    if (!t) return;
    customBad = ratioOf(t) === null;
    if (!customBad) {
      app.setAspect(t);
      customText = ""; // the placeholder now shows the ratio in use
    }
  }

  const FIELDS: (keyof Crop)[] = ["x", "y", "w", "h"];

  function setField(k: keyof Crop, raw: string) {
    const n = Math.round(Number(raw));
    if (!Number.isFinite(n)) return;
    const c = app.crop ?? { x: 0, y: 0, w: app.srcW, h: app.srcH };
    const next = { ...c, [k]: n };
    // keep the rectangle inside the picture: shrink the size before moving
    if (k === "x") next.x = Math.min(Math.max(0, n), app.srcW - next.w);
    if (k === "y") next.y = Math.min(Math.max(0, n), app.srcH - next.h);
    if (k === "w") next.w = Math.min(Math.max(1, n), app.srcW - next.x);
    if (k === "h") next.h = Math.min(Math.max(1, n), app.srcH - next.y);
    app.setCrop(next);
  }
</script>

<div class="cropbar">
  <span class="lbl">Aspect</span>
  <div class="seg" role="radiogroup" aria-label="Aspect lock">
    {#each ASPECTS as a (a)}
      <button class:on={app.aspect === a} role="radio" aria-checked={app.aspect === a} onclick={() => app.setAspect(a)}>
        {a === "free" ? "Free" : a}
      </button>
    {/each}
  </div>
  <span class="more" class:on={inMore}>
    <Select
      label="More aspect ratios"
      value={inMore ? app.aspect : null}
      placeholder="More..."
      options={MORE_ASPECTS.map((m) => ({ value: m.value as string | null, label: m.value, hint: m.hint }))}
      onchange={(v) => v && app.setAspect(v)}
    />
  </span>
  <input
    class="field mono custom"
    class:bad={customBad}
    class:on={custom}
    type="text"
    spellcheck="false"
    placeholder={custom ? app.aspect : "custom 5:3"}
    aria-label="Custom aspect ratio, for example 5:3 or 1.66"
    title="A ratio such as 5:3, 1.66:1 or 1.66. Enter applies it."
    bind:value={customText}
    onkeydown={(e) => {
      if (e.key === "Enter") {
        applyCustom();
        (e.currentTarget as HTMLInputElement).blur();
      }
    }}
    onblur={applyCustom}
  />

  <div class="nums">
    {#each FIELDS as k (k)}
      <label>
        <span>{k}</span>
        <input
          class="field mono"
          type="number"
          min="0"
          value={app.crop ? app.crop[k] : k === "w" ? app.srcW : k === "h" ? app.srcH : 0}
          onchange={(e) => setField(k, (e.currentTarget as HTMLInputElement).value)}
        />
      </label>
    {/each}
  </div>

  <button class="btn small" onclick={() => app.setCrop(null)} disabled={!app.crop}>Clear</button>
  <button class="btn small" onclick={() => (app.cropMode = false)}>Done <kbd>Esc</kbd></button>
  <span class="hint muted">Drag on the picture. Arrows move, Shift+arrows 10 px, Alt+arrows resize.</span>
  {#if app.rotate !== 0 || app.mirror}
    <span class="hint warn">The picture is shown unrotated while cropping; rotate and mirror apply after the crop.</span>
  {/if}
</div>

<style>
  .cropbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 6px 8px;
    border: var(--bw) solid var(--accent-line);
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent-bg) 50%, var(--bg-panel));
  }
  .lbl {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border-radius: var(--radius-seg);
    background: var(--bg-chip);
    border: var(--bw) solid var(--border);
  }
  .seg button {
    padding: 2px 8px;
    border: 0;
    border-radius: calc(var(--radius-seg) - 2px);
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--accent-bg);
    color: var(--accent-strong);
  }
  .more :global(.sel) {
    width: 92px;
    height: 24px;
  }
  .more.on :global(.sel) {
    border-color: var(--accent-line);
    color: var(--accent-strong);
  }
  .custom {
    width: 92px;
    height: 24px;
  }
  .custom.on {
    border-color: var(--accent-line);
  }
  .custom.on::placeholder {
    color: var(--accent-strong);
  }
  .nums {
    display: flex;
    gap: 6px;
  }
  .nums label {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .nums input {
    width: 64px;
    height: 22px;
  }
  .hint {
    font-size: 11px;
  }
  kbd {
    font-size: 10px;
  }
</style>
