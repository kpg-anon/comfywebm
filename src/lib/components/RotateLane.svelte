<script lang="ts">
  // The Rotate lane under the timeline: rotation keyframes over file time, on
  // the timeline's own zoom window, as a graph of the angle (up is clockwise).
  // The angle scale fits the keyframes, with a quarter turn to spare above and
  // below, and follows a turn past 180 degrees (270 is drawn above 180, not as
  // -90), so the line never jumps across the lane; it holds still while a
  // keyframe is dragged. Before the first keyframe the angle is the Rotate
  // setting; at a keyframe it changes at once, or (smooth, a diamond) turns
  // from the previous keyframe's angle over the time between them.
  // Click the lane to add a keyframe, click one to edit it in the row under
  // the lane (exact time and angle, at once or smoothly), drag one (time and angle;
  // a drag along the lane keeps the angle). Angles snap to quarter turns and
  // 15 degree steps, times to range edges and the playhead (Alt turns snapping
  // off). A keyframe sits on a frame and turns that frame.
  import { app } from "$lib/state.svelte";
  import * as rot from "$lib/rotation";
  import { showMenu, type MenuItem } from "$lib/menu.svelte";
  import { fmtTime } from "$lib/time";
  import KeyBar from "./KeyBar.svelte";
  import Seg from "./Seg.svelte";

  let { viewStart, viewLen }: { viewStart: number; viewLen: number } = $props();

  const H = 110;
  const PAD = 8; // room above the top line and below the bottom one for the keyframes
  const SNAP_PX = 7;
  let lane = $state<HTMLDivElement>();
  let laneW = $state(0);

  const dur = $derived(app.duration || 0);
  const pts = $derived(app.rotatePoints);
  /** the setting's angle and each keyframe's, as the encode turns them (not wrapped) */
  const path = $derived(rot.keys(app.rotate, pts));
  const pct = (t: number) => (viewLen > 0 ? ((t - viewStart) / viewLen) * 100 : 0);

  /** the angle scale: whole quarter turns around all the angles, one more on each side */
  function fitRange(): { lo: number; hi: number } {
    const all = [path.b, ...path.k.map((k) => k.a)];
    let lo = Math.floor(Math.min(...all) / 90) * 90 - 90;
    let hi = Math.ceil(Math.max(...all) / 90) * 90 + 90;
    if (hi - lo < 270) [lo, hi] = [lo - 45, hi + 45];
    return { lo, hi };
  }
  /** held while a keyframe is dragged, so the scale does not move under the pointer */
  let held = $state<{ lo: number; hi: number } | null>(null);
  const range = $derived(held ?? fitRange());
  const yOf = (a: number) => PAD + ((range.hi - a) / (range.hi - range.lo)) * (H - 2 * PAD);
  const angleOfY = (y: number) => range.hi - ((y - PAD) / (H - 2 * PAD)) * (range.hi - range.lo);
  /** a line and its label at each quarter turn in the scale */
  const lines = $derived.by(() => {
    const out: number[] = [];
    for (let a = Math.ceil(range.lo / 90) * 90; a <= range.hi; a += 90) out.push(a);
    return out;
  });

  /** the angle across the visible window, as an SVG path (x in %, y in px) */
  const curve = $derived.by(() => {
    if (!pts.length || viewLen <= 0) return "";
    const c = rot.clean(pts);
    const end = viewStart + viewLen;
    const ts: number[] = [viewStart, end];
    c.forEach((p, i) => {
      // just before each keyframe too, so a change at once is drawn upright
      ts.push(p.t - 1e-6, p.t);
      const n = c[i + 1];
      if (n?.smooth) for (let k = 1; k < 40; k++) ts.push(p.t + ((n.t - p.t) * k) / 40);
    });
    return ts
      .filter((t) => t >= viewStart && t <= end)
      .sort((a, b) => a - b)
      .map((t, i) => `${i ? "L" : "M"}${pct(t)},${yOf(rot.angleAt(app.rotate, c, t))}`)
      .join("");
  });

  function at(e: { clientX: number; clientY: number }) {
    const b = lane!.getBoundingClientRect();
    const fx = b.width > 0 ? (e.clientX - b.left) / b.width : 0;
    // the rect is on-screen pixels; H is in page pixels (the UI scale zooms the page)
    const y = ((e.clientY - b.top) / Math.max(1, b.height)) * H;
    return { t: Math.min(Math.max(viewStart + fx * viewLen, 0), dur), angle: angleOfY(y) };
  }

  /** quarter turns within 10 degrees, else 15 degree steps within 3 (Alt: whole degrees) */
  function snapA(a: number, alt: boolean) {
    if (alt) return Math.round(a);
    const q = Math.round(a / 90) * 90;
    if (Math.abs(a - q) < 10) return q;
    const s = Math.round(a / 15) * 15;
    return Math.abs(a - s) < 3 ? s : Math.round(a);
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

  /** The angle to store for keyframe i dragged to `a`: kept within a half turn of the
   *  previous keyframe's angle, which is the way the encode turns to it. */
  function reachable(i: number, a: number) {
    const prev = i > 0 ? path.k[i - 1]?.a : path.b;
    if (prev === undefined) return a;
    return Math.min(prev + 180, Math.max(prev - 179, a));
  }

  /** a keyframe dragged; its angle changes only once the pointer has gone 4 px up or down,
   *  so a drag along the lane moves it in time and keeps its angle */
  let drag = $state<{ i: number; moved: boolean; y0: number; free: boolean } | null>(null);

  function down(e: PointerEvent, i: number | null) {
    if (e.button !== 0 || !app.media) return;
    e.preventDefault();
    e.stopPropagation();
    lane!.setPointerCapture(e.pointerId);
    held = fitRange();
    if (i === null) {
      const p = at(e);
      i = app.addRotateKey(snapT(p.t, e.altKey));
      // the clicked height, reached the short way from the keyframe before
      app.setRotateKey(i, { angle: snapA(reachable(i, p.angle), e.altKey) });
    }
    app.rotateSel = i;
    drag = { i, moved: false, y0: e.clientY, free: false };
    const k = app.rotatePoints[i];
    if (k) app.seek(k.t);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const p = at(e);
    drag.moved = true;
    if (Math.abs(e.clientY - drag.y0) > 4) drag.free = true;
    app.setRotateKey(drag.i, drag.free ? { t: snapT(p.t, e.altKey), angle: snapA(reachable(drag.i, p.angle), e.altKey) } : { t: snapT(p.t, e.altKey) });
  }

  function up(e: PointerEvent) {
    held = null;
    if (!drag) return;
    lane!.releasePointerCapture(e.pointerId);
    // show the frame the keyframe is now on
    const k = app.rotatePoints[drag.i];
    if (drag.moved && k) app.seek(k.t);
    drag = null;
  }

  /** the keyframe open in the row under the lane, or null */
  const ki = $derived(app.rotateSel !== null && app.rotatePoints[app.rotateSel] && path.k[app.rotateSel] ? app.rotateSel : null);
  const kp = $derived(ki === null ? null : app.rotatePoints[ki]);
  /** its angle as the lane draws it (a turn past 180 goes on to 270) */
  const kAngle = $derived(ki === null ? 0 : Math.round((path.k[ki]?.a ?? 0) * 10) / 10);

  function goKey(k: number) {
    const q = app.rotatePoints[k];
    if (!q) return;
    app.rotateSel = k;
    app.seek(q.t);
  }

  function keyMenu(e: MouseEvent, i: number) {
    const p = pts[i];
    if (!p) return;
    app.rotateSel = i;
    const items: MenuItem[] = [
      ...[0, 90, 180, -90].map((a) => ({ label: `${a}°`, disabled: Math.abs(rot.norm(p.angle) - rot.norm(a)) < 1e-6, action: () => app.setRotateKey(i, { angle: a }) })),
      "sep",
      p.smooth
        ? { label: "Change at once on this frame", action: () => app.setRotateKey(i, { smooth: false }) }
        : { label: i > 0 ? "Turn smoothly from the previous keyframe" : "Turn smoothly (needs a keyframe before it)", disabled: i === 0, action: () => app.setRotateKey(i, { smooth: true }) },
      { label: "Go to this keyframe", action: () => goKey(i) },
      { label: "Move to the playhead", action: () => app.setRotateKey(i, { t: app.current }) },
      { label: "Remove this keyframe", danger: true, action: () => app.removeRotateKey(i) },
    ];
    showMenu(e, items);
  }

  function laneMenu(e: MouseEvent) {
    const p = at(e);
    const now = app.rotateAt(app.current);
    const add = (a: number) => () => (app.rotateSel = app.addRotateKey(app.current, a));
    showMenu(e, [
      { label: `Turn 90° clockwise at the playhead (to ${Math.round(rot.norm(now + 90))}°)`, action: add(now + 90) },
      { label: `Turn 90° counterclockwise at the playhead (to ${Math.round(rot.norm(now - 90))}°)`, action: add(now - 90) },
      {
        label: `Add a keyframe here (${fmtTime(p.t)})`,
        action: () => {
          const i = app.addRotateKey(p.t);
          app.rotateSel = i;
          app.seek(app.rotatePoints[i].t);
        },
      },
      "sep",
      { label: "Picture: fit in the frame", disabled: !app.frameFill, action: () => (app.frameFill = false) },
      { label: "Picture: fill the frame, edges cut", disabled: app.frameFill, action: () => (app.frameFill = true) },
      "sep",
      { label: "Remove all keyframes", danger: true, disabled: !pts.length, action: () => app.clearRotateKeys() },
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
  class:unused={app.zoomPoints.length > 0}
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
  {#each lines as a (a)}
    <span class="grid" class:zero={a % 360 === 0} style:top="{yOf(a)}px"></span>
  {/each}
  {#if curve}
    <svg class="curve" viewBox="0 0 100 {H}" preserveAspectRatio="none" aria-hidden="true">
      <path d={curve} vector-effect="non-scaling-stroke" />
    </svg>
  {/if}
  <!-- over the line, so they stay readable where it passes; the keyframes are drawn over them -->
  {#each lines as a (a)}
    <span class="lbl mono" style:top="{yOf(a)}px">{a}°</span>
  {/each}
  {#if app.zoomPoints.length}
    <span class="hint">Not used while Follow has keyframes: Follow turns its own view.</span>
  {:else if !pts.length}
    <span class="hint">Click where the picture should turn to add a keyframe (higher is clockwise), then set it exactly in the row that opens under the lane. Right-click for a 90° turn at the playhead.</span>
  {/if}
  <span class="head" style:left="{pct(app.current)}%"></span>
  {#each pts as p, i (i)}
    {#if pct(p.t) >= -1 && pct(p.t) <= 101 && path.k[i]}
      <span
        class="pt"
        class:smooth={p.smooth}
        class:on={drag?.i === i || app.rotateSel === i}
        style:left="{pct(p.t)}%"
        style:top="{yOf(path.k[i].a)}px"
        role="slider"
        tabindex="-1"
        title="{Math.round(path.k[i].a)}° at {fmtTime(p.t)}{p.smooth ? ', turning from the previous keyframe' : ''}"
        aria-label="Rotation keyframe at {fmtTime(p.t)}"
        aria-valuenow={p.angle}
        onpointerdown={(e) => down(e, i)}
        oncontextmenu={(e) => {
          e.stopPropagation();
          keyMenu(e, i);
        }}
      ></span>
      {#if drag?.i === i && drag.moved}
        <span class="val mono" style:left="{pct(p.t)}%" style:top="{yOf(path.k[i].a) < H / 2 ? yOf(path.k[i].a) + 10 : yOf(path.k[i].a) - 24}px">{Math.round(path.k[i].a)}° · {fmtTime(p.t)}</span>
      {/if}
    {/if}
  {/each}
</div>
<!-- always there while the lane is, so opening a keyframe does not move the lane -->
<KeyBar
  i={ki}
  n={pts.length}
  t={kp?.t ?? null}
  onprev={() => ki !== null && goKey(ki - 1)}
  onnext={() => ki !== null && goKey(ki + 1)}
  ontime={(v) => {
    if (ki === null) return;
    app.setRotateKey(ki, { t: v });
    app.seek(app.rotatePoints[ki].t);
  }}
  onstep={(f) => ki !== null && app.stepRotateKey(ki, f)}
  onremove={() => ki !== null && app.removeRotateKey(ki)}
  onclose={() => (app.rotateSel = null)}
>
  <span class="kpart">
    <span class="klbl">Angle</span>
    {#each [-90, 0, 90, 180] as a (a)}
      <button class="btn small" disabled={!kp} class:on={!!kp && Math.abs(rot.norm(kp.angle) - rot.norm(a)) < 1e-6} onclick={() => ki !== null && app.setRotateKey(ki, { angle: a })}>{a}°</button>
    {/each}
    <input
      class="field mono deg"
      type="number"
      step="1"
      disabled={!kp}
      value={kp ? kAngle : ""}
      title="Any angle, clockwise. Up and Down change it by 1°."
      aria-label="Angle in degrees, clockwise"
      onchange={(e) => {
        const v = Number((e.currentTarget as HTMLInputElement).value);
        if (ki !== null && Number.isFinite(v)) app.setRotateKey(ki, { angle: v });
      }}
    />°
  </span>
  <span class="kpart" title="How the picture gets to this angle from the keyframe before: on this frame, or turning over the time between the two">
    <span class="klbl">Turn</span>
    {#if ki === 0}
      <span class="muted turnnote">at once (first keyframe)</span>
    {:else}
      <Seg
        label="How it turns"
        value={kp?.smooth ? "smooth" : "once"}
        onchange={(v) => ki !== null && app.setRotateKey(ki, { smooth: v === "smooth" })}
        options={[
          { value: "once", label: "At once", title: "Changes on this keyframe's frame.", disabled: !kp },
          { value: "smooth", label: "Smoothly", title: "Turns from the previous keyframe's angle over the time between them.", disabled: !kp },
        ]}
      />
    {/if}
  </span>
</KeyBar>

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
  .lane.unused .curve,
  .lane.unused .pt {
    opacity: 0.4;
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
    left: 40px;
    right: 0;
    height: 0;
    border-top: 1px dashed var(--border);
    opacity: 0.6;
    pointer-events: none;
  }
  .grid.zero {
    border-top-style: solid;
    opacity: 1;
  }
  /* each label on its line's left end, in the margin the lines leave free */
  .lbl {
    position: absolute;
    left: 2px;
    padding: 1px 3px;
    border-radius: 3px;
    background: var(--bg-input);
    transform: translateY(-50%);
    font-size: 9.5px;
    line-height: 1;
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
  .curve path {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
  }
  .hint {
    position: absolute;
    left: 48px;
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
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border: 2px solid var(--accent);
    border-radius: 50%;
    background: var(--bg-panel);
    cursor: grab;
  }
  .pt.smooth {
    border-radius: 2px;
    transform: rotate(45deg);
  }
  .pt.on,
  .pt:hover {
    background: var(--accent);
  }
  .deg {
    width: 64px;
  }
  /* about as wide as the At once / Smoothly buttons, so the row keeps its layout */
  .turnnote {
    display: inline-block;
    min-width: 150px;
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
