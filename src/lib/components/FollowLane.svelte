<script lang="ts">
  // The Follow lane under the timeline: the follow keyframes over file time, on
  // the timeline's own zoom window. Each keyframe is where the zoomed view is
  // centred at that time; between them the view moves on a smooth curve
  // (zoom.ts). The two lines show the centre across (accent) and down
  // (dashed), top = left/top of the picture. Click to add a keyframe (the
  // playhead goes there; drag the picture to aim it), drag one to move it in
  // time, double-click or right-click it to remove it. Times snap to range
  // edges and the playhead (Alt turns snapping off).
  // Several keyframes: drag on the lane to draw a box around them (Ctrl+drag
  // adds to the selection), Ctrl+click adds one or takes it out, Shift+click
  // selects all from the last one clicked. Dragging a selected keyframe moves
  // them all in time, Backspace or Delete removes them, dragging the picture
  // moves their view. The wheel zooms the timeline at the pointer (Shift+wheel
  // pans).
  import { app } from "$lib/state.svelte";
  import type { ZoomPoint } from "$lib/types";
  import * as zoomMath from "$lib/zoom";
  import { showMenu, type MenuItem } from "$lib/menu.svelte";
  import { fmtTime } from "$lib/time";

  let {
    viewStart,
    viewLen,
    ontrack,
    onwheel,
  }: { viewStart: number; viewLen: number; ontrack: () => void; onwheel: (e: WheelEvent, frac: number) => void } = $props();

  // the wheel zooms the timeline at the pointer; added by hand so it can call preventDefault
  $effect(() => {
    const el = lane;
    if (!el) return;
    const w = (e: WheelEvent) => {
      if (!app.media) return;
      e.preventDefault();
      const b = el.getBoundingClientRect();
      onwheel(e, b.width > 0 ? (e.clientX - b.left) / b.width : 0.5);
    };
    el.addEventListener("wheel", w, { passive: false });
    return () => el.removeEventListener("wheel", w);
  });

  const H = 48;
  const PAD = 5;
  const KEY_Y = 31;
  const SNAP_PX = 7;
  let lane = $state<HTMLDivElement>();
  let laneW = $state(0);

  const dur = $derived(app.duration || 0);
  const pts = $derived(app.zoomPoints);
  const pct = (t: number) => (viewLen > 0 ? ((t - viewStart) / viewLen) * 100 : 0);
  const yOf = (v: number) => PAD + v * (H - 2 * PAD);

  /** the centre across and down over the visible window, as SVG points (x in %, y in px) */
  const lines = $derived.by(() => {
    if (!pts.length || viewLen <= 0) return null;
    const c = zoomMath.clean(pts);
    const n = 160;
    const ts = [...Array.from({ length: n + 1 }, (_, i) => viewStart + (i / n) * viewLen), ...c.map((p) => p.t).filter((t) => t > viewStart && t < viewStart + viewLen)].sort((a, b) => a - b);
    const at = ts.map((t) => [t, zoomMath.at(c, t)] as const);
    return {
      x: at.map(([t, p]) => `${pct(t)},${yOf(p.x)}`).join(" "),
      y: at.map(([t, p]) => `${pct(t)},${yOf(p.y)}`).join(" "),
    };
  });

  function timeAt(e: { clientX: number }) {
    const b = lane!.getBoundingClientRect();
    const fx = b.width > 0 ? (e.clientX - b.left) / b.width : 0;
    return Math.min(Math.max(viewStart + fx * viewLen, 0), dur);
  }

  /** snap a time to range edges and the playhead, within SNAP_PX */
  function snapT(t: number, alt: boolean) {
    if (alt || !laneW || viewLen <= 0) return t;
    const reach = (SNAP_PX / laneW) * viewLen;
    let best = t;
    let bd = reach;
    for (const x of [app.current, ...app.segments.flatMap((g) => [g.start, g.end])]) {
      if (Math.abs(x - t) < bd) {
        bd = Math.abs(x - t);
        best = x;
      }
    }
    return best;
  }

  /** one keyframe dragged (i), or a selection (from: the keyframes when the drag began) */
  let drag = $state<{ i: number; moved: boolean; t0: number; x0: number; from: ZoomPoint[] | null; sel: number[] } | null>(null);
  /** a drag on the lane: the box's two times, and the selection it adds to (Ctrl); `x0` is
   *  where it began, and it is only a box once it has moved 4 px (a click adds a keyframe) */
  let box = $state<{ a: number; b: number; add: number[]; x0: number; moved: boolean; alt: boolean } | null>(null);
  /** the keyframe clicked last, by time (Shift+click selects from it) */
  let anchor: number | null = null;

  function down(e: PointerEvent, i: number | null) {
    if (e.button !== 0 || !app.media) return;
    e.preventDefault();
    e.stopPropagation();
    if (i === null) {
      lane!.setPointerCapture(e.pointerId);
      const t = timeAt(e);
      box = { a: t, b: t, add: e.ctrlKey || e.metaKey || e.shiftKey ? app.followSelected : [], x0: e.clientX, moved: false, alt: e.altKey };
      return;
    }
    if (i !== null) {
      const t = pts[i].t;
      if (e.ctrlKey || e.metaKey) {
        app.followSel = app.isFollowSel(t) ? app.followSelected.filter((s) => Math.abs(s - t) >= 1e-6) : [...app.followSelected, t];
        anchor = t;
        return;
      }
      if (e.shiftKey && anchor !== null) {
        const [lo, hi] = [Math.min(anchor, t), Math.max(anchor, t)];
        app.followSel = pts.filter((p) => p.t >= lo - 1e-6 && p.t <= hi + 1e-6).map((p) => p.t);
        return;
      }
      if (!app.isFollowSel(t)) app.followSel = [t];
      anchor = t;
    }
    lane!.setPointerCapture(e.pointerId);
    const sel = app.followSelected;
    const t0 = app.zoomPoints[i].t;
    drag = { i, moved: false, t0, x0: timeAt(e), from: sel.length > 1 ? app.zoomPoints.map((p) => ({ ...p })) : null, sel };
    app.seek(t0);
  }

  function move(e: PointerEvent) {
    if (box) {
      if (!box.moved && Math.abs(e.clientX - box.x0) < 4) return;
      box.moved = true;
      box.b = timeAt(e);
      return;
    }
    if (!drag) return;
    drag.moved = true;
    if (drag.from) {
      // the grabbed keyframe snaps; the others keep their distance from it
      const dt = snapT(drag.t0 + timeAt(e) - drag.x0, e.altKey) - drag.t0;
      app.moveFollowKeys(drag.from, drag.sel, dt);
      const at = drag.t0 + dt;
      drag.i = app.zoomPoints.findIndex((p) => Math.abs(p.t - at) < 1e-3);
    } else {
      app.setZoomPoint(drag.i, { t: snapT(timeAt(e), e.altKey) });
      if (app.zoomPoints[drag.i]) app.followSel = [app.zoomPoints[drag.i].t];
    }
  }

  function up(e: PointerEvent) {
    if (box) {
      lane!.releasePointerCapture(e.pointerId);
      if (!box.moved) {
        // a click: a keyframe there, selected, and the playhead on it
        const i = app.addFollowKey(snapT(box.a, box.alt));
        const t = app.zoomPoints[i].t;
        app.followSel = [t];
        anchor = t;
        app.seek(t);
        box = null;
        return;
      }
      const [lo, hi] = [Math.min(box.a, box.b), Math.max(box.a, box.b)];
      const inside = pts.filter((p) => p.t >= lo && p.t <= hi).map((p) => p.t);
      app.followSel = [...box.add.filter((s) => !inside.some((t) => Math.abs(t - s) < 1e-6)), ...inside];
      box = null;
      return;
    }
    if (!drag) return;
    lane!.releasePointerCapture(e.pointerId);
    // show the frame the keyframe is now on
    if (drag.moved && app.zoomPoints[drag.i]) app.seek(app.zoomPoints[drag.i].t);
    if (app.zoomPoints[drag.i]) anchor = app.zoomPoints[drag.i].t;
    drag = null;
  }

  function keyMenu(e: MouseEvent, i: number) {
    const p = pts[i];
    if (!p) return;
    const sel = app.followSelected;
    if (sel.length > 1 && app.isFollowSel(p.t)) {
      showMenu(e, [
        { label: `Select all ${pts.length} keyframes`, disabled: sel.length === pts.length, action: () => app.selectAllFollow() },
        { label: "Clear the selection", action: () => (app.followSel = []) },
        "sep",
        { label: `Remove the ${sel.length} selected keyframes`, danger: true, action: () => app.removeSelectedFollow() },
      ]);
      return;
    }
    const items: MenuItem[] = [
      { label: "Go to this keyframe", action: () => app.seek(p.t) },
      { label: "Move to the playhead", action: () => app.setZoomPoint(i, { t: app.current }) },
      "sep",
      { label: "Remove this keyframe", danger: true, action: () => app.removeFollowKey(i) },
    ];
    showMenu(e, items);
  }

  function laneMenu(e: MouseEvent) {
    const t = timeAt(e);
    showMenu(e, [
      { label: `Add a keyframe here (${fmtTime(t)})`, action: () => app.seek(app.zoomPoints[app.addFollowKey(t)].t) },
      { label: "Add a keyframe at the playhead", action: () => app.addFollowKey(app.current) },
      { label: "Track a person from the playhead (box)...", disabled: app.following !== null, action: ontrack },
      { label: "Track a face from the playhead...", disabled: app.following !== null, action: () => app.startPoints("face") },
      { label: "Track points from the playhead...", disabled: app.following !== null, action: () => app.startPoints() },
      "sep",
      { label: "Select all keyframes", disabled: !pts.length, action: () => app.selectAllFollow() },
      { label: `Remove the ${app.followSelected.length} selected keyframes`, danger: true, disabled: app.followSelected.length < 2, action: () => app.removeSelectedFollow() },
      { label: "Remove all keyframes", danger: true, disabled: !pts.length, action: () => app.clearFollow() },
    ]);
  }

  // shade the parts of the file that are not in the output
  const outside = $derived.by(() => {
    if (!app.segments.length) return [];
    const g = [...app.segments].sort((a, b) => a.start - b.start);
    const out: [number, number][] = [];
    let at = 0;
    for (const s of g) {
      if (s.start > at) out.push([at, s.start]);
      at = Math.max(at, s.end);
    }
    if (at < dur) out.push([at, dur]);
    return out;
  });
</script>

<div
  class="lane"
  bind:this={lane}
  bind:clientWidth={laneW}
  style:height="{H}px"
  role="presentation"
  onpointerdown={(e) => down(e, null)}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  oncontextmenu={laneMenu}
>
  {#each outside as [a, b] (a)}
    <span class="out" style:left="{pct(a)}%" style:width="{pct(b) - pct(a)}%"></span>
  {/each}
  {#if lines}
    <svg class="curve" viewBox="0 0 100 {H}" preserveAspectRatio="none" aria-hidden="true">
      <polyline class="cx" points={lines.x} vector-effect="non-scaling-stroke" />
      <polyline class="cy" points={lines.y} vector-effect="non-scaling-stroke" />
    </svg>
  {:else if app.following === null}
    <span class="hint">Follow: click to add a keyframe, then drag the picture to aim the zoomed view. Or Track a person to add keyframes that follow them. The wheel zooms the timeline.</span>
  {/if}
  <span class="head" style:left="{pct(app.current)}%"></span>
  {#if box?.moved}
    <span class="box" style:left="{pct(Math.min(box.a, box.b))}%" style:width="{Math.abs(pct(box.b) - pct(box.a))}%"></span>
  {/if}
  {#each pts as p, i (i)}
    {#if pct(p.t) >= -1 && pct(p.t) <= 101}
      <span
        class="key"
        class:on={drag?.i === i || app.followKeyAt(app.current) === i}
        class:sel={app.isFollowSel(p.t)}
        style:left="{pct(p.t)}%"
        style:top="{KEY_Y}px"
        role="slider"
        tabindex="-1"
        aria-label="Follow keyframe at {fmtTime(p.t)}"
        aria-valuenow={p.t}
        onpointerdown={(e) => down(e, i)}
        ondblclick={(e) => {
          e.stopPropagation();
          app.removeFollowKey(i);
        }}
        oncontextmenu={(e) => {
          e.stopPropagation();
          keyMenu(e, i);
        }}
      ></span>
      {#if drag?.i === i && drag.moved}
        <span class="val mono" style:left="{pct(p.t)}%" style:top="{KEY_Y - 24}px">{fmtTime(p.t)}</span>
      {/if}
    {/if}
  {/each}
  <div class="bar" role="presentation" onpointerdown={(e) => e.stopPropagation()} oncontextmenu={(e) => e.stopPropagation()}>
    {#if app.following !== null}
      <span class="mono small" title="Tracking with {app.trackingWith}">Tracking {Math.round(app.following * 100)}%</span>
      <button class="btn small" onclick={() => app.cancelFollow()}>Stop</button>
    {:else if app.followMode}
      <span class="small">Draw a box on the picture</span>
      <button class="btn small" onclick={() => app.cancelFollow()}>Cancel</button>
    {:else if app.pointsMode}
      <span class="small">Click points on the picture</span>
    {:else}
      {#if pts.length}
        <span class="mono small muted" title="Drag on the lane to select several (Ctrl+click, Shift+click also); Backspace or Delete removes them. The wheel zooms the timeline.">{app.followSelected.length > 1 ? `${app.followSelected.length} of ${pts.length} selected` : `${pts.length} key${pts.length === 1 ? "" : "s"}`}</span>
        <label class="fz" title="Follow's own zoom, for all its keyframes (the sidebar's zoom is for the picture without Follow). Double-click for 2x.">
          <span class="small">Zoom</span>
          <input
            type="range"
            min="1"
            max="4"
            step="0.05"
            aria-label="Follow zoom"
            value={app.followZoom}
            oninput={(e) => app.setFollowZoom(Number((e.currentTarget as HTMLInputElement).value))}
            ondblclick={() => app.setFollowZoom(2)}
          />
          <span class="mono small">{app.followZoom.toFixed(2)}x</span>
        </label>
      {/if}
      <button class="btn small" disabled={!app.media || app.audioOnly} title="Draw a box around a person; the app follows them from the playhead to the end of the range and adds keyframes along the way, with a steady camera" onclick={ontrack}>Track box...</button>
      <button class="btn small" disabled={!app.media || app.audioOnly} title="Click on a face: the view stays on it and turns to keep the eyes level, found afresh in every frame (OpenCV's face finder; needs VitTrack from Settings > Tools > Tracking)" onclick={() => app.startPoints("face")}>Track face...</button>
      <button class="btn small" disabled={!app.media || app.audioOnly} title="Click one point to follow (the view stays centred on it), or two to also turn the view so the line between them stays level. For a face, Track face holds better." onclick={() => app.startPoints()}>Track points...</button>
    {/if}
  </div>
</div>

<style>
  .lane {
    position: relative;
    margin-top: 6px;
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    overflow: hidden;
    cursor: crosshair;
    touch-action: none;
    user-select: none;
  }
  .out {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.28);
    pointer-events: none;
  }
  .curve {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
    pointer-events: none;
  }
  .curve polyline {
    fill: none;
    stroke-width: 1.5;
  }
  .cx {
    stroke: var(--accent);
  }
  .cy {
    stroke: var(--text-secondary);
    stroke-dasharray: 4 3;
    opacity: 0.8;
  }
  .hint {
    position: absolute;
    left: 8px;
    right: 150px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 11px;
    color: var(--text-muted);
    pointer-events: none;
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--text-secondary);
    opacity: 0.6;
    pointer-events: none;
  }
  .key {
    position: absolute;
    width: 9px;
    height: 9px;
    margin: -5px 0 0 -5px;
    border: 2px solid var(--accent);
    background: var(--bg-panel);
    transform: rotate(45deg);
    cursor: grab;
  }
  .key.on,
  .key:hover {
    background: var(--accent);
  }
  /* selected: a ring around the diamond */
  .key.sel {
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--bg-input), 0 0 0 3.5px var(--text-primary);
  }
  .box {
    position: absolute;
    top: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border: 1px solid var(--accent);
    pointer-events: none;
  }
  .val {
    position: absolute;
    transform: translateX(-50%);
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.75);
    color: #fff;
    font-size: 10px;
    white-space: nowrap;
    pointer-events: none;
  }
  .bar {
    position: absolute;
    top: 3px;
    right: 4px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 0 0 6px;
    border-radius: 4px;
    background: var(--bg-input);
    cursor: default;
  }
  .fz {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .fz input {
    width: 90px;
  }
  .bar .btn {
    height: 18px;
    padding: 0 7px;
    font-size: 11px;
  }
</style>
