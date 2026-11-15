<script lang="ts">
  // The master curve: drag a point, click an empty spot to add one,
  // double-click a point (not an end) to remove it. Input runs left to right,
  // output bottom to top, both 0..1.
  import { normalize, sample, type Pt } from "$lib/curve";

  let { points, onchange }: { points: Pt[]; onchange: (p: Pt[]) => void } = $props();

  let svg: SVGSVGElement;
  let drag = $state<number | null>(null);

  const pts = $derived(normalize(points));
  const line = $derived(
    sample(pts, 96)
      .map((y, i) => `${((i / 95) * 200).toFixed(2)},${(100 - y * 100).toFixed(2)}`)
      .join(" "),
  );

  function at(e: PointerEvent): Pt {
    const b = svg.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (e.clientX - b.left) / Math.max(1, b.width)));
    const y = Math.min(1, Math.max(0, 1 - (e.clientY - b.top) / Math.max(1, b.height)));
    return [x, y];
  }

  /** Move point i to p, keeping it between its neighbors; the ends keep their x. */
  function place(list: Pt[], i: number, p: Pt): Pt[] {
    const out = list.map((q) => [...q] as Pt);
    const lo = i === 0 ? 0 : out[i - 1][0] + 0.01;
    const hi = i === out.length - 1 ? 1 : out[i + 1][0] - 0.01;
    const x = i === 0 ? 0 : i === out.length - 1 ? 1 : Math.min(hi, Math.max(lo, p[0]));
    out[i] = [Math.round(x * 1000) / 1000, Math.round(p[1] * 1000) / 1000];
    return out;
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    svg.setPointerCapture(e.pointerId);
    const p = at(e);
    const b = svg.getBoundingClientRect();
    // a point within 8 px is grabbed; otherwise a new point is added there
    const near = pts.findIndex(([x, y]) => Math.hypot((x - p[0]) * b.width, (y - p[1]) * b.height) < 8);
    if (near >= 0) {
      drag = near;
      return;
    }
    const list = normalize([...pts, p]);
    drag = list.findIndex(([x]) => Math.abs(x - p[0]) < 1e-4);
    onchange(list);
  }

  function move(e: PointerEvent) {
    if (drag === null) return;
    onchange(place(pts, drag, at(e)));
  }

  function up(e: PointerEvent) {
    if (drag === null) return;
    svg.releasePointerCapture(e.pointerId);
    drag = null;
  }

  function remove(i: number) {
    if (i <= 0 || i >= pts.length - 1) return;
    onchange(pts.filter((_, k) => k !== i));
  }
</script>

<svg
  bind:this={svg}
  class="curve"
  viewBox="0 0 200 100"
  role="application"
  aria-label="Curve: drag points, click to add, double-click to remove"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
>
  {#each [25, 50, 75] as g (g)}
    <line class="grid" x1={g * 2} y1="0" x2={g * 2} y2="100" />
    <line class="grid" x1="0" y1={g} x2="200" y2={g} />
  {/each}
  <line class="diag" x1="0" y1="100" x2="200" y2="0" />
  <polyline class="line" points={line} />
  {#each pts as [x, y], i (i)}
    <circle
      class="pt"
      class:on={drag === i}
      cx={x * 200}
      cy={100 - y * 100}
      r="2.6"
      role="button"
      tabindex="-1"
      ondblclick={() => remove(i)}
    />
  {/each}
</svg>

<style>
  .curve {
    width: 100%;
    aspect-ratio: 2 / 1;
    display: block;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    cursor: crosshair;
    touch-action: none;
  }
  .grid {
    stroke: var(--border-muted);
    stroke-width: 0.5;
    vector-effect: non-scaling-stroke;
  }
  .diag {
    stroke: var(--border);
    stroke-width: 1;
    stroke-dasharray: 3 3;
    vector-effect: non-scaling-stroke;
  }
  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .pt {
    fill: var(--bg-panel);
    stroke: var(--accent-strong);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
    cursor: grab;
  }
  .pt.on {
    fill: var(--accent);
  }
</style>
