<script lang="ts">
  // Crop-mode controls: aspect lock, numeric x/y/w/h, full frame, clear.
  import { app } from "$lib/state.svelte";
  import { ASPECTS } from "$lib/crop";
  import type { Crop } from "$lib/types";

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
    border: 1px solid var(--accent-line);
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
    border-radius: var(--radius);
    background: var(--bg-chip);
    border: 1px solid var(--border);
  }
  .seg button {
    padding: 2px 8px;
    border: 0;
    border-radius: 4px;
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
