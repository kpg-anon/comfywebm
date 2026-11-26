<script lang="ts">
  // Crop rectangle drawn over the displayed picture. `rect` is the picture's
  // box inside the stage in CSS pixels (letterboxing removed); the crop itself
  // is stored in source display pixels, so every pointer delta is scaled by
  // source width / rect width. Used by the editor and by Splits, so the
  // values come in as props.
  import { draw, move, resize, type Handle } from "$lib/crop";
  import type { Crop } from "$lib/types";

  interface Props {
    rect: { x: number; y: number; w: number; h: number };
    /** the picture's size in source display pixels */
    W: number;
    H: number;
    crop: Crop | null;
    /** width / height the rectangle keeps, or null for free */
    ratio: number | null;
    /** drawing and dragging are on */
    active: boolean;
    /** while dragging */
    onchange: (c: Crop | null) => void;
    /** when the pointer is released */
    oncommit: (c: Crop | null) => void;
    /** the largest the rectangle may be (Follow: the view at its zoom), in the same pixels as W and H */
    max?: { w: number; h: number } | null;
  }
  let { rect, W, H, crop, ratio, active, onchange, oncommit, max = null }: Props = $props();

  /** Shrinks c to `max` (with an aspect lock keeping its shape, else each side on its own),
   *  holding the side or corner opposite the one dragged. */
  function cap(c: Crop, from: { x: number; y: number }): Crop {
    if (!max || (c.w <= max.w + 0.5 && c.h <= max.h + 0.5)) return c;
    const k = Math.min(max.w / c.w, max.h / c.h);
    const [w, h] = ratio ? [c.w * k, c.h * k] : [Math.min(c.w, max.w), Math.min(c.h, max.h)];
    // `from` is the fixed point: the far edge of a resize, the start of a new box
    const x = Math.abs(from.x - (c.x + c.w)) <= 1 ? c.x + c.w - w : Math.abs(from.x - c.x) <= 1 ? c.x : c.x + (c.w - w) / 2;
    const y = Math.abs(from.y - (c.y + c.h)) <= 1 ? c.y + c.h - h : Math.abs(from.y - c.y) <= 1 ? c.y : c.y + (c.h - h) / 2;
    return { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) };
  }
  /** the point a resize keeps still: the start rectangle's side or corner opposite the handle */
  function anchorOf(c: Crop, h: Handle) {
    return {
      x: h.includes("w") ? c.x + c.w : h.includes("e") ? c.x : NaN,
      y: h.includes("n") ? c.y + c.h : h.includes("s") ? c.y : NaN,
    };
  }

  const sx = $derived(rect.w > 0 ? W / rect.w : 1);
  const sy = $derived(rect.h > 0 ? H / rect.h : 1);

  const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

  type Drag =
    | { kind: "new"; px: number; py: number; x0: number; y0: number }
    | { kind: "move"; start: Crop; x0: number; y0: number }
    | { kind: "resize"; start: Crop; handle: Handle; x0: number; y0: number };
  let drag: Drag | null = null;
  /** the rectangle being dragged, the last one sent to onchange */
  let live: Crop | null = null;

  /** Pointer position in source pixels, clamped to the picture. */
  function toSrc(e: PointerEvent, el: HTMLElement) {
    const b = el.getBoundingClientRect();
    const x = Math.min(Math.max(0, (e.clientX - b.left) * sx), W);
    const y = Math.min(Math.max(0, (e.clientY - b.top) * sy), H);
    return { x, y };
  }

  let box: HTMLDivElement;

  function down(e: PointerEvent, kind: "new" | "move" | Handle) {
    if (!active || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    box.setPointerCapture(e.pointerId);
    const p = toSrc(e, box);
    live = crop;
    if (kind === "new") drag = { kind: "new", px: p.x, py: p.y, x0: e.clientX, y0: e.clientY };
    else if (kind === "move" && crop) drag = { kind: "move", start: { ...crop }, x0: e.clientX, y0: e.clientY };
    else if (crop) drag = { kind: "resize", start: { ...crop }, handle: kind as Handle, x0: e.clientX, y0: e.clientY };
  }

  function moveEv(e: PointerEvent) {
    if (!drag) return;
    const dx = (e.clientX - drag.x0) * sx;
    const dy = (e.clientY - drag.y0) * sy;
    let c: Crop;
    if (drag.kind === "new") c = cap(draw(drag.px, drag.py, dx, dy, ratio, W, H), { x: drag.px, y: drag.py });
    else if (drag.kind === "move") c = move(drag.start, dx, dy, W, H);
    else c = cap(resize(drag.start, drag.handle, dx, dy, ratio, W, H), anchorOf(drag.start, drag.handle));
    live = c;
    onchange(c);
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    box.releasePointerCapture(e.pointerId);
    const c = live;
    // a click without a real drag does not make a 1-pixel crop
    if (drag.kind === "new" && c && (c.w < 8 || c.h < 8)) oncommit(null);
    else oncommit(c);
    drag = null;
  }

  // crop rectangle in overlay CSS pixels
  const view = $derived(crop ? { x: crop.x / sx, y: crop.y / sy, w: crop.w / sx, h: crop.h / sy } : null);
</script>

<div
  class="overlay"
  class:active
  bind:this={box}
  style:left="{rect.x}px"
  style:top="{rect.y}px"
  style:width="{rect.w}px"
  style:height="{rect.h}px"
  role="presentation"
  onpointerdown={(e) => down(e, "new")}
  onpointermove={moveEv}
  onpointerup={up}
  onpointercancel={up}
>
  {#if view}
    <!-- the dim layer is clipped to the picture; the handles are not -->
    <div class="dimclip">
      <div class="dim" style:left="{view.x}px" style:top="{view.y}px" style:width="{view.w}px" style:height="{view.h}px"></div>
    </div>
    <div class="crop" style:left="{view.x}px" style:top="{view.y}px" style:width="{view.w}px" style:height="{view.h}px">
      {#if active}
        <div class="body" role="presentation" onpointerdown={(e) => down(e, "move")}></div>
        <div class="grid"></div>
        {#each HANDLES as h (h)}
          <div class="h {h}" role="presentation" onpointerdown={(e) => down(e, h)}></div>
        {/each}
        <span class="size mono">{crop?.w}x{crop?.h}</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .overlay {
    position: absolute;
    pointer-events: none;
    touch-action: none;
  }
  .dimclip {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
  .dim {
    position: absolute;
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.55);
  }
  .overlay.active {
    pointer-events: auto;
    cursor: crosshair;
    outline: 1px dashed color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .crop {
    position: absolute;
    outline: 1px solid var(--accent);
  }
  .body {
    position: absolute;
    inset: 0;
    cursor: move;
  }
  .grid {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background:
      linear-gradient(to right, transparent calc(33.33% - 0.5px), rgba(255, 255, 255, 0.18) calc(33.33% - 0.5px), rgba(255, 255, 255, 0.18) calc(33.33% + 0.5px), transparent calc(33.33% + 0.5px), transparent calc(66.66% - 0.5px), rgba(255, 255, 255, 0.18) calc(66.66% - 0.5px), rgba(255, 255, 255, 0.18) calc(66.66% + 0.5px), transparent calc(66.66% + 0.5px)),
      linear-gradient(to bottom, transparent calc(33.33% - 0.5px), rgba(255, 255, 255, 0.18) calc(33.33% - 0.5px), rgba(255, 255, 255, 0.18) calc(33.33% + 0.5px), transparent calc(33.33% + 0.5px), transparent calc(66.66% - 0.5px), rgba(255, 255, 255, 0.18) calc(66.66% - 0.5px), rgba(255, 255, 255, 0.18) calc(66.66% + 0.5px), transparent calc(66.66% + 0.5px));
  }
  .h {
    position: absolute;
    width: 11px;
    height: 11px;
    margin: -6px 0 0 -6px;
    background: var(--accent);
    border: 1px solid var(--text-inverse);
    border-radius: 2px;
  }
  .nw { left: 0; top: 0; cursor: nwse-resize; }
  .n { left: 50%; top: 0; cursor: ns-resize; }
  .ne { left: 100%; top: 0; cursor: nesw-resize; }
  .e { left: 100%; top: 50%; cursor: ew-resize; }
  .se { left: 100%; top: 100%; cursor: nwse-resize; }
  .s { left: 50%; top: 100%; cursor: ns-resize; }
  .sw { left: 0; top: 100%; cursor: nesw-resize; }
  .w { left: 0; top: 50%; cursor: ew-resize; }
  .size {
    position: absolute;
    left: 4px;
    top: 4px;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.65);
    color: var(--text-primary);
    font-size: 11px;
    pointer-events: none;
  }
</style>
