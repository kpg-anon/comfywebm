<script lang="ts">
  // Crop rectangle drawn over the displayed picture. `rect` is the picture's
  // box inside the stage in CSS pixels (letterboxing removed); the crop itself
  // is stored in source display pixels, so every pointer delta is scaled by
  // source width / rect width.
  import { app } from "$lib/state.svelte";
  import { draw, move, ratioOf, resize, type Handle } from "$lib/crop";
  import type { Crop } from "$lib/types";

  let { rect }: { rect: { x: number; y: number; w: number; h: number } } = $props();

  const W = $derived(app.srcW);
  const H = $derived(app.srcH);
  const sx = $derived(rect.w > 0 ? W / rect.w : 1);
  const sy = $derived(rect.h > 0 ? H / rect.h : 1);

  const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

  type Drag =
    | { kind: "new"; px: number; py: number; x0: number; y0: number }
    | { kind: "move"; start: Crop; x0: number; y0: number }
    | { kind: "resize"; start: Crop; handle: Handle; x0: number; y0: number };
  let drag: Drag | null = null;

  /** Pointer position in source pixels, clamped to the picture. */
  function toSrc(e: PointerEvent, el: HTMLElement) {
    const b = el.getBoundingClientRect();
    const x = Math.min(Math.max(0, (e.clientX - b.left) * sx), W);
    const y = Math.min(Math.max(0, (e.clientY - b.top) * sy), H);
    return { x, y };
  }

  let box: HTMLDivElement;

  function down(e: PointerEvent, kind: "new" | "move" | Handle) {
    if (!app.cropMode || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    box.setPointerCapture(e.pointerId);
    const p = toSrc(e, box);
    if (kind === "new") drag = { kind: "new", px: p.x, py: p.y, x0: e.clientX, y0: e.clientY };
    else if (kind === "move" && app.crop) drag = { kind: "move", start: { ...app.crop }, x0: e.clientX, y0: e.clientY };
    else if (app.crop) drag = { kind: "resize", start: { ...app.crop }, handle: kind as Handle, x0: e.clientX, y0: e.clientY };
  }

  function moveEv(e: PointerEvent) {
    if (!drag) return;
    const dx = (e.clientX - drag.x0) * sx;
    const dy = (e.clientY - drag.y0) * sy;
    const r = ratioOf(app.aspect);
    let c: Crop;
    if (drag.kind === "new") c = draw(drag.px, drag.py, dx, dy, r, W, H);
    else if (drag.kind === "move") c = move(drag.start, dx, dy, W, H);
    else c = resize(drag.start, drag.handle, dx, dy, r, W, H);
    // write directly while dragging; setCrop on release drops a full-frame crop
    app.crop = c;
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    box.releasePointerCapture(e.pointerId);
    const c = app.crop;
    // a click without a real drag does not make a 1-pixel crop
    if (drag.kind === "new" && c && (c.w < 8 || c.h < 8)) app.crop = null;
    else app.setCrop(c);
    drag = null;
  }

  // crop rectangle in overlay CSS pixels
  const view = $derived(
    app.crop ? { x: app.crop.x / sx, y: app.crop.y / sy, w: app.crop.w / sx, h: app.crop.h / sy } : null,
  );
</script>

<div
  class="overlay"
  class:active={app.cropMode}
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
      {#if app.cropMode}
        <div class="body" role="presentation" onpointerdown={(e) => down(e, "move")}></div>
        <div class="grid"></div>
        {#each HANDLES as h (h)}
          <div class="h {h}" role="presentation" onpointerdown={(e) => down(e, h)}></div>
        {/each}
        <span class="size mono">{app.crop?.w}x{app.crop?.h}</span>
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
