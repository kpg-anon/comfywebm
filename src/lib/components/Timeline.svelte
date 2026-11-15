<script lang="ts">
  import { app } from "$lib/state.svelte";
  import { fmtTime } from "$lib/time";
  import Icon from "./Icon.svelte";
  import TimeField from "./TimeField.svelte";
  import CropBar from "./CropBar.svelte";

  let track: HTMLDivElement;
  let drag: { kind: "seek" } | { kind: "start" | "end"; i: number } | null = null;

  const dur = $derived(app.duration || 0);
  const pct = (t: number) => (dur > 0 ? (Math.min(Math.max(t, 0), dur) / dur) * 100 : 0);
  const many = $derived(app.segments.length > 1);

  function timeAt(e: PointerEvent) {
    const b = track.getBoundingClientRect();
    const f = b.width > 0 ? (e.clientX - b.left) / b.width : 0;
    return Math.min(Math.max(f, 0), 1) * dur;
  }

  function down(e: PointerEvent, kind: "seek" | "start" | "end" | "body", i = -1) {
    if (!app.media || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    track.setPointerCapture(e.pointerId);
    if (kind === "seek" || kind === "body") {
      if (kind === "body") app.sel = i;
      drag = { kind: "seek" };
      app.seek(timeAt(e));
    } else {
      app.sel = i;
      drag = { kind, i };
    }
  }

  function moveEv(e: PointerEvent) {
    if (!drag) return;
    const t = timeAt(e);
    if (drag.kind === "seek") app.seek(t);
    else {
      app.setEdge(drag.i, drag.kind, t);
      const s = app.segments[drag.i];
      if (s) app.seek(drag.kind === "start" ? s.start : s.end);
    }
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    track.releasePointerCapture(e.pointerId);
    drag = null;
  }
</script>

<div class="timeline">
  <div
    class="track"
    class:disabled={!app.media}
    bind:this={track}
    role="presentation"
    onpointerdown={(e) => down(e, "seek")}
    onpointermove={moveEv}
    onpointerup={up}
    onpointercancel={up}
  >
    <div class="rail"></div>
    {#each app.segments as s, i (i)}
      <div
        class="sel"
        class:active={i === app.sel}
        style:left="{pct(s.start)}%"
        style:width="{pct(s.end) - pct(s.start)}%"
        role="presentation"
        onpointerdown={(e) => down(e, "body", i)}
      >
        {#if many}<span class="num">{i + 1}</span>{/if}
      </div>
      <div class="handle in" class:active={i === app.sel} style:left="{pct(s.start)}%" role="presentation" onpointerdown={(e) => down(e, "start", i)}></div>
      <div class="handle out" class:active={i === app.sel} style:left="{pct(s.end)}%" role="presentation" onpointerdown={(e) => down(e, "end", i)}></div>
    {/each}
    <div class="head" style:left="{pct(app.current)}%"></div>
  </div>

  <div class="row">
    <div class="group">
      <button class="btn icon" disabled={!app.media} onclick={() => app.stepFrame(-1)} aria-label="Previous frame (,)"><Icon name="prevFrame" /></button>
      <button class="btn icon" disabled={!app.media} onclick={() => app.togglePlay()} aria-label="Play or pause (Space)">
        <Icon name={app.paused ? "play" : "pause"} />
      </button>
      <button class="btn icon" disabled={!app.media} onclick={() => app.stepFrame(1)} aria-label="Next frame (.)"><Icon name="nextFrame" /></button>
    </div>

    <div class="times">
      <TimeField label="Time" value={app.media ? app.current : null} onset={(t) => app.seek(t)} />
      <TimeField label={many ? `In (range ${app.sel + 1})` : "In"} value={app.selSeg?.start ?? null} placeholder="start" onset={(t) => app.setSelStart(t)} />
      <TimeField label="Out" value={app.selSeg?.end ?? null} placeholder="end" onset={(t) => app.setSelEnd(t)} />
      <TimeField label="Length" value={app.media ? (app.selSeg ? app.selSeg.end - app.selSeg.start : app.duration) : null} onset={(t) => app.setSelLength(t)} />
    </div>
    {#if many}
      <div class="total">
        <span class="lbl">Total</span>
        <span class="mono">{fmtTime(app.clipLen)}</span>
      </div>
    {/if}
  </div>

  <div class="row">
    <div class="group">
      <button class="btn" disabled={!app.media} onclick={() => app.markIn()}><Icon name="markIn" size={14} />In <kbd>I</kbd></button>
      <button class="btn" disabled={!app.media} onclick={() => app.markOut()}><Icon name="markOut" size={14} />Out <kbd>O</kbd></button>
      <button class="btn" disabled={!app.media} onclick={() => app.addRange()}><Icon name="plus" size={14} />Range <kbd>N</kbd></button>
      <button class="btn" disabled={app.sel < 0} onclick={() => app.removeRange()} aria-label="Remove the selected range">
        <Icon name="x" size={14} />Remove{#if many}&nbsp;<kbd>Del</kbd>{/if}
      </button>
      <button class="btn" class:on={app.loop} disabled={!app.media} aria-pressed={app.loop} onclick={() => app.toggleLoop()}>
        <Icon name="loop" size={14} />Loop <kbd>L</kbd>
      </button>
      {#if many}
        <label class="sep"><input type="checkbox" bind:checked={app.separate} /> Separate files</label>
      {/if}
    </div>
    <span class="grow"></span>
    <div class="group">
      <button class="btn" class:on={app.cropMode} disabled={!app.media} aria-pressed={app.cropMode} onclick={() => (app.cropMode = !app.cropMode)}>
        <Icon name="crop" size={14} />Crop <kbd>C</kbd>
      </button>
      <button class="btn" disabled={!app.media || app.ffmpegMissing} onclick={() => app.runSample()}><Icon name="film" size={14} />Sample <kbd>P</kbd></button>
      <button class="btn" disabled={!app.media || app.ffmpegMissing} onclick={() => app.exportFrame()}><Icon name="image" size={14} />Frame <kbd>S</kbd></button>
    </div>
  </div>

  {#if app.cropMode}
    <CropBar />
  {/if}
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 8px 10px 10px;
    border-top: 1px solid var(--border-muted);
    background: var(--bg-panel);
  }
  .track {
    position: relative;
    height: 26px;
    cursor: pointer;
    touch-action: none;
  }
  .track.disabled {
    cursor: default;
    opacity: 0.5;
  }
  .rail {
    position: absolute;
    left: 0;
    right: 0;
    top: 10px;
    height: 6px;
    border-radius: 3px;
    background: var(--border);
  }
  .sel {
    position: absolute;
    top: 6px;
    height: 14px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--range) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--range) 45%, transparent);
    overflow: hidden;
  }
  .sel.active {
    background: color-mix(in srgb, var(--range) 38%, transparent);
    border-color: var(--range);
  }
  .num {
    position: absolute;
    left: 6px;
    top: -1px;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-primary);
    pointer-events: none;
  }
  .handle {
    position: absolute;
    top: 2px;
    width: 9px;
    height: 22px;
    margin-left: -4.5px;
    background: color-mix(in srgb, var(--range) 55%, var(--bg-panel));
    cursor: ew-resize;
  }
  .handle.active {
    background: var(--range);
  }
  .handle.in {
    border-radius: 3px 0 0 3px;
  }
  .handle.out {
    border-radius: 0 3px 3px 0;
  }
  .head {
    position: absolute;
    top: 0;
    width: 2px;
    height: 26px;
    margin-left: -1px;
    background: var(--accent);
    pointer-events: none;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
  }
  .row {
    display: flex;
    align-items: flex-end;
    gap: 6px 12px;
    flex-wrap: wrap;
  }
  .group {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .times {
    display: grid;
    grid-template-columns: repeat(4, 100px);
    gap: 6px;
  }
  .total {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: 4px;
    font-size: 12px;
  }
  .lbl {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
  }
  .sep {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 4px;
    font-size: 12px;
    cursor: pointer;
  }
  .btn kbd {
    font-size: 10px;
    padding: 0 3px;
    color: var(--text-muted);
  }
</style>
