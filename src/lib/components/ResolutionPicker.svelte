<script lang="ts">
  // Output size: Auto (size mode picks the largest size with a clean
  // picture), Source, a long-edge step, a percentage, or one exact edge with
  // the other following the aspect ratio. In size mode every step shows the
  // picture quality it would get at the bitrate the limit allows.
  import { app } from "$lib/state.svelte";
  import { PERCENTS, STEPS, exactBox, fit, percentBox, quality, stepBox, withLimit, type Box, type Quality, type ResMode } from "$lib/resolution";
  import Seg from "./Seg.svelte";

  const w = $derived(app.baseW);
  const h = $derived(app.baseH);
  const long = $derived(Math.max(w, h));
  const limit = $derived<Box>({ w: app.maxWidth || null, h: app.maxHeight || null });
  const limitLong = $derived(Math.max(limit.w ?? Infinity, limit.h ?? Infinity));
  // the bitrate does not depend on the picture size in size mode, so one plan gives every step's quality
  const kbps = $derived(app.rateMode === "size" ? (app.plan?.videoKbps ?? null) : null);

  const steps = $derived.by(() => {
    const top = Math.min(long || 1920, limitLong);
    const list = STEPS.filter((s) => s < top);
    if (app.resMode === "step" && !list.includes(app.resStep) && app.resStep < top) list.push(app.resStep);
    return list.sort((a, b) => a - b);
  });

  function dims(box: Box) {
    return w && h ? fit(w, h, withLimit(box, limit)) : null;
  }

  function q(box: Box): Quality | null {
    const d = dims(box);
    return d && kbps ? quality(kbps, d.w, d.h, app.outFps, app.codec) : null;
  }

  const modes = $derived<{ value: ResMode; label: string; disabled?: boolean }[]>([
    { value: "auto", label: "Auto" },
    { value: "source", label: "Source" },
    { value: "step", label: "Steps" },
    { value: "percent", label: "%" },
    { value: "exact", label: "Exact" },
  ]);

  // the exact fields: the edge being typed holds the value, the other follows the aspect ratio
  const exactOther = $derived(w && h ? Math.max(2, Math.round(app.exactEdge === "w" ? (app.exactVal * h) / w : (app.exactVal * w) / h)) : null);
  const exactW = $derived(app.exactEdge === "w" ? app.exactVal : exactOther);
  const exactH = $derived(app.exactEdge === "h" ? app.exactVal : exactOther);
  const exactTooBig = $derived(!!w && !!h && (app.exactEdge === "w" ? app.exactVal > w : app.exactVal > h));

  function setExact(edge: "w" | "h", e: Event) {
    const n = Math.round(Number((e.currentTarget as HTMLInputElement).value));
    if (!Number.isFinite(n) || n < 2) return;
    app.exactEdge = edge;
    app.exactVal = n;
  }

  function setPct(e: Event) {
    const n = Number((e.currentTarget as HTMLInputElement).value);
    if (Number.isFinite(n) && n > 0) app.resPct = Math.min(100, Math.max(1, Math.round(n)));
  }

  const QTEXT: Record<Quality, string> = { good: "clean picture", fair: "acceptable picture", poor: "blurry or blocky picture" };
  const out = $derived(app.plan ? `${app.plan.outWidth}x${app.plan.outHeight}` : null);
</script>

<div class="res">
  <div class="row">
    <span class="lbl">Resolution</span>
    <Seg label="Resolution mode" value={app.resMode} onchange={(v) => (app.resMode = v)} options={modes} />
  </div>

  {#if app.resMode === "auto"}
    <div class="note">
      {#if app.rateMode === "size"}
        Picks the largest size whose bitrate still gives a clean picture.
      {:else}
        Quality mode keeps the source size (within the picture limit).
      {/if}
    </div>
  {:else if app.resMode === "step"}
    <div class="chips" role="radiogroup" aria-label="Long edge">
      {#each steps as s (s)}
        {@const qq = q(stepBox(w, h, s))}
        {@const d = dims(stepBox(w, h, s))}
        <button
          class="chip"
          class:on={app.resStep === s}
          role="radio"
          aria-checked={app.resStep === s}
          title={`${d ? `${d.w}x${d.h}` : `${s} px long edge`}${qq ? ` · ${QTEXT[qq]}` : ""}`}
          onclick={() => (app.resStep = s)}
        >
          {#if qq}<i class="dot {qq}"></i>{/if}{s}
        </button>
      {:else}
        <span class="note">The picture is already small.</span>
      {/each}
    </div>
    <div class="note">Long edge in pixels.{#if kbps}{" "}Dots: picture quality at {Math.round(kbps)} kbps.{/if}</div>
  {:else if app.resMode === "percent"}
    <div class="chips" role="radiogroup" aria-label="Percent of the source">
      {#each PERCENTS as p (p)}
        {@const qq = q(percentBox(w, h, p))}
        <button class="chip" class:on={app.resPct === p} role="radio" aria-checked={app.resPct === p} onclick={() => (app.resPct = p)}>
          {#if qq}<i class="dot {qq}"></i>{/if}{p}%
        </button>
      {/each}
      <span class="unit">
        <input class="field mono pct" type="number" min="1" max="100" step="1" aria-label="Percent" value={app.resPct} onchange={setPct} />
        <span class="muted">%</span>
      </span>
    </div>
  {:else if app.resMode === "exact"}
    <div class="exact">
      <input class="field mono" type="number" min="2" step="2" aria-label="Width" class:lead={app.exactEdge === "w"} value={exactW ?? ""} onchange={(e) => setExact("w", e)} />
      <span class="muted">x</span>
      <input class="field mono" type="number" min="2" step="2" aria-label="Height" class:lead={app.exactEdge === "h"} value={exactH ?? ""} onchange={(e) => setExact("h", e)} />
      {#if q(exactBox(app.exactEdge, app.exactVal))}
        {@const qq = q(exactBox(app.exactEdge, app.exactVal))}
        <i class="dot {qq}" title={qq ? QTEXT[qq] : ""}></i>
      {/if}
    </div>
    <div class="note">
      Type either edge; the other follows the aspect ratio.
      {#if exactTooBig}<span class="warn">Larger than the source; the source size is used (no upscaling).</span>{/if}
    </div>
  {/if}

  {#if out}
    <div class="result mono">
      <span>{app.baseW}x{app.baseH}</span>
      <span class="muted">→</span>
      <span class="strong">{out}</span>
      {#if app.plan?.autoScaled}<span class="muted">(auto)</span>{/if}
    </div>
  {/if}
</div>

<style>
  .res {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .row :global(.seg) {
    flex: 1;
    max-width: 230px;
  }
  .lbl {
    color: var(--text-secondary);
    font-size: 12px;
  }
  .note {
    font-size: 11px;
    color: var(--text-muted);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 7px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-seg);
    background: var(--bg-chip);
    color: var(--text-secondary);
    font-family: var(--mono);
    font-size: 11px;
    cursor: pointer;
  }
  .chip:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .chip.on {
    background: var(--accent-bg);
    border-color: var(--accent-line);
    color: var(--accent-strong);
  }
  .dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex: none;
  }
  .dot.good {
    background: var(--green);
  }
  .dot.fair {
    background: var(--yellow);
  }
  .dot.poor {
    background: var(--red);
  }
  .unit {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }
  .pct {
    width: 56px;
  }
  .exact {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .exact input {
    width: 80px;
  }
  .exact input:not(.lead) {
    color: var(--text-muted);
  }
  .result {
    display: flex;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .strong {
    color: var(--text-primary);
    font-weight: 600;
  }
  .warn {
    color: var(--yellow);
  }
</style>
