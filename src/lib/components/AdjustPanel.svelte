<script lang="ts">
  // Advanced > Adjustments: brightness, contrast, saturation, gamma, hue and a
  // master curve (ffmpeg eq, hue, curves). Each row has a reset button (or double-click
  // the slider); hold the eye (or \) to see the picture without them.
  import { app, NEUTRAL_ADJUST, isNeutral } from "$lib/state.svelte";
  import { CURVE_PRESETS, isIdentity, normalize, type Pt } from "$lib/curve";
  import type { Adjust } from "$lib/types";
  import CurveEditor from "./CurveEditor.svelte";
  import Icon from "./Icon.svelte";

  type Key = "temperature" | "tint" | "brightness" | "contrast" | "saturation" | "gamma" | "hue";
  const signed = (v: number) => (v > 0 ? "+" : "") + v.toFixed(2);
  const SLIDERS: { key: Key; label: string; min: number; max: number; step: number; help: string; fmt: (v: number) => string }[] = [
    { key: "temperature", label: "Temperature", min: -1, max: 1, step: 0.01, help: "White balance: warmer to the right (more red, less blue), cooler to the left. 0 = unchanged. Auto sets it from the clip.", fmt: signed },
    { key: "tint", label: "Tint", min: -1, max: 1, step: 0.01, help: "White balance: toward magenta to the right, toward green to the left. 0 = unchanged.", fmt: signed },
    { key: "brightness", label: "Brightness", min: -0.5, max: 0.5, step: 0.01, help: "Adds to the light level of every pixel. 0 = unchanged.", fmt: (v) => (v > 0 ? "+" : "") + v.toFixed(2) },
    { key: "contrast", label: "Contrast", min: 0.5, max: 2, step: 0.01, help: "Spreads dark and light apart (above 1) or together (below 1). 1 = unchanged.", fmt: (v) => v.toFixed(2) },
    { key: "saturation", label: "Saturation", min: 0, max: 3, step: 0.05, help: "Color strength. 0 = black and white, 1 = unchanged.", fmt: (v) => v.toFixed(2) },
    { key: "gamma", label: "Gamma", min: 0.3, max: 3, step: 0.05, help: "Brightens (above 1) or darkens (below 1) the mid-tones and leaves black and white alone. 1 = unchanged.", fmt: (v) => v.toFixed(2) },
    { key: "hue", label: "Hue", min: -180, max: 180, step: 1, help: "Turns every color around the color wheel, in degrees. 0 = unchanged.", fmt: (v) => `${v > 0 ? "+" : ""}${Math.round(v)}°` },
  ];

  function set<K extends keyof Adjust>(k: K, v: Adjust[K]) {
    app.adjust = { ...app.adjust, [k]: v };
  }
  const val = (k: Key) => app.adjust[k] ?? 0;
  const neutral = (k: Key) => NEUTRAL_ADJUST[k] ?? 0;

  const curve = $derived<Pt[]>(app.adjust.curve.length ? (app.adjust.curve as Pt[]) : normalize([]));
</script>

<div class="adj">
  <div class="row foot">
    <button
      class="btn small eye"
      class:on={app.adjustHold}
      disabled={!app.media || isNeutral(app.adjust)}
      aria-label="Hold to see the original (or hold \). The preview is close to, but not exactly, what ffmpeg produces; Sample (P) shows the real result."
      onpointerdown={(e) => {
        if (e.button !== 0) return;
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        app.adjustHold = true;
      }}
      onpointerup={() => (app.adjustHold = false)}
      onpointercancel={() => (app.adjustHold = false)}
      onlostpointercapture={() => (app.adjustHold = false)}><Icon name="eye" size={14} /><span>Original</span></button
    >
    <span class="acts">
      <button
        class="btn small"
        disabled={!app.media || app.audioOnly || app.autoColorBusy}
        title="Measures 12 frames of the ranges and sets the white balance (temperature, tint), contrast, brightness and gamma: takes out a yellow or blue cast, stretches a flat picture, darkens a too-bright one. Saturation, hue and the curve stay as they are."
        onclick={() => void app.autoColor()}>{app.autoColorBusy ? "Measuring..." : "Auto"}</button
      >
      <button class="btn small" disabled={isNeutral(app.adjust)} onclick={() => (app.adjust = { ...NEUTRAL_ADJUST, curve: [] })}>Reset all</button>
    </span>
  </div>

  {#each SLIDERS as s (s.key)}
    <div class="row">
      <span class="lbl" title={s.help}>{s.label}</span>
      <input
        type="range"
        min={s.min}
        max={s.max}
        step={s.step}
        aria-label={s.label}
        value={val(s.key)}
        oninput={(e) => set(s.key, Number((e.currentTarget as HTMLInputElement).value))}
        ondblclick={() => set(s.key, neutral(s.key))}
      />
      <span class="val mono" class:changed={val(s.key) !== neutral(s.key)}>{s.fmt(val(s.key))}</span>
      <button
        class="reset"
        aria-label={`Reset ${s.label.toLowerCase()}`}
        title={`Reset ${s.label.toLowerCase()}`}
        disabled={val(s.key) === neutral(s.key)}
        onclick={() => set(s.key, neutral(s.key))}><Icon name="undo" size={12} /></button
      >
    </div>
  {/each}

  <div class="row">
    <span class="lbl" title="Maps every input light level (left to right) to an output level (bottom to top). Drag a point, click to add one, double-click a point to remove it.">Curve</span>
    <span class="grow"></span>
    <button class="reset" aria-label="Reset the curve" title="Reset the curve" disabled={!app.adjust.curve.length} onclick={() => set("curve", [])}><Icon name="undo" size={12} /></button>
  </div>
  <div class="presets">
    {#each CURVE_PRESETS as p (p.name)}
      <button class="chip" onclick={() => set("curve", isIdentity(p.points) ? [] : p.points.map((q) => [...q] as Pt))}>{p.name}</button>
    {/each}
  </div>
  <CurveEditor points={curve} onchange={(p) => set("curve", isIdentity(p) ? [] : p)} />
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
  .eye {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    user-select: none;
  }
  .eye.on {
    background: var(--accent);
    color: var(--accent-text, #fff);
  }
  .acts {
    display: flex;
    gap: 4px;
  }
  .grow {
    flex: 1;
  }
  .reset {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .reset:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .reset:disabled {
    opacity: 0.3;
    cursor: default;
  }
</style>
