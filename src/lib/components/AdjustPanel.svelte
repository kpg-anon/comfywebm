<script lang="ts">
  // Advanced > Adjustments: brightness, contrast, saturation, gamma, hue and a
  // master curve (ffmpeg eq, hue, curves). Double-click a slider to reset it.
  import { app, NEUTRAL_ADJUST, isNeutral } from "$lib/state.svelte";
  import { CURVE_PRESETS, isIdentity, normalize, type Pt } from "$lib/curve";
  import type { Adjust } from "$lib/types";
  import CurveEditor from "./CurveEditor.svelte";

  type Key = "brightness" | "contrast" | "saturation" | "gamma" | "hue";
  const SLIDERS: { key: Key; label: string; min: number; max: number; step: number; help: string; fmt: (v: number) => string }[] = [
    { key: "brightness", label: "Brightness", min: -0.5, max: 0.5, step: 0.01, help: "Adds to the light level of every pixel. 0 = unchanged.", fmt: (v) => (v > 0 ? "+" : "") + v.toFixed(2) },
    { key: "contrast", label: "Contrast", min: 0.5, max: 2, step: 0.01, help: "Spreads dark and light apart (above 1) or together (below 1). 1 = unchanged.", fmt: (v) => v.toFixed(2) },
    { key: "saturation", label: "Saturation", min: 0, max: 3, step: 0.05, help: "Color strength. 0 = black and white, 1 = unchanged.", fmt: (v) => v.toFixed(2) },
    { key: "gamma", label: "Gamma", min: 0.3, max: 3, step: 0.05, help: "Brightens (above 1) or darkens (below 1) the mid-tones and leaves black and white alone. 1 = unchanged.", fmt: (v) => v.toFixed(2) },
    { key: "hue", label: "Hue", min: -180, max: 180, step: 1, help: "Turns every color around the color wheel, in degrees. 0 = unchanged.", fmt: (v) => `${v > 0 ? "+" : ""}${Math.round(v)}°` },
  ];

  function set<K extends keyof Adjust>(k: K, v: Adjust[K]) {
    app.adjust = { ...app.adjust, [k]: v };
  }

  const curve = $derived<Pt[]>(app.adjust.curve.length ? (app.adjust.curve as Pt[]) : normalize([]));
</script>

<div class="adj">
  {#each SLIDERS as s (s.key)}
    <div class="row">
      <span class="lbl" title={s.help}>{s.label}</span>
      <input
        type="range"
        min={s.min}
        max={s.max}
        step={s.step}
        aria-label={s.label}
        value={app.adjust[s.key]}
        oninput={(e) => set(s.key, Number((e.currentTarget as HTMLInputElement).value))}
        ondblclick={() => set(s.key, NEUTRAL_ADJUST[s.key])}
      />
      <span class="val mono" class:changed={app.adjust[s.key] !== NEUTRAL_ADJUST[s.key]}>{s.fmt(app.adjust[s.key])}</span>
    </div>
  {/each}

  <div class="row">
    <span class="lbl" title="Maps every input light level (left to right) to an output level (bottom to top). Drag a point, click to add one, double-click a point to remove it.">Curve</span>
  </div>
  <div class="presets">
    {#each CURVE_PRESETS as p (p.name)}
      <button class="chip" onclick={() => set("curve", isIdentity(p.points) ? [] : p.points.map((q) => [...q] as Pt))}>{p.name}</button>
    {/each}
  </div>
  <CurveEditor points={curve} onchange={(p) => set("curve", isIdentity(p) ? [] : p)} />

  <div class="row foot">
    <label class="check" title="Shows the adjustments on the video while editing. The preview is close to, but not exactly, what ffmpeg produces; Sample (P) shows the real result.">
      <input type="checkbox" bind:checked={app.adjustPreview} /> Show on the picture
    </label>
    <button class="btn small" disabled={isNeutral(app.adjust)} onclick={() => (app.adjust = { ...NEUTRAL_ADJUST, curve: [] })}>Reset all</button>
  </div>
</div>

<style>
  .adj {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    flex-wrap: wrap;
  }
  .lbl {
    flex: none;
    width: 70px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  input[type="range"] {
    flex: 1;
  }
  .val {
    width: 46px;
    text-align: right;
    font-size: 11px;
    color: var(--text-muted);
  }
  .val.changed {
    color: var(--accent-strong);
  }
  /* the six curve presets in two columns, three rows */
  .presets {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 3px;
  }
  .chip {
    height: 20px;
    padding: 0 6px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-seg);
    background: var(--bg-chip);
    color: var(--text-secondary);
    font-size: 10.5px;
    cursor: pointer;
  }
  .chip:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .foot {
    justify-content: space-between;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    cursor: pointer;
  }
</style>
