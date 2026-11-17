<script lang="ts">
  // The Splits tab: videos and pictures combined into one output, one after
  // another or at the same time. Plain choices on the left, a live preview on
  // the right; the sidebar's settings (preset, size limit, format, caption...)
  // apply to the result.
  //
  // In the preview: drag a part to move its picture, use the wheel to zoom,
  // drag the line between two parts to change their sizes, double-click a
  // part (or Crop on its row) to choose the part of the picture to use.
  import { untrack } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { app } from "$lib/state.svelte";
  import { splits, frameOf, SHAPES, SIZES, type Shape, type SplitClip } from "$lib/splits.svelte";
  import { pickSplitClips } from "$lib/dialogs";
  import { fmtTime } from "$lib/time";
  import { showMenu, type MenuItem } from "$lib/menu.svelte";
  import * as ipc from "$lib/ipc";
  import type { Rect } from "$lib/types";
  import Icon from "./Icon.svelte";
  import Seg from "./Seg.svelte";
  import Select from "./Select.svelte";
  import TimeField from "./TimeField.svelte";
  import CropOverlay from "./CropOverlay.svelte";
  import CropBar from "./CropBar.svelte";
  import SplitTrack from "./SplitTrack.svelte";

  // plan again shortly after anything that changes the job
  $effect(() => {
    void JSON.stringify(splits.spec());
    const t = setTimeout(() => void splits.refreshPlan(), 300);
    return () => clearTimeout(t);
  });

  // small pictures of the clips (a picture is its own)
  let posters = $state<Record<string, string>>({});
  $effect(() => {
    for (const c of splits.clips) {
      if (posters[c.path] !== undefined) continue;
      if (c.still) {
        posters[c.path] = convertFileSrc(c.path);
        continue;
      }
      posters[c.path] = "";
      ipc.poster(c.path).then((p) => (posters[c.path] = convertFileSrc(p))).catch(() => {});
    }
  });

  async function addFiles() {
    const files = await pickSplitClips();
    if (files.length) await splits.addPaths(files);
  }

  const n = $derived(splits.clips.length);
  const [cw, ch] = $derived(splits.canvas);
  const boxes = $derived(splits.boxes);
  const total = $derived(splits.length_);

  // --- preview ---
  let stageW = $state(0);
  let stageH = $state(0);
  const scale = $derived(cw && ch && stageW && stageH ? Math.min(stageW / cw, stageH / ch) : 0);
  /** the video element of each clip, by clip id */
  let videos = $state<Record<number, HTMLVideoElement | undefined>>({});
  /** one after another: the clip shown */
  const current = $derived(splits.split ? -1 : splits.indexAt(splits.time));
  /** the clip being cropped (its id), or null */
  let cropping = $state<number | null>(null);
  /** the clip picked in the preview or the list (its id) */
  let selected = $state<number | null>(null);

  const muted = (i: number) => {
    if (!app.audioOn) return true;
    if (!splits.split) return false;
    return splits.sound === "none" || (typeof splits.sound === "number" && splits.sound !== i);
  };

  /**
   * Where a clip's picture goes in a space of bw x bh: `wrap` is the part shown
   * (its crop, or all of it) fitted in (cover: filling) the space, `media` the
   * whole picture behind it. The same math as the engine's fit, so the preview
   * matches the encode.
   */
  function place(c: SplitClip, r: Rect, bw: number, bh: number, cover: boolean) {
    const [W, H] = frameOf(c);
    const k = cover ? Math.max(bw / r.w, bh / r.h) : Math.min(bw / r.w, bh / r.h);
    const ww = r.w * k;
    const wh = r.h * k;
    return { k, wrap: { x: (bw - ww) / 2, y: (bh - wh) / 2, w: ww, h: wh }, media: { x: -r.x * k, y: -r.y * k, w: W * k, h: H * k } };
  }

  // Playback: one clock for the whole output (requestAnimationFrame). Each
  // video is kept at the clock's moment of its clip; a picture just shows.
  let raf = 0;
  let wall0 = 0;
  let t0 = 0;

  function sync(hard: boolean) {
    splits.clips.forEach((c, i) => {
      const v = videos[c.id];
      if (!v || c.still || v.readyState < 1) return;
      const { local, active } = splits.at(i, splits.time);
      const run = splits.playing && active;
      if (hard || Math.abs(v.currentTime - local) > (run ? 0.3 : 0.04)) v.currentTime = local;
      if (run && v.paused) v.play().catch(() => {});
      else if (!run && !v.paused) v.pause();
    });
  }

  function frame() {
    if (!splits.playing) return;
    const t = t0 + (performance.now() - wall0) / 1000;
    if (t >= total) {
      splits.time = total;
      stop();
      return;
    }
    splits.time = t;
    sync(false);
    raf = requestAnimationFrame(frame);
  }

  function play() {
    if (splits.playing) return stop();
    if (n < 2 || total <= 0) return;
    if (splits.time >= total - 0.05) splits.time = 0;
    cropping = null;
    splits.playing = true;
    t0 = splits.time;
    wall0 = performance.now();
    sync(true);
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(frame);
  }

  function stop() {
    splits.playing = false;
    cancelAnimationFrame(raf);
    sync(false);
  }

  // A seek (seek bar, a click in a clip's track): every video goes there.
  // untrack: sync reads the clock, which changes every frame; tracking it
  // would seek every video on every frame of playback.
  $effect(() => {
    void splits.seekSeq;
    untrack(() => {
      t0 = splits.time;
      wall0 = performance.now();
      sync(true);
    });
  });
  // a trim or a new order changes what each video shows at this moment
  $effect(() => {
    void JSON.stringify(splits.clips.map((c) => [c.id, c.start, c.end]));
    void splits.mode;
    untrack(() => {
      if (!splits.playing) sync(true);
    });
  });
  // while an end of a range is dragged, that clip shows the end
  $effect(() => {
    const p = splits.peek;
    if (!p) return;
    const v = videos[p.id];
    if (v && v.readyState >= 1) v.currentTime = p.t;
  });
  // leaving the tab stops playback
  $effect(() => () => {
    cancelAnimationFrame(raf);
    splits.playing = false;
  });

  // seek bar
  let bar = $state<HTMLDivElement>();
  let seeking = false;
  function barSeek(e: PointerEvent) {
    const b = bar!.getBoundingClientRect();
    splits.seek(((e.clientX - b.left) / Math.max(1, b.width)) * total);
  }

  // --- drag in the preview: move a picture, or the line between two parts ---
  type Drag =
    | { kind: "pan"; i: number; start: Rect; x0: number; y0: number; k: number; moved: boolean }
    | { kind: "divider"; k: number; start: number[]; x0: number; y0: number; per: number };
  let drag: Drag | null = null;
  let canvasEl = $state<HTMLDivElement>();

  /** CSS pixels on screen per canvas pixel (the UI scale zooms the page) */
  function screenScale() {
    const r = canvasEl?.getBoundingClientRect();
    return r && cw ? r.width / cw : scale;
  }

  function cellDown(e: PointerEvent, i: number) {
    if (e.button !== 0 || cropping !== null) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const c = splits.clips[i];
    selected = c.id;
    const b = boxes[i];
    const shown = splits.shown(i);
    const k = place(c, shown, b.w, b.h, splits.fill === "crop").k * screenScale();
    drag = { kind: "pan", i, start: shown, x0: e.clientX, y0: e.clientY, k, moved: false };
  }

  function dividerDown(e: PointerEvent, k: number) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const w = splits.weights;
    const sum = w.reduce((a, b) => a + b, 0) || 1;
    // screen pixels per unit of weight along the split
    const per = ((splits.mode === "columns" ? cw : ch) * screenScale()) / sum;
    drag = { kind: "divider", k, start: [...w], x0: e.clientX, y0: e.clientY, per };
  }

  function dragMove(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x0;
    const dy = e.clientY - drag.y0;
    if (drag.kind === "pan") {
      if (Math.abs(dx) + Math.abs(dy) > 2) drag.moved = true;
      if (!drag.moved) return;
      splits.pan(drag.i, drag.start, -dx / drag.k, -dy / drag.k);
      return;
    }
    const d = (splits.mode === "columns" ? dx : dy) / drag.per;
    const w = [...drag.start];
    const sum = w.reduce((a, b) => a + b, 0);
    const min = sum * 0.08;
    const a = w[drag.k];
    const b = w[drag.k + 1];
    const na = Math.min(Math.max(min, a + d), a + b - min);
    w[drag.k] = na;
    w[drag.k + 1] = a + b - na;
    splits.custom = w;
    splits.parts = "custom";
  }

  function dragUp(e: PointerEvent) {
    if (!drag) return;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    drag = null;
  }

  // the wheel zooms the picture under the pointer (added by hand: it must call preventDefault)
  function wheelZoom(node: HTMLElement, i: () => number) {
    const on = (e: WheelEvent) => {
      if (cropping !== null) return;
      e.preventDefault();
      const k = i();
      const c = splits.clips[k];
      const b = boxes[k];
      if (!c || !b) return;
      const shown = splits.shown(k);
      const p = place(c, shown, b.w, b.h, splits.fill === "crop");
      const r = node.getBoundingClientRect();
      const s = r.width / Math.max(1, b.w);
      // the source point under the pointer
      const px = shown.x + ((e.clientX - r.left) / s - p.wrap.x) / p.k;
      const py = shown.y + ((e.clientY - r.top) / s - p.wrap.y) / p.k;
      const d = e.deltaMode === 1 ? e.deltaY * 40 : e.deltaY;
      splits.zoomTo(k, splits.zoomOf(k) * Math.pow(1.0015, -d), px, py);
    };
    node.addEventListener("wheel", on, { passive: false });
    return { destroy: () => node.removeEventListener("wheel", on) };
  }

  /** the lines between parts: side by side or stacked, two or more clips */
  const dividers = $derived.by(() => {
    if (!splits.split || splits.mode === "grid" || n < 2 || boxes.length !== n) return [];
    return boxes.slice(0, -1).map((b, k) => {
      const nb = boxes[k + 1];
      return splits.mode === "columns" ? (b.x + b.w + nb.x) / 2 : (b.y + b.h + nb.y) / 2;
    });
  });

  // --- crop mode: the clip whole on the stage, with the crop rectangle ---
  const cropIdx = $derived(cropping === null ? -1 : splits.clips.findIndex((c) => c.id === cropping));
  const cropClip = $derived(cropIdx >= 0 ? splits.clips[cropIdx] : null);
  const cropBox = $derived.by(() => {
    if (!cropClip || !stageW || !stageH) return null;
    const [W, H] = frameOf(cropClip);
    const k = Math.min((stageW - 24) / W, (stageH - 24) / H);
    return { x: (stageW - W * k) / 2, y: (stageH - H * k) / 2, w: W * k, h: H * k };
  });
  let cropVideo = $state<HTMLVideoElement>();
  $effect(() => {
    const v = cropVideo;
    const c = cropClip;
    if (!v || !c) return;
    const at = untrack(() => splits.at(cropIdx, splits.time).local);
    const go = () => (v.currentTime = at);
    if (v.readyState >= 1) go();
    else v.addEventListener("loadedmetadata", go, { once: true });
  });
  // leave crop mode when its clip goes away
  $effect(() => {
    if (cropping !== null && !cropClip) cropping = null;
  });
  $effect(() => {
    if (cropping === null) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !(e.target instanceof HTMLInputElement)) {
        e.preventDefault();
        cropping = null;
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  function startCrop(id: number) {
    stop();
    selected = id;
    cropping = id;
  }

  // --- matching lengths ---
  /** the clips clip i can be made as long as (split screen: a picture's length follows the videos) */
  const others = (i: number) =>
    splits.clips.map((o, k) => ({ o, k })).filter(({ o, k }) => k !== i && !(splits.split && o.still) && splits.lenOf(k) > 0);

  function matchMenu(e: MouseEvent, c: SplitClip, i: number) {
    showMenu(
      e,
      others(i).map(({ o, k }) => ({ label: `As long as clip ${k + 1} (${fmtTime(splits.lenOf(k))})`, action: () => splits.matchLength(c.id, o.id) })),
    );
  }

  function clipMenu(e: MouseEvent, i: number) {
    // a text field in the row keeps its own menu (cut, copy, paste)
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    const c = splits.clips[i];
    if (!c) return;
    selected = c.id;
    const side = splits.mode === "columns" ? ["left", "right"] : splits.mode === "rows" ? ["up", "down"] : ["earlier", "later"];
    const items: MenuItem[] = [
      { label: "Crop...", action: () => startCrop(c.id) },
      { label: "Reset crop and zoom", disabled: !c.crop, action: () => splits.setCrop(c.id, null) },
      "sep",
      ...others(i).map(({ o, k }) => ({ label: `Make as long as clip ${k + 1} (${fmtTime(splits.lenOf(k))})`, disabled: splits.split && c.still, action: () => splits.matchLength(c.id, o.id) })),
      "sep",
      { label: `Move ${side[0]}`, disabled: i === 0, action: () => splits.move(c.id, -1) },
      { label: `Move ${side[1]}`, disabled: i === n - 1, action: () => splits.move(c.id, 1) },
      { label: "Show in folder", action: () => void ipc.openItem(c.path).catch(() => {}) },
      "sep",
      { label: "Remove", danger: true, action: () => splits.remove(c.id) },
    ];
    showMenu(e, items);
  }

  const summary = $derived.by(() => {
    const p = splits.plan;
    if (!p) return null;
    const parts =
      splits.split && p.cells?.length ? `, ${n} parts (${p.cells.map((c) => `${c.w}x${c.h}`).join(", ")} on the ${cw}x${ch} canvas)` : "";
    const snd = !p.audioKbps
      ? "no sound"
      : !splits.split
        ? "each clip's sound"
        : splits.sound === "mix"
          ? "all sounds mixed"
          : `sound from clip ${(splits.sound as number) + 1}`;
    return `${p.outWidth}x${p.outHeight}${parts}, ${splits.fps} fps, ${fmtTime(p.durationOut)}, ${snd}`;
  });

  const shapeOpts = SHAPES.map((s) => ({ value: s.value, label: s.label, title: s.hint }));
  const partOpts = SHAPES.map((s) => ({ value: s.value, label: s.value === "auto" ? "Auto" : s.value, title: s.value === "auto" ? "each part takes its share of the final shape" : s.hint }));
  const soundOpts = $derived([
    ...splits.clips.map((c, i) => ({ value: i as number | "mix" | "none", label: `Clip ${i + 1} (${c.name})${c.info.audio.length ? "" : ", no sound"}` })),
    { value: "mix" as const, label: "All, mixed" },
    { value: "none" as const, label: "None" },
  ]);
  const partsOpts = $derived([
    { value: "fit" as const, label: "Fit each clip", title: "each part as wide (side by side) or tall (stacked) as its clip's shape needs" },
    { value: "equal" as const, label: "Equal" },
    { value: "first" as const, label: "First clip larger" },
    ...(splits.parts === "custom" ? [{ value: "custom" as const, label: "By hand", title: "set by dragging the line between two parts in the preview" }] : []),
  ]);
</script>

<div class="splits">
  <div class="head">
    <span class="title head-text">Splits</span>
    <span class="muted small">{n ? `${n} clips` : "no clips"}{#if splits.busy} · {splits.busy}{/if}</span>
    <span class="grow"></span>
    <button class="btn small" onclick={addFiles}><Icon name="plus" size={11} />Add videos or pictures</button>
    <button class="btn small danger" disabled={!n} onclick={() => splits.clear()}><Icon name="trash" size={11} />Clear</button>
  </div>

  {#if !n}
    <div class="empty">
      <Icon name="stack" size={36} />
      <div class="big">Combine videos and pictures into one</div>
      <div class="muted">
        Add videos or pictures (or drop them here). They can have different sizes, shapes and frame rates. Show them one after another, or at the same
        time side by side, stacked or in a grid. A picture beside a video stays on screen for the whole video.
      </div>
      <button class="btn primary" onclick={addFiles}><Icon name="plus" size={12} />Add videos or pictures</button>
    </div>
  {:else}
    <div class="body">
      <div class="choices">
        <div class="q">How to combine</div>
        <Seg
          label="How to combine"
          bind:value={splits.mode}
          options={[
            { value: "columns", label: "Side by side" },
            { value: "rows", label: "Stacked" },
            { value: "grid", label: "Grid" },
            { value: "sequence", label: "One after another" },
          ]}
        />

        {#if splits.split}
          <div class="q" title="Picking one sets the final shape: two 9:16 parts side by side make a 9:8 picture.">Shape of each part</div>
          <Seg label="Shape of each part" bind:value={splits.partShape} options={partOpts} />
        {/if}
        <div class="q">Final shape</div>
        {#if splits.split && splits.partShape !== "auto"}
          <div class="muted small">Set by the shape of each part: {cw}:{ch} ({(cw / ch).toFixed(2)}).</div>
        {:else}
          <Seg label="Final shape" bind:value={splits.shape} options={shapeOpts as { value: Shape; label: string; title: string }[]} />
        {/if}

        <div class="q">Size</div>
        <Seg
          label="Size"
          bind:value={splits.size}
          options={[
            { value: "auto", label: "Auto", title: "as large as the size limit allows for a clean picture" },
            ...(["small", "medium", "large"] as const).map((s) => ({ value: s, label: SIZES[s].label, title: splits.sizeLabel(s) })),
          ]}
        />
        <div class="muted small">
          {splits.size === "auto" ? `Up to ${splits.sizeLabel("large")}, smaller when the size limit needs it.` : splits.sizeLabel(splits.size)}
        </div>

        <div class="q">Fill each space</div>
        <Seg
          label="Fill each space"
          bind:value={splits.fill}
          options={[
            { value: "bars", label: "Whole picture", title: "the whole picture, with black bars where the shapes differ" },
            { value: "crop", label: "Fill, cut edges", title: "fills the space; the edges that do not fit are cut off" },
            { value: "blur", label: "Blurred background", title: "the whole picture over a blurred, enlarged copy of itself" },
          ]}
        />

        {#if splits.split}
          {#if splits.mode !== "grid" && n > 1}
            <div class="q">Sizes of the parts</div>
            <Seg label="Sizes of the parts" bind:value={splits.parts} options={partsOpts} />
            <div class="muted small">Or drag the line between two parts in the preview.</div>
          {/if}
          <div class="q">Sound from</div>
          <Select label="Sound from" bind:value={splits.sound} options={soundOpts} />
          <div class="q">Length</div>
          <Seg
            label="Length"
            bind:value={splits.length}
            options={[
              { value: "shortest", label: "Shortest clip" },
              { value: "longest", label: "Longest clip", title: "the shorter clips hold their last frame" },
            ]}
          />
          <div class="q">Gap between parts</div>
          <div class="line">
            <Seg
              label="Gap between parts"
              bind:value={splits.gap}
              options={[
                { value: "none", label: "None" },
                { value: "thin", label: "Thin" },
                { value: "thick", label: "Thick" },
              ]}
            />
            {#if splits.gap !== "none"}<input type="color" aria-label="Gap color" bind:value={splits.gapColor} />{/if}
          </div>
        {/if}
      </div>

      <div class="right">
        <div class="stage" bind:clientWidth={stageW} bind:clientHeight={stageH}>
          {#if cropClip && cropBox}
            {@const c = cropClip}
            <div class="cropstage">
              {#if c.still}
                <img class="whole" src={posters[c.path] || undefined} alt="" draggable="false" style:left="{cropBox.x}px" style:top="{cropBox.y}px" style:width="{cropBox.w}px" style:height="{cropBox.h}px" />
              {:else}
                <video
                  class="whole"
                  bind:this={cropVideo}
                  src={convertFileSrc(c.path)}
                  muted
                  preload="auto"
                  playsinline
                  style:left="{cropBox.x}px"
                  style:top="{cropBox.y}px"
                  style:width="{cropBox.w}px"
                  style:height="{cropBox.h}px"
                ></video>
              {/if}
              <CropOverlay
                rect={cropBox}
                W={frameOf(c)[0]}
                H={frameOf(c)[1]}
                crop={c.crop}
                ratio={splits.cropRatio(cropIdx)}
                active
                onchange={(r) => (c.crop = r)}
                oncommit={(r) => splits.setCrop(c.id, r)}
              />
            </div>
          {:else if scale}
            <div
              class="canvas"
              bind:this={canvasEl}
              style:width="{cw * scale}px"
              style:height="{ch * scale}px"
              style:background={splits.split && splits.gap !== "none" ? splits.gapColor : "#000"}
            >
              {#each splits.clips as c, i (c.id)}
                {@const b = boxes[i]}
                {#if b}
                  {@const bw = b.w * scale}
                  {@const bh = b.h * scale}
                  {@const shown = splits.shown(i)}
                  {@const fg = place(c, shown, bw, bh, splits.fill === "crop")}
                  {@const bg = place(c, shown, bw, bh, true)}
                  <div
                    class="cell"
                    class:hidden={!splits.split && i !== current}
                    class:sel={selected === c.id && splits.split}
                    style:left="{b.x * scale}px"
                    style:top="{b.y * scale}px"
                    style:width="{bw}px"
                    style:height="{bh}px"
                    role="presentation"
                    onpointerdown={(e) => cellDown(e, i)}
                    onpointermove={dragMove}
                    onpointerup={dragUp}
                    onpointercancel={dragUp}
                    ondblclick={() => startCrop(c.id)}
                    oncontextmenu={(e) => clipMenu(e, i)}
                    use:wheelZoom={() => i}
                  >
                    {#if splits.fill === "blur"}
                      <div class="wrap blur" style:left="{bg.wrap.x}px" style:top="{bg.wrap.y}px" style:width="{bg.wrap.w}px" style:height="{bg.wrap.h}px">
                        <img src={posters[c.path] || undefined} alt="" draggable="false" style:left="{bg.media.x}px" style:top="{bg.media.y}px" style:width="{bg.media.w}px" style:height="{bg.media.h}px" />
                      </div>
                    {/if}
                    <div class="wrap" style:left="{fg.wrap.x}px" style:top="{fg.wrap.y}px" style:width="{fg.wrap.w}px" style:height="{fg.wrap.h}px">
                      {#if c.still}
                        <img src={posters[c.path] || undefined} alt="" draggable="false" style:left="{fg.media.x}px" style:top="{fg.media.y}px" style:width="{fg.media.w}px" style:height="{fg.media.h}px" />
                      {:else}
                        <video
                          bind:this={videos[c.id]}
                          src={convertFileSrc(c.path)}
                          poster={posters[c.path] || undefined}
                          muted={muted(i)}
                          preload="auto"
                          playsinline
                          style:left="{fg.media.x}px"
                          style:top="{fg.media.y}px"
                          style:width="{fg.media.w}px"
                          style:height="{fg.media.h}px"
                          onloadedmetadata={() => sync(false)}
                        ></video>
                      {/if}
                    </div>
                    {#if splits.split}<span class="tag mono">{i + 1}</span>{/if}
                  </div>
                {/if}
              {/each}
              {#each dividers as at, k (k)}
                <div
                  class="divider"
                  class:rows={splits.mode === "rows"}
                  style:left={splits.mode === "columns" ? `${at * scale}px` : null}
                  style:top={splits.mode === "rows" ? `${at * scale}px` : null}
                  role="separator"
                  aria-label="Size of parts {k + 1} and {k + 2}"
                  onpointerdown={(e) => dividerDown(e, k)}
                  onpointermove={dragMove}
                  onpointerup={dragUp}
                  onpointercancel={dragUp}
                ></div>
              {/each}
            </div>
          {/if}
        </div>

        {#if cropClip}
          {@const c = cropClip}
          <CropBar
            aspect={c.aspect}
            crop={c.crop}
            W={frameOf(c)[0]}
            H={frameOf(c)[1]}
            lead={[
              {
                value: "part",
                label: splits.split ? "Part" : "Frame",
                title: splits.partFollowsClip(cropIdx)
                  ? `the space of clip ${cropIdx + 1} takes the crop's shape, so any shape fits it`
                  : `the shape of the space clip ${cropIdx + 1} fills (${splits.partRatio(cropIdx).toFixed(2)})`,
              },
            ]}
            onaspect={(a) => splits.setCropAspect(c.id, a)}
            oncrop={(r) => splits.setCrop(c.id, r)}
            ondone={() => (cropping = null)}
            hint="Clip {cropIdx + 1}: drag on the picture to choose the part to use."
          >
            <label class="zoom">
              <span>Zoom</span>
              <input
                type="range"
                min="1"
                max="8"
                step="0.05"
                value={splits.zoomOf(cropIdx)}
                oninput={(e) => splits.zoomTo(cropIdx, Number((e.currentTarget as HTMLInputElement).value))}
              />
              <span class="mono">{Math.round(splits.zoomOf(cropIdx) * 100)}%</span>
            </label>
          </CropBar>
        {/if}

        <div class="pbar">
          <button class="btn small" onclick={play} disabled={n < 2 || total <= 0}><Icon name={splits.playing ? "pause" : "play"} size={12} />{splits.playing ? "Pause" : "Play"}</button>
          <span class="mono small time">{fmtTime(splits.time)} / {fmtTime(total)}</span>
          <div
            class="seek"
            bind:this={bar}
            role="slider"
            tabindex="-1"
            aria-label="Position in the result"
            aria-valuemin={0}
            aria-valuemax={total}
            aria-valuenow={splits.time}
            onpointerdown={(e) => {
              if (e.button !== 0 || !total) return;
              seeking = true;
              (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
              barSeek(e);
            }}
            onpointermove={(e) => seeking && barSeek(e)}
            onpointerup={() => (seeking = false)}
            onpointercancel={() => (seeking = false)}
          >
            {#if !splits.split && total > 0}
              {#each splits.clips as c, i (c.id)}
                <span class="block" class:cur={i === current} style:left="{(splits.offsets[i] / total) * 100}%" style:width="{(splits.lenOf(i) / total) * 100}%">{i + 1}</span>
              {/each}
            {/if}
            <span class="phead" style:left="{total ? (splits.time / total) * 100 : 0}%"></span>
          </div>
        </div>
        <div class="pinfo">
          {#if summary}<span class="mono small">{summary}</span>{/if}
          {#if splits.split && !cropClip}<span class="muted small">Drag a part to move its picture, wheel to zoom, double-click to crop.</span>{/if}
        </div>
        {#if splits.planError}<div class="err small">{splits.planError}</div>{/if}
        {#each splits.plan?.warnings ?? [] as w (w)}<div class="warn small"><Icon name="warn" size={11} /> {w}</div>{/each}

        <div class="clips">
          {#each splits.clips as c, i (c.id)}
            {@const oth = others(i)}
            <div class="clip" class:sel={selected === c.id} role="listitem" oncontextmenu={(e) => clipMenu(e, i)} onpointerdown={() => (selected = c.id)}>
              <div class="row">
                <span class="num mono">{i + 1}</span>
                {#if posters[c.path]}<img class="poster" src={posters[c.path]} alt="" draggable="false" />{:else}<span class="poster"></span>{/if}
                <div class="cmain">
                  <div class="name" title={c.path}>{c.name}</div>
                  <div class="meta mono">
                    {#if c.still}
                      picture · {c.info.video.width}x{c.info.video.height}
                    {:else}
                      {c.info.video.width}x{c.info.video.height} · {Math.round(c.info.video.fps * 100) / 100} fps · {fmtTime(c.info.duration)}{c.info.audio.length ? "" : " · no sound"}
                    {/if}
                    {#if c.crop} · crop {c.crop.w}x{c.crop.h}{/if}
                  </div>
                </div>
                <div class="trim">
                  {#if c.still && splits.split}
                    <span class="muted small follows">Shows for the whole result ({fmtTime(splits.lenOf(i))})</span>
                  {:else if c.still}
                    <TimeField label="Shows for" value={c.end - c.start} onset={(v) => splits.setStillLength(c.id, v)} />
                  {:else}
                    <TimeField label="Start of clip {i + 1}" value={c.start} onset={(v) => splits.setTrim(c.id, v, c.end)} />
                    <span class="muted to">to</span>
                    <TimeField label="End of clip {i + 1}" value={c.end} onset={(v) => splits.setTrim(c.id, c.start, v)} />
                  {/if}
                </div>
                {#if oth.length && !(c.still && splits.split)}
                  {#if oth.length === 1}
                    <button
                      class="btn small"
                      title="Make clip {i + 1} as long as clip {oth[0].k + 1} ({fmtTime(splits.lenOf(oth[0].k))}){c.still ? '' : ': the end moves (the start too when the file is too short)'}"
                      onclick={() => splits.matchLength(c.id, oth[0].o.id)}
                    >Match clip {oth[0].k + 1}</button>
                  {:else}
                    <button class="btn small" title="Make clip {i + 1} as long as another clip" onclick={(e) => matchMenu(e, c, i)}>Match...</button>
                  {/if}
                {/if}
                <button class="btn small" class:on={cropping === c.id} title="Choose the part of the picture to use, and zoom" onclick={() => (cropping === c.id ? (cropping = null) : startCrop(c.id))}><Icon name="crop" size={11} />Crop</button>
                <button class="btn small icon" aria-label="Move up" disabled={i === 0} onclick={() => splits.move(c.id, -1)}><Icon name="up" size={11} /></button>
                <button class="btn small icon" aria-label="Move down" disabled={i === n - 1} onclick={() => splits.move(c.id, 1)}><span class="down"><Icon name="up" size={11} /></span></button>
                <button class="btn small icon" aria-label="Remove {c.name}" onclick={() => splits.remove(c.id)}><Icon name="x" size={11} /></button>
              </div>
              {#if !c.still}<SplitTrack {c} {i} />{/if}
            </div>
          {/each}
          {#if n === 1}<div class="muted small">Add at least one more clip.</div>{/if}
        </div>
      </div>
    </div>

    <div class="foot">
      <span class="muted small">The sidebar's settings (preset, size limit, format, caption, watermark) apply to the result.</span>
      <span class="grow"></span>
      <button class="btn" disabled={n < 2 || app.ffmpegMissing || !!splits.planError} onclick={() => splits.enqueue(false)}>Add to queue</button>
      <button class="btn primary" disabled={n < 2 || app.ffmpegMissing || !!splits.planError} onclick={() => splits.enqueue(true)}>Encode</button>
    </div>
  {/if}
</div>

<style>
  .splits {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-app);
  }
  .head,
  .foot {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 10px var(--pad-panel);
    background: var(--bg-panel);
  }
  .head {
    border-bottom: 1px solid var(--border-muted);
  }
  .foot {
    border-top: 1px solid var(--border-muted);
  }
  .title {
    font-size: 13px;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 11.5px;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    color: var(--text-muted);
    text-align: center;
  }
  .empty .muted {
    max-width: 480px;
    font-size: 12px;
  }
  .big {
    font-size: 16px;
    color: var(--text-secondary);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 14px;
    padding: 12px var(--pad-panel);
    overflow: hidden;
  }
  .choices {
    flex: none;
    width: 330px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 4px;
  }
  .choices :global(.seg) {
    flex-wrap: wrap;
  }
  .q {
    margin-top: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .line input[type="color"] {
    width: 32px;
    height: 22px;
    padding: 0;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-sm);
    background: none;
  }
  .right {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 200px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-stage);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .cropstage {
    position: absolute;
    inset: 0;
  }
  .cropstage .whole {
    position: absolute;
    object-fit: fill;
  }
  .canvas {
    position: relative;
    overflow: hidden;
    outline: 1px solid var(--border);
  }
  .cell {
    position: absolute;
    overflow: hidden;
    background: #000;
    cursor: grab;
    touch-action: none;
  }
  .cell:active {
    cursor: grabbing;
  }
  .cell.hidden {
    visibility: hidden;
  }
  .cell.sel {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
    z-index: 1;
  }
  .wrap {
    position: absolute;
    overflow: hidden;
    pointer-events: none;
  }
  .wrap video,
  .wrap img {
    position: absolute;
    max-width: none;
    object-fit: fill;
  }
  .wrap.blur {
    filter: blur(14px) brightness(0.9);
  }
  .tag {
    position: absolute;
    left: 4px;
    top: 4px;
    padding: 0 5px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    font-size: 10px;
    pointer-events: none;
    opacity: 0.8;
  }
  .divider {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 12px;
    margin-left: -6px;
    z-index: 2;
    cursor: ew-resize;
    touch-action: none;
  }
  .divider.rows {
    top: auto;
    left: 0;
    right: 0;
    bottom: auto;
    width: auto;
    height: 12px;
    margin: -6px 0 0;
    cursor: ns-resize;
  }
  .divider::after {
    content: "";
    position: absolute;
    left: 5px;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--accent);
    opacity: 0;
    transition: opacity 0.1s;
  }
  .divider.rows::after {
    left: 0;
    right: 0;
    top: 5px;
    bottom: auto;
    width: auto;
    height: 2px;
  }
  .divider:hover::after,
  .divider:active::after {
    opacity: 0.9;
  }
  .zoom {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .zoom input {
    width: 120px;
  }
  .pbar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .time {
    flex: none;
    color: var(--text-secondary);
  }
  .seek {
    position: relative;
    flex: 1;
    height: 18px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    cursor: pointer;
    touch-action: none;
    overflow: hidden;
  }
  .block {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border-right: 1px solid var(--bg-panel);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--text-muted);
    font-size: 10px;
    pointer-events: none;
  }
  .block.cur {
    background: color-mix(in srgb, var(--accent) 34%, transparent);
    color: var(--text-primary);
  }
  .phead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    margin-left: -1px;
    background: var(--accent);
    pointer-events: none;
  }
  .pinfo {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    justify-content: space-between;
  }
  .err {
    color: var(--red);
  }
  .warn {
    color: var(--yellow);
  }
  .clips {
    flex: none;
    max-height: 42%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .clip {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 8px;
    border: var(--bw) solid var(--border-muted);
    border-radius: var(--radius);
    background: var(--bg-panel);
  }
  .clip.sel {
    border-color: var(--accent-line);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .num {
    width: 16px;
    color: var(--text-muted);
    font-size: 11px;
    text-align: center;
  }
  .poster {
    width: 64px;
    height: 36px;
    flex: none;
    object-fit: cover;
    border-radius: var(--radius-sm, 3px);
    background: var(--bg-input);
  }
  .cmain {
    flex: 1;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
  }
  .meta {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .trim {
    display: flex;
    align-items: flex-end;
    gap: 5px;
  }
  .trim .to {
    padding-bottom: 4px;
  }
  .trim :global(input) {
    width: 92px;
  }
  .follows {
    max-width: 200px;
  }
  .down {
    display: inline-flex;
    transform: rotate(180deg);
  }
</style>
