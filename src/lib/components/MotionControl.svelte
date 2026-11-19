<script lang="ts">
  // Frame rate cap and speed, checked against the display refresh rate
  // (Settings). Speed is a slider on a log scale from 0.25x to 4x that snaps
  // to speeds which play evenly; the chips list those speeds for this source.
  // Problems are explained with the nearest smooth values to switch to.
  import { app } from "$lib/state.svelte";
  import { evenOn, fmtFps, fmtSpeed, motion, nearestSmooth, smooth, smoothSpeeds } from "$lib/motion";
  import Select from "./Select.svelte";
  import { speedRange } from "$lib/ramp";

  const LO = 0.25;
  const HI = 4;

  const hz = $derived(app.refreshHz);
  /** a speed ramp on the timeline replaces the Speed setting, for the editor's file only
   * (Batch and Splits jobs use Speed) */
  const ramped = $derived(app.speedRamp.length > 0 && app.view === "editor");
  const rampRange = $derived(
    ramped ? speedRange(app.speedRamp, app.segments.length ? app.segments : [{ start: 0, end: app.duration }]) : [1, 1],
  );
  const src = $derived(app.fpsSrc);
  const cap = $derived(app.fps && app.fps > 0 ? app.fps : null);
  const m = $derived(motion(src, app.speed || 1, cap, hz));
  const list = $derived(smoothSpeeds(src, cap, hz, LO, HI));
  const near = $derived(nearestSmooth(app.speed, list));
  // 0.4x and 0.8x are always offered: they turn 60 and 30 fps footage into 24 fps
  // playback, even where that is not smooth on this display
  const chips = $derived([...new Set([...list, 0.4, 0.8])].sort((a, b) => a - b));
  const bad = $derived(!smooth(m) && (app.speed !== 1 || m.capped));

  const fpsOptions = $derived([
    { value: null, label: `Source${app.media ? ` (${fmtFps(src)})` : ""}` },
    ...app.fpsCaps.map((f) => {
      const mm = motion(src, app.speed || 1, f, hz);
      const hint = !app.media ? "" : !mm.capped ? "no change" : smooth(mm) ? "smooth" : mm.uneven ? "stutters" : `judders at ${hz} Hz`;
      return { value: f as number | null, label: `At most ${f} fps`, hint };
    }),
  ]);

  // the highest caps that make the current speed smooth (for the suggestion line)
  const goodCaps = $derived(
    [...app.fpsCaps].reverse().filter((f) => {
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
    <span class="lbl" title="The most frames per second the output may have. Fewer frames leave more bitrate for each frame. Source keeps the source rate (times the speed).">Frame rate</span>
    <Select label="Frame rate cap" value={cap} onchange={(v) => (app.fps = v)} options={fpsOptions} />
  </div>

  {#if ramped}
    <div class="row">
      <span class="lbl">Speed</span>
      <span class="ramped">
        Speed ramp on the timeline: {fmtSpeed(rampRange[0])}{#if rampRange[1] !== rampRange[0]}{" to "}{fmtSpeed(rampRange[1])}{/if}
      </span>
      <button class="btn small" onclick={() => (app.rampLane = true)}>Show</button>
      <button class="btn small" title="Remove the speed ramp; the Speed setting applies again" onclick={() => app.clearRamp()}>Remove</button>
    </div>
  {:else}
  <div class="row">
    <span class="lbl" title="Playback speed of the output, 0.25x to 4x; sound is sped up or slowed down with it. The slider snaps to speeds that play evenly on your display (the chips below). Double-click the slider for 1x.">Speed</span>
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
  {/if}

  {#if app.media && !ramped}
    <div class="chips" aria-label="Speeds that play evenly" title="Speeds whose output frame rate divides evenly into your display's refresh rate (Settings), so motion stays smooth">
      {#each chips as s (s)}
        <button class="chip" class:on={Math.abs(s - app.speed) < 1e-6} class:uneven={!list.includes(s)} title={`${fmtFps(outFpsOf(s))}${list.includes(s) ? "" : `, not even on ${hz} Hz`}`} onclick={() => (app.speed = s)}>{fmtSpeed(s)}</button>
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
  .ramped {
    flex: 1;
    min-width: 0;
    font-size: 11.5px;
    color: var(--accent-strong);
  }
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
  .chip.uneven {
    border-style: dashed;
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
