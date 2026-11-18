<script lang="ts">
  // The zoom lane under the timeline: zoom points over file time, on the
  // timeline's own zoom window. Up is closer (1x at the bottom, 4x at the
  // top). Click to add a point, drag a point (time and zoom), double-click or
  // right-click it to remove or set it. Clicking a point selects it and moves
  // the playhead to it; its window on the picture can then be dragged (what
  // shows) and the wheel over it changes the zoom. Between two points zoom
  // and centre ease (engine zoom.rs).
  import { app } from "$lib/state.svelte";
  import * as zm from "$lib/zoom";
  import { showMenu, type MenuItem } from "$lib/menu.svelte";
  import { fmtTime } from "$lib/time";

  let { viewStart, viewLen }: { viewStart: number; viewLen: number } = $props();

  const H = 54;
  const PAD = 6;
  const SNAP_PX = 7;
  let lane = $state<HTMLDivElement>();
  let laneW = $state(0);

  const dur = $derived(app.duration || 0);
  const pts = $derived(app.zoomPoints);
  const pct = (t: number) => (viewLen > 0 ? ((t - viewStart) / viewLen) * 100 : 0);
  const yOf = (z: number) => PAD + (1 - (z - zm.MIN_ZOOM) / (zm.MAX_ZOOM - zm.MIN_ZOOM)) * (H - 2 * PAD);
  const zoomOfY = (y: number) => zm.MIN_ZOOM + (1 - (y - PAD) / (H - 2 * PAD)) * (zm.MAX_ZOOM - zm.MIN_ZOOM);

  /** the zoom across the visible window, sampled (it eases between points) */
  const curve = $derived.by(() => {
    if (!pts.length || viewLen <= 0) return "";
    const out: string[] = [];
    for (let k = 0; k <= 120; k++) {
      const t = viewStart + (k / 120) * viewLen;
      out.push(`${pct(t)},${yOf(zm.at(pts, t).zoom)}`);
    }
    return out.join(" ");
  });

  function at(e: { clientX: number; clientY: number }) {
    const b = lane!.getBoundingClientRect();
    const fx = b.width > 0 ? (e.clientX - b.left) / b.width : 0;
    const y = ((e.clientY - b.top) / Math.max(1, b.height)) * H;
    return { t: Math.min(Math.max(viewStart + fx * viewLen, 0), dur), zoom: zoomOfY(y) };
  }

  /** zooms a dragged point snaps to (Alt: no snapping) */
  const ZOOMS = [1, 1.25, 1.5, 2, 2.5, 3, 4];
  function snapZ(z: number, alt: boolean) {
    if (alt) return z;
    for (const x of ZOOMS) if (Math.abs(z - x) < 0.08) return x;
    return Math.round(z * 100) / 100;
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

  let drag = $state<{ i: number; moved: boolean } | null>(null);

  function down(e: PointerEvent, i: number | null) {
    if (e.button !== 0 || !app.media) return;
    e.preventDefault();
    e.stopPropagation();
    lane!.setPointerCapture(e.pointerId);
    if (i === null) {
      const p = at(e);
      i = app.addZoomPoint(snapT(p.t, e.altKey), snapZ(p.zoom, e.altKey));
    }
    app.zoomSel = i;
    app.seek(app.zoomPoints[i]?.t ?? app.current);
    drag = { i, moved: false };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const p = at(e);
    drag.moved = true;
    app.setZoomPoint(drag.i, { t: snapT(p.t, e.altKey), zoom: snapZ(p.zoom, e.altKey) });
    app.seek(app.zoomPoints[drag.i]?.t ?? app.current);
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    lane!.releasePointerCapture(e.pointerId);
    drag = null;
  }

  function pointMenu(e: MouseEvent, i: number) {
    const p = pts[i];
    if (!p) return;
    app.zoomSel = i;
    const items: MenuItem[] = [
      ...[1, 1.5, 2, 3, 4].map((z) => ({ label: `${z}x`, disabled: p.zoom === z, action: () => app.setZoomPoint(i, { zoom: z }) })),
      "sep",
      { label: "Centre the window", action: () => app.setZoomPoint(i, { x: 0.5, y: 0.5 }) },
      { label: "Move to the playhead", action: () => app.setZoomPoint(i, { t: app.current }) },
      { label: "Remove this point", danger: true, action: () => app.removeZoomPoint(i) },
    ];
    showMenu(e, items);
  }

  function laneMenu(e: MouseEvent) {
    const p = at(e);
    showMenu(e, [
      { label: `Add a point here (${fmtTime(p.t)})`, action: () => app.addZoomPoint(p.t, snapZ(p.zoom, false)) },
      { label: "Add a point at the playhead", action: () => app.addZoomPoint(app.current) },
      "sep",
      { label: "Remove all points", danger: true, disabled: !pts.length, action: () => app.clearZoom() },
    ]);
  }

  // shade the parts of the file that are not in the output
  const outside = $derived.by(() => {
    if (!app.segments.length) return [];
    const g = [...app.segments].sort((a, b) => a.start - b.start);
    const out: [number, number][] = [];
    let t = 0;
    for (const s of g) {
      if (s.start > t) out.push([t, s.start]);
      t = Math.max(t, s.end);
    }
    if (t < dur) out.push([t, dur]);
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
  <span class="grid faint" style:top="{yOf(2)}px"></span>
  <span class="grid faint" style:top="{yOf(3)}px"></span>
  <span class="lbl mono" style:top="{yOf(4) - 6}px">4x</span>
  <span class="lbl mono" style:top="{yOf(2) - 6}px">2x</span>
  <span class="lbl mono" style:top="{yOf(1) - 6}px">1x</span>
  {#if curve}
    <svg class="curve" viewBox="0 0 100 {H}" preserveAspectRatio="none" aria-hidden="true">
      <polyline points={curve} vector-effect="non-scaling-stroke" />
    </svg>
  {:else}
    <span class="hint">Click to add a zoom point (higher is closer), then drag its window on the picture to choose what shows. Add more to move and zoom between them.</span>
  {/if}
  <span class="head" style:left="{pct(app.current)}%"></span>
  {#each pts as p, i (i)}
    {#if pct(p.t) >= -1 && pct(p.t) <= 101}
      <span
        class="pt"
        class:on={drag?.i === i}
        class:sel={app.zoomSel === i}
        style:left="{pct(p.t)}%"
        style:top="{yOf(p.zoom)}px"
        role="slider"
        tabindex="-1"
        aria-label="Zoom point at {fmtTime(p.t)}"
        aria-valuenow={p.zoom}
        onpointerdown={(e) => down(e, i)}
        ondblclick={(e) => {
          e.stopPropagation();
          app.removeZoomPoint(i);
        }}
        oncontextmenu={(e) => pointMenu(e, i)}
      ></span>
      {#if drag?.i === i}
        <span class="val mono" style:left="{pct(p.t)}%" style:top="{yOf(p.zoom) < H / 2 ? yOf(p.zoom) + 8 : yOf(p.zoom) - 22}px">{p.zoom}x · {fmtTime(p.t)}</span>
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
  .pt:hover,
  .pt.sel {
    background: var(--accent);
  }
  .pt.sel {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent);
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
