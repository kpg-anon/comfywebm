<script lang="ts">
  // The speed ramp lane under the timeline: speed points over file time, on
  // the timeline's own zoom window. Up is faster (log scale, 0.25x to 4x, 1x
  // in the middle). Click to add a point, drag a point (time and speed),
  // double-click or right-click it to remove or set it. Between two points
  // the speed changes in a straight line; before the first and after the
  // last it stays at that point's speed. Speeds snap to common ones (1/2x,
  // 1x, 2x...); times snap to the range edges and the playhead (Alt turns
  // snapping off).
  import { app } from "$lib/state.svelte";
  import * as ramp from "$lib/ramp";
  import { showMenu, type MenuItem } from "$lib/menu.svelte";
  import { fmtTime } from "$lib/time";

  let { viewStart, viewLen }: { viewStart: number; viewLen: number } = $props();

  const H = 54;
  const PAD = 6; // room above 4x and below 0.25x for the points
  const SNAP_PX = 7;
  let lane = $state<HTMLDivElement>();
  let laneW = $state(0);

  const dur = $derived(app.duration || 0);
  const pts = $derived(app.speedRamp);
  const pct = (t: number) => (viewLen > 0 ? ((t - viewStart) / viewLen) * 100 : 0);
  const yOf = (s: number) => PAD + (1 - ramp.speedToY(s)) * (H - 2 * PAD);
  const speedOfY = (y: number) => ramp.yToSpeed(1 - (y - PAD) / (H - 2 * PAD));

  /** the curve across the visible window, as SVG points (x in %, y in px) */
  const curve = $derived.by(() => {
    if (!pts.length || viewLen <= 0) return "";
    const c = ramp.clean(pts);
    const ts = [viewStart, ...c.map((p) => p.t).filter((t) => t > viewStart && t < viewStart + viewLen), viewStart + viewLen];
    return ts.map((t) => `${pct(t)},${yOf(ramp.speedAt(c, t))}`).join(" ");
  });

  function at(e: { clientX: number; clientY: number }) {
    const b = lane!.getBoundingClientRect();
    const fx = b.width > 0 ? (e.clientX - b.left) / b.width : 0;
    // the rect is on-screen pixels; H is in page pixels (the UI scale zooms the page)
    const y = ((e.clientY - b.top) / Math.max(1, b.height)) * H;
    return { t: Math.min(Math.max(viewStart + fx * viewLen, 0), dur), speed: speedOfY(y) };
  }

  /** common speeds a dragged point snaps to when it is within 4% of one (Alt: no snapping) */
  const SPEEDS = [0.25, 0.4, 0.5, 0.75, 0.8, 1, 1.25, 1.5, 2, 3, 4];
  function snapS(s: number, alt: boolean) {
    if (alt) return s;
    for (const x of SPEEDS) if (Math.abs(Math.log(s / x)) < 0.04) return x;
    return s;
  }

  /** snap a time to range edges and the playhead, within SNAP_PX */
  function snapT(t: number, alt: boolean) {
    if (alt || !laneW || viewLen <= 0) return t;
    const reach = (SNAP_PX / laneW) * viewLen;
    let best = t;
    let bd = reach;
    const targets = [app.current, ...app.segments.flatMap((g) => [g.start, g.end])];
    for (const x of targets) {
      if (Math.abs(x - t) < bd) {
        bd = Math.abs(x - t);
        best = x;
      }
    }
    return best;
  }

  let drag = $state<{ i: number; moved: boolean } | null>(null);

  function down(e: PointerEvent, i: number | null) {
    if (e.button !== 0 || !app.media) return;
    e.preventDefault();
    e.stopPropagation();
    lane!.setPointerCapture(e.pointerId);
    if (i === null) {
      const p = at(e);
      i = app.addRampPoint(snapT(p.t, e.altKey), snapS(p.speed, e.altKey));
    }
    drag = { i, moved: false };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const p = at(e);
    drag.moved = true;
    app.setRampPoint(drag.i, snapT(p.t, e.altKey), snapS(p.speed, e.altKey));
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    lane!.releasePointerCapture(e.pointerId);
    drag = null;
  }

  function pointMenu(e: MouseEvent, i: number) {
    const p = pts[i];
    if (!p) return;
    const set = (s: number) => () => app.setRampPoint(i, p.t, s);
    const items: MenuItem[] = [
      ...[0.25, 0.4, 0.5, 0.8, 1, 2, 4].map((s) => ({ label: `${s}x`, disabled: p.speed === s, action: set(s) })),
      "sep",
      { label: "Move to the playhead", action: () => app.setRampPoint(i, app.current, p.speed) },
      { label: "Remove this point", danger: true, action: () => app.removeRampPoint(i) },
    ];
    showMenu(e, items);
  }

  function laneMenu(e: MouseEvent) {
    const p = at(e);
    showMenu(e, [
      { label: `Add a point here (${fmtTime(p.t)})`, action: () => app.addRampPoint(p.t, p.speed) },
      { label: "Add a point at the playhead", action: () => app.addRampPoint(app.current) },
      "sep",
      { label: "Remove all points", danger: true, disabled: !pts.length, action: () => app.clearRamp() },
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
  <span class="grid" style:top="{yOf(1)}px"></span>
  <span class="grid faint" style:top="{yOf(2)}px"></span>
  <span class="grid faint" style:top="{yOf(0.5)}px"></span>
  <span class="lbl mono" style:top="{yOf(4) - 6}px">4x</span>
  <span class="lbl mono" style:top="{yOf(1) - 6}px">1x</span>
  <span class="lbl mono" style:top="{yOf(0.25) - 6}px">¼x</span>
  {#if curve}
    <svg class="curve" viewBox="0 0 100 {H}" preserveAspectRatio="none" aria-hidden="true">
      <polyline points={curve} vector-effect="non-scaling-stroke" />
    </svg>
  {:else}
    <span class="hint">Click to add a speed point. Drag it up for faster, down for slower; add more to ramp between them.</span>
  {/if}
  <span class="head" style:left="{pct(app.current)}%"></span>
  {#each pts as p, i (i)}
    {#if pct(p.t) >= -1 && pct(p.t) <= 101}
      <span
        class="pt"
        class:on={drag?.i === i}
        style:left="{pct(p.t)}%"
        style:top="{yOf(p.speed)}px"
        role="slider"
        tabindex="-1"
        aria-label="Speed point at {fmtTime(p.t)}"
        aria-valuenow={p.speed}
        onpointerdown={(e) => down(e, i)}
        ondblclick={(e) => {
          e.stopPropagation();
          app.removeRampPoint(i);
        }}
        oncontextmenu={(e) => pointMenu(e, i)}
      ></span>
      {#if drag?.i === i}
        <span class="val mono" style:left="{pct(p.t)}%" style:top="{yOf(p.speed) < H / 2 ? yOf(p.speed) + 8 : yOf(p.speed) - 22}px">{p.speed}x · {fmtTime(p.t)}</span>
      {/if}
    {/if}
  {/each}
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
  .grid {
    position: absolute;
    left: 0;
    right: 0;
    height: 0;
    border-top: 1px dashed var(--border);
    pointer-events: none;
  }
  .grid.faint {
    opacity: 0.45;
  }
  .lbl {
    position: absolute;
    left: 4px;
    font-size: 9.5px;
    color: var(--text-muted);
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
    stroke: var(--accent);
    stroke-width: 2;
  }
  .hint {
    position: absolute;
    left: 40px;
    right: 8px;
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
  .pt {
    position: absolute;
    width: 11px;
    height: 11px;
    margin: -6px 0 0 -6px;
    border: 2px solid var(--accent);
    border-radius: 50%;
    background: var(--bg-panel);
    cursor: grab;
  }
  .pt.on,
  .pt:hover {
    background: var(--accent);
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
</style>
