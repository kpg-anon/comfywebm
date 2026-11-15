<script lang="ts">
  // Frame rate cap and speed, checked against the display refresh rate
  // (Settings). Speed is a slider on a log scale from 0.25x to 4x that snaps
  // to speeds which play evenly; the chips list those speeds for this source.
  // Problems are explained with the nearest smooth values to switch to.
  import { app } from "$lib/state.svelte";
  import { evenOn, fmtFps, fmtSpeed, motion, nearestSmooth, smooth, smoothSpeeds } from "$lib/motion";
  import Select from "./Select.svelte";

  const FPS_CAPS = [120, 60, 50, 48, 30, 25, 24, 20, 15, 12, 10];
  const LO = 0.25;
  const HI = 4;

  const hz = $derived(app.refreshHz);
  const src = $derived(app.fpsSrc);
  const cap = $derived(app.fps && app.fps > 0 ? app.fps : null);
  const m = $derived(motion(src, app.speed || 1, cap, hz));
  const list = $derived(smoothSpeeds(src, cap, hz, LO, HI));
  const near = $derived(nearestSmooth(app.speed, list));
  const bad = $derived(!smooth(m) && (app.speed !== 1 || m.capped));

  const fpsOptions = $derived([
    { value: null, label: `Source${app.media ? ` (${fmtFps(src)})` : ""}` },
    ...FPS_CAPS.map((f) => {
      const mm = motion(src, app.speed || 1, f, hz);
      const hint = !app.media ? "" : !mm.capped ? "no change" : smooth(mm) ? "smooth" : mm.uneven ? "stutters" : `judders at ${hz} Hz`;
      return { value: f as number | null, label: `At most ${f} fps`, hint };
    }),
  ]);

  // caps that make the current speed smooth (for the suggestion line)
  const goodCaps = $derived(
    FPS_CAPS.filter((f) => {
      const mm = motion(src, app.speed || 1, f, hz);
      return mm.capped && smooth(mm);
    }).slice(0, 2),
  );

  const toSlider = (s: number) => Math.log2(s);
  function fromSlider(e: Event) {
    let s = 2 ** Number((e.currentTarget as HTMLInputElement).value);
    // snap to a smooth speed within 4%, else to 0.05
    const snap = list.find((x) => Math.abs(x - s) / x < 0.04);
    s = snap ?? Math.round(s * 20) / 20;
    app.speed = Math.min(HI, Math.max(LO, s));
  }

  function typed(e: Event) {
    const n = Number((e.currentTarget as HTMLInputElement).value.replace(",", ".").replace(/x$/i, ""));
    if (Number.isFinite(n) && n > 0) app.speed = Math.min(HI, Math.max(LO, Math.round(n * 1000) / 1000));
    else (e.currentTarget as HTMLInputElement).value = String(app.speed);
  }

  const outFpsOf = (s: number) => motion(src, s, cap, hz).out;
</script>

<div class="motion">
  <div class="row">
    <span class="lbl">Frame rate</span>
    <Select label="Frame rate cap" value={cap} onchange={(v) => (app.fps = v)} options={fpsOptions} />
  </div>

  <div class="row">
    <span class="lbl">Speed</span>
    <input
      type="range"
      min={toSlider(LO)}
      max={toSlider(HI)}
      step="0.01"
      aria-label="Speed"
      value={toSlider(app.speed || 1)}
      oninput={fromSlider}
      ondblclick={() => (app.speed = 1)}
    />
    <input class="field mono spd" type="text" inputmode="decimal" aria-label="Speed factor" value={fmtSpeed(app.speed)} onchange={typed} onfocus={(e) => (e.currentTarget as HTMLInputElement).select()} />
  </div>

  {#if app.media}
    <div class="chips" aria-label="Speeds that play evenly">
      {#each list as s (s)}
        <button class="chip" class:on={Math.abs(s - app.speed) < 1e-6} title={`${fmtFps(outFpsOf(s))}`} onclick={() => (app.speed = s)}>{fmtSpeed(s)}</button>
      {/each}
    </div>
    <div class="out mono" class:bad>
      {fmtFps(m.out)} out{#if m.capped} (from {fmtFps(m.rate)}){/if} ·
      {#if bad}
        {m.uneven ? "uneven frame steps" : `uneven on ${hz} Hz`}
      {:else if evenOn(m.out, hz)}
        even on {hz} Hz
      {:else}
        source rate
      {/if}
    </div>
    {#if bad}
      <div class="fix">
        <span class="muted">Smooth:</span>
        {#each near.filter((x) => x !== null) as s (s)}
          <button class="btn small" onclick={() => (app.speed = s as number)}>{fmtSpeed(s as number)} ({fmtFps(outFpsOf(s as number))})</button>
        {/each}
        {#each goodCaps as f (f)}
          <button class="btn small" onclick={() => (app.fps = f)}>cap {f} fps</button>
        {/each}
        {#if m.capped}
          <button class="btn small" onclick={() => (app.fps = null)}>no cap</button>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .motion {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
  }
  .row :global(.sel) {
    flex: 1;
    max-width: 190px;
    margin-left: auto;
  }
  .lbl {
    flex: none;
    width: 66px;
    color: var(--text-secondary);
    font-size: 12px;
  }
  input[type="range"] {
    flex: 1;
  }
  .spd {
    width: 62px;
    text-align: right;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
  }
  .chip {
    height: 20px;
    padding: 0 6px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-seg);
    background: var(--bg-chip);
    color: var(--text-secondary);
    font-family: var(--mono);
    font-size: 10.5px;
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
  .out {
    font-size: 11px;
    color: var(--text-muted);
  }
  .out.bad {
    color: var(--yellow);
  }
  .fix {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    font-size: 11px;
  }
</style>
