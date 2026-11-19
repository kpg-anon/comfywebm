<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import * as ipc from "$lib/ipc";
  import { app, isNeutral } from "$lib/state.svelte";
  import { sample } from "$lib/curve";
  import CropOverlay from "./CropOverlay.svelte";
  import { ratioOf } from "$lib/crop";
  import * as ramp from "$lib/ramp";
  import * as zoomMath from "$lib/zoom";
  import type { SpeedPoint } from "$lib/types";
  import Icon from "./Icon.svelte";

  let video = $state<HTMLVideoElement>();
  let sw = $state(0);
  let sh = $state(0);
  // the plan command adds the logo file to the asset scope; a logo set while an older plan is
  // shown can load before that and fail, so a failed load is tried again (a few times)
  let logoTry = $state(0);
  $effect(() => {
    void app.logoPath;
    logoTry = 0;
  });
  // height / width of the watermark image, for placing and dragging it
  let logoAspect = $state(1);

  type Rect = { x: number; y: number; w: number; h: number };
  /** The watermark's top-left inside `r` (the output picture on screen). */
  function logoPos(r: Rect, lw: number, lh: number) {
    const m = Math.min(r.w, r.h) * 0.03;
    const c = app.logoCorner;
    if (c === "custom") return { x: (r.w - lw) * app.logoX, y: (r.h - lh) * app.logoY };
    return { x: c.endsWith("l") ? m : r.w - lw - m, y: c.startsWith("t") ? m : r.h - lh - m };
  }

  /**
   * Drags a box inside an area: `start` is its top-left in the area, `room` the
   * space left beside and below it; `apply` gets the position as 0..1 of that room.
   */
  function dragIn(e: PointerEvent, start: { x: number; y: number }, room: { w: number; h: number }, apply: (fx: number, fy: number) => void) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const frac = (v: number, r: number) => (r > 0 ? Math.min(1, Math.max(0, v / r)) : 0.5);
    const set = (x: number, y: number) => apply(frac(x, room.w), frac(y, room.h));
    set(start.x, start.y);
    const [px, py] = [e.clientX, e.clientY];
    const move = (ev: PointerEvent) => set(start.x + ev.clientX - px, start.y + ev.clientY - py);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  // Dragging the watermark places it anywhere (switches to Custom). bw, bh: its
  // bounding box on screen (larger than the image when it is turned).
  function dragLogo(e: PointerEvent, r: Rect, bw: number, bh: number) {
    dragIn(e, logoPos(r, bw, bh), { w: r.w - bw, h: r.h - bh }, (fx, fy) => {
      app.logoX = fx;
      app.logoY = fy;
    });
    app.logoCorner = "custom";
  }

  // Dragging the caption places it anywhere (switches to Custom).
  function dragCaption(e: PointerEvent) {
    const s = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const d = (e.currentTarget as HTMLElement).parentElement!.getBoundingClientRect();
    dragIn(e, { x: s.left - d.left, y: s.top - d.top }, { w: d.width - s.width, h: d.height - s.height }, (fx, fy) => {
      app.captionX = fx;
      app.captionY = fy;
    });
    app.captionCustom = true;
  }
  let capW = $state(0);
  let capH = $state(0);

  /** Bounding box of a w x h box turned by `deg`. */
  function turnedBox(w: number, h: number, deg: number) {
    const a = (deg * Math.PI) / 180;
    const [c, s] = [Math.abs(Math.cos(a)), Math.abs(Math.sin(a))];
    return { w: w * c + h * s, h: w * s + h * c };
  }

  /** The tile grid in output pixels, as the engine lays it out (plan.rs overlay_chain). */
  function tileGrid(outW: number, outH: number) {
    const w = Math.max(2, Math.round((outW * Math.min(100, Math.max(2, app.logoSize))) / 100 / 2) * 2);
    const cell = Math.max(w, 2, Math.round(w * (1 + Math.min(3, Math.max(0, app.logoSpacing)))));
    // a turned grid covers the picture's diagonal
    const d = Math.ceil(Math.hypot(outW, outH));
    const [cw, ch] = app.logoAngle ? [d, d] : [outW, outH];
    const cols = Math.ceil(cw / cell) + 1;
    const rows = Math.ceil(ch / cell) + 1;
    return { w, cell, cols, rows, x: Math.trunc((outW - cols * cell) / 2), y: Math.trunc((outH - rows * cell) / 2) };
  }

  // Caption font for the preview: an installed font by its family name; a font
  // file that is not installed is loaded through the asset protocol once the
  // plan command has granted it (tried again a few times, as the logo is).
  const captionFace = $derived(app.captionFont ? (app.fonts.find((f) => f.path === app.captionFont) ?? null) : null);
  let fileFontReady = $state(false);
  $effect(() => {
    const path = app.captionFont;
    fileFontReady = false;
    if (!path || captionFace || !app.fonts.length) return;
    let stop = false;
    let face: FontFace | null = null;
    (async () => {
      for (let n = 0; n < 6 && !stop; n++) {
        try {
          face = new FontFace("cwm-caption-file", `url("${convertFileSrc(path)}")`);
          await face.load();
          if (stop) return;
          document.fonts.add(face);
          fileFontReady = true;
          return;
        } catch {
          await new Promise((r) => setTimeout(r, 400));
        }
      }
    })();
    return () => {
      stop = true;
      if (face) document.fonts.delete(face);
    };
  });
  const captionCss = $derived(
    captionFace
      ? { family: `"${captionFace.family.replace(/"/g, "")}", Arial, sans-serif`, weight: String(captionFace.weight), style: captionFace.italic ? "italic" : "normal" }
      : app.captionFont && fileFontReady
        ? { family: `"cwm-caption-file", Arial, sans-serif`, weight: "normal", style: "normal" }
        : null,
  );

  $effect(() => {
    app.video = video ?? null;
    return () => {
      if (app.video === video) app.video = null;
    };
  });

  // The picture box inside the stage. object-fit would letterbox; the box is
  // computed here instead so the crop overlay and the rotate/mirror preview
  // share it. The source display size gives the aspect, so the proxy (smaller
  // pixel size, same aspect) maps to the same box. The picture is shown turned
  // and flipped as the output will be. Outside crop mode the stage shows what
  // the output shows (the crop with the zoom, or the follow window at the
  // playhead), as large as it fits; the rest of the picture is off the stage.
  const turned = $derived(app.rotate === 90 || app.rotate === 270);
  /** the picture's size as shown, in its own pixels */
  const vsize = $derived.by(() => {
    const m = app.media;
    // a sound file: the shape of its picture
    const still = app.stillSize;
    const aw = (m?.audioOnly ? still?.[0] : m?.video.width) || 16;
    const ah = (m?.audioOnly ? still?.[1] : m?.video.height) || 9;
    return turned ? { w: ah, h: aw } : { w: aw, h: ah };
  });
  // While playing with follow keyframes, the window is placed for the frame on
  // screen: requestVideoFrameCallback gives that frame's time. currentTime,
  // read on each animation frame, runs ahead of or behind the shown frame, and
  // the window placed for another frame made the picture shake.
  let frameT = $state<number | null>(null);
  $effect(() => {
    const v = video as (HTMLVideoElement & { requestVideoFrameCallback?: (cb: (now: number, m: { mediaTime: number }) => void) => number; cancelVideoFrameCallback?: (id: number) => void }) | undefined;
    if (!v || app.paused || !app.zoomPoints.length || !v.requestVideoFrameCallback) {
      frameT = null;
      return;
    }
    let id = 0;
    const each = (_: number, m: { mediaTime: number }) => {
      frameT = m.mediaTime;
      id = v.requestVideoFrameCallback!(each);
    };
    id = v.requestVideoFrameCallback(each);
    return () => {
      v.cancelVideoFrameCallback?.(id);
      frameT = null;
    };
  });
  /** the part of the picture (as shown) the stage is fitted to */
  const focus = $derived.by(() => {
    const whole = { x: 0, y: 0, w: vsize.w, h: vsize.h };
    if (app.cropMode || app.followMode || app.audioOnly || !app.viewW) return whole;
    const o = app.outViewAt(frameT ?? app.current);
    // outViewAt is in viewW pixels, the same as vsize for a video
    const k = vsize.w / app.viewW;
    return { x: o.x * k, y: o.y * k, w: Math.max(1, o.w * k), h: Math.max(1, o.h * k) };
  });
  /** the picture's box as shown on screen (may reach past the stage) */
  const shown = $derived.by(() => {
    if (sw <= 0 || sh <= 0) return { x: 0, y: 0, w: 0, h: 0 };
    const k = Math.min(sw / focus.w, sh / focus.h);
    return { x: sw / 2 - (focus.x + focus.w / 2) * k, y: sh / 2 - (focus.y + focus.h / 2) * k, w: vsize.w * k, h: vsize.h * k };
  });
  /** the element's box: it keeps the unrotated shape; the transform turns it */
  const box = $derived(turned ? { x: shown.x + (shown.w - shown.h) / 2, y: shown.y + (shown.h - shown.w) / 2, w: shown.h, h: shown.w } : shown);
  /** the output's area on screen */
  const outRect = $derived.by(() => {
    const k = vsize.w > 0 ? shown.w / vsize.w : 0;
    return { x: shown.x + focus.x * k, y: shown.y + focus.y * k, w: focus.w * k, h: focus.h * k };
  });
  // the flips apply after the rotation, as in the encode (rotate, then hflip, then vflip)
  const transform = $derived(`${app.vflip ? "scaleY(-1) " : ""}${app.mirror ? "scaleX(-1) " : ""}rotate(${app.rotate}deg)`);

  // Color adjustments on the picture: an SVG filter close to ffmpeg's eq,
  // hue and curves (not exact: eq works on YUV, this on RGB). Sample shows
  // the real result.
  // the preview's mute and volume (the speaker in the corner, M)
  $effect(() => {
    if (!video) return;
    video.muted = app.muted;
    video.volume = app.volume;
  });
  const hasSound = $derived(!!app.media?.audio.length);

  // With follow keyframes, dragging the picture moves the view at the playhead:
  // the keyframe on this frame, or a new one made here once the drag starts
  // (the picture moves with the pointer, the window the other way); a click
  // without a drag plays and pauses as before.
  let pan: { x0: number; y0: number; i: number; cx: number; cy: number; moved: boolean } | null = null;
  let panned = false;
  function panDown(e: PointerEvent) {
    if (e.button !== 0 || !app.zoomPoints.length || app.cropMode || app.followMode) return;
    const i = app.followKeyAt(app.current);
    const p = i >= 0 ? app.zoomPoints[i] : zoomMath.at(app.zoomPoints, app.current);
    pan = { x0: e.clientX, y0: e.clientY, i, cx: p.x, cy: p.y, moved: false };
  }
  function panMove(e: PointerEvent) {
    if (!pan) return;
    const dx = e.clientX - pan.x0;
    const dy = e.clientY - pan.y0;
    if (!pan.moved) {
      if (Math.abs(dx) + Math.abs(dy) < 4) return;
      pan.moved = true;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      app.video?.pause();
      if (pan.i < 0) pan.i = app.addFollowKey(app.current);
    }
    // screen pixels per frame width
    const f = app.zoomFrame;
    const k = (shown.w / app.viewW) * f.w;
    const kh = (shown.h / app.viewH) * f.h;
    app.setZoomPoint(pan.i, { x: pan.cx - dx / k, y: pan.cy - dy / kh });
  }
  function panUp() {
    if (pan?.moved) panned = true;
    pan = null;
  }
  function soundWheel(e: WheelEvent) {
    // the wheel over the speaker sets the volume; elsewhere it steps frames
    e.preventDefault();
    e.stopPropagation();
    const d = e.deltaMode === 1 ? e.deltaY * 40 : e.deltaY;
    app.setVolume((app.muted ? 0 : app.volume) - Math.sign(d) * 0.05);
  }

  // the speed setting plays in the preview too (pitch kept, as atempo does)
  // (defaultPlaybackRate too: loading a new source resets the rate to it)
  $effect(() => {
    if (app.speedRamp.length) return;
    const r = Math.min(16, Math.max(0.0625, Number(app.speed) || 1));
    if (video) {
      video.defaultPlaybackRate = r;
      video.playbackRate = r;
    }
  });
  // A speed ramp: the rate follows the ramp at the playhead, each frame. The
  // loop reads the video's own time, not app.current, so it does not re-run
  // the effect on every frame.
  $effect(() => {
    const v = video;
    const pts = ramp.clean($state.snapshot(app.speedRamp) as SpeedPoint[]);
    if (!v || !pts.length) return;
    let raf = 0;
    const step = () => {
      const r = ramp.speedAt(pts, v.currentTime);
      if (Math.abs(v.playbackRate - r) > 0.005) v.playbackRate = r;
      v.defaultPlaybackRate = r;
      raf = requestAnimationFrame(step);
    };
    step();
    return () => cancelAnimationFrame(raf);
  });
  // Fade preview: how dark the picture is at the playhead, from the fades at
  // the start of the first output range and the end of the last (in source
  // seconds, so the speed counts).
  const fadeDark = $derived.by(() => {
    const fi = Number(app.fadeIn) || 0;
    const fo = Number(app.fadeOut) || 0;
    if ((!fi && !fo) || app.cropMode || app.playback !== "forward") return 0;
    const segs = app.outSegments;
    // with a ramp, the speed where each fade is (near enough for a preview)
    const pts = app.speedRamp.length ? ramp.clean(app.speedRamp) : null;
    const start = segs.length ? segs[0].start : 0;
    const end = segs.length ? segs[segs.length - 1].end : app.duration;
    const t = app.current;
    let d = 0;
    const si = pts ? ramp.speedAt(pts, start) : Number(app.speed) || 1;
    const so = pts ? ramp.speedAt(pts, end) : Number(app.speed) || 1;
    if (fi > 0 && t >= start && t < start + fi * si) d = Math.max(d, 1 - (t - start) / (fi * si));
    if (fo > 0 && t <= end && t > end - fo * so) d = Math.max(d, 1 - (end - t) / (fo * so));
    return Math.min(1, Math.max(0, d));
  });
  const adj = $derived(app.adjust);
  const showAdj = $derived(app.adjustPreview && !isNeutral(adj));
  const table = $derived(adj.curve.length ? sample(adj.curve, 33).map((v) => v.toFixed(4)).join(" ") : "");

  // follow the playhead every frame while playing (timeupdate fires about 4x a second)
  $effect(() => {
    if (app.paused) return;
    let raf = 0;
    const loop = () => {
      app.tick();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  // Wheel: one frame per notch. Trackpads send many small deltas, so they are summed.
  let wheelAcc = 0;
  function onWheel(e: WheelEvent) {
    if (!app.media) return;
    e.preventDefault();
    const d = e.deltaMode === 1 ? e.deltaY * 40 : e.deltaMode === 2 ? e.deltaY * 400 : e.deltaY;
    wheelAcc += d;
    while (Math.abs(wheelAcc) >= 50) {
      const dir = Math.sign(wheelAcc);
      app.stepFrame(dir);
      wheelAcc -= dir * 100;
      if (Math.sign(wheelAcc) !== dir) wheelAcc = 0;
    }
  }

  function onMeta() {
    const v = video;
    if (!v) return;
    // no picture: the webview cannot decode the video (a sound file has none)
    if (v.videoWidth === 0 && !app.audioOnly) {
      void app.useProxy();
      return;
    }
    // a new source: a seek on the old one ends without a seeked event
    app.queuedSeek = null;
    app.seekPending = false;
    // keep the playhead when switching to the proxy
    if (app.current > 0) app.seek(app.current);
  }
</script>

<div class="stage" class:panning={app.zoomPoints.length > 0 && !app.cropMode} bind:clientWidth={sw} bind:clientHeight={sh} onwheel={onWheel} onpointerdown={panDown} onpointermove={panMove} onpointerup={panUp} onpointercancel={panUp} role="presentation">
  {#if app.videoSrc}
    <div class="pic" style:left="{box.x}px" style:top="{box.y}px" style:width="{box.w}px" style:height="{box.h}px" style:transform={transform}>
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
      style:filter={showAdj ? "url(#cwm-adjust)" : null}
      bind:this={video}
      src={app.videoSrc}
      preload="auto"
      playsinline
      tabindex="-1"
      onloadedmetadata={onMeta}
      onerror={() => void app.useProxy()}
      onplay={() => (app.paused = false)}
      onpause={() => {
        app.paused = true;
        if (video) app.current = video.currentTime;
      }}
      onseeked={() => {
        if (!app.seekDone()) return;
        // the playhead keeps its time when the player went to the middle of its frame;
        // it takes the player's time when the player ended elsewhere (past the end)
        if (video && video.paused && Math.abs(video.currentTime - app.videoTimeOf(app.current)) > 1e-3) app.current = video.currentTime;
      }}
      onended={() => app.onEnded()}
      onclick={() => {
        if (panned) {
          panned = false;
          return;
        }
        if (!app.cropMode) app.togglePlay();
      }}
      ></video>
      {#if app.audioOnly && app.stillUrl}
        <img class="still" src={app.stillUrl} alt="" draggable="false" style:filter={showAdj ? "url(#cwm-adjust)" : null} />
      {/if}
    </div>
    {#if !app.cropMode && !app.followMode && !app.audioOnly}
      <!-- outside the output's area: the stage's own colour, as the encode has nothing there -->
      <div class="outmask" style:left="{outRect.x}px" style:top="{outRect.y}px" style:width="{outRect.w}px" style:height="{outRect.h}px"></div>
    {/if}
    {#if app.followMode}
      <!-- draw the box to follow; letting go starts following it -->
      <CropOverlay
        rect={shown}
        W={app.viewW}
        H={app.viewH}
        crop={app.followBox}
        ratio={null}
        active
        onchange={(c) => (app.followBox = c)}
        oncommit={(c) => (c ? void app.follow(c) : (app.followBox = null))}
      />
      <div class="followhint">Draw a box around the person to follow (Esc cancels)</div>
    {/if}
    <!-- over the picture as shown (turned and flipped), so the crop is drawn on what you see;
         outside crop mode the stage shows the crop itself -->
    {#if app.cropMode && !app.followMode}
    <CropOverlay
      rect={shown}
      W={app.viewW}
      H={app.viewH}
      crop={app.viewCrop}
      ratio={ratioOf(app.aspect)}
      active
      onchange={(c) => app.setViewCrop(c, false)}
      oncommit={(c) => app.setViewCrop(c)}
    />
    {/if}
    {#if fadeDark > 0}
      <div class="fade" style:left="{outRect.x}px" style:top="{outRect.y}px" style:width="{outRect.w}px" style:height="{outRect.h}px" style:opacity={fadeDark}></div>
    {/if}
    {#if app.logoPath && app.plan && !app.cropMode && app.container !== "audio"}
      {@const r = outRect}
      {@const src = convertFileSrc(app.logoPath)}
      {@const retry = () => logoTry < 5 && setTimeout(() => logoTry++, 400)}
      {#key logoTry}
        {#if app.logoCorner === "tile" && app.plan.outWidth > 0}
          {@const t = tileGrid(app.plan.outWidth, app.plan.outHeight)}
          {@const k = r.w / app.plan.outWidth}
          <div class="tiles" style:left="{r.x}px" style:top="{r.y}px" style:width="{r.w}px" style:height="{r.h}px" style:opacity={app.logoOpacity}>
            <div
              class="grid"
              style:left="{t.x * k}px"
              style:top="{t.y * k}px"
              style:width="{t.cols * t.cell * k}px"
              style:height="{t.rows * t.cell * k}px"
              style:transform={app.logoAngle ? `rotate(${app.logoAngle}deg)` : null}
            >
              {#each { length: t.rows } as _, j (j)}
                {#each { length: t.cols } as _, i (i)}
                  <span class="cell" style:left="{i * t.cell * k}px" style:top="{j * t.cell * k}px" style:width="{t.cell * k}px" style:height="{t.cell * k}px">
                    <img {src} alt="" draggable="false" onerror={i + j === 0 ? retry : null} style:max-width="{t.w * k}px" style:max-height="{t.w * k}px" />
                  </span>
                {/each}
              {/each}
            </div>
          </div>
        {:else}
          {@const lw = (r.w * app.logoSize) / 100}
          {@const lh = lw * logoAspect}
          {@const b = turnedBox(lw, lh, app.logoAngle)}
          {@const p = logoPos(r, b.w, b.h)}
          <!-- the bounding box of the (turned) image is what gets placed, as in the engine -->
          <div
            class="logo"
            class:custom={app.logoCorner === "custom"}
            title="Drag to move the watermark"
            role="presentation"
            onpointerdown={(e) => dragLogo(e, r, b.w, b.h)}
            style:width="{b.w}px"
            style:height="{b.h}px"
            style:left="{r.x + p.x}px"
            style:top="{r.y + p.y}px"
          >
            <img
              onerror={retry}
              onload={(e) => {
                const im = e.currentTarget as HTMLImageElement;
                if (im.naturalWidth) logoAspect = im.naturalHeight / im.naturalWidth;
              }}
              {src}
              alt=""
              draggable="false"
              style:width="{lw}px"
              style:left="{(b.w - lw) / 2}px"
              style:top="{(b.h - lh) / 2}px"
              style:opacity={app.logoOpacity}
              style:transform={app.logoAngle ? `rotate(${app.logoAngle}deg)` : null}
            />
          </div>
        {/if}
      {/key}
    {/if}
    {#if app.captionText.trim() && !app.cropMode && app.container !== "audio"}
      {@const r = outRect}
      {@const fs = (r.h * app.captionSize) / 100}
      <div class="caption" class:top={app.captionTop} class:custom={app.captionCustom} style:left="{r.x}px" style:top="{r.y}px" style:width="{r.w}px" style:height="{r.h}px">
        <span
          role="presentation"
          title="Drag to move the caption"
          bind:offsetWidth={capW}
          bind:offsetHeight={capH}
          onpointerdown={dragCaption}
          style:left={app.captionCustom ? `${(r.w - capW) * app.captionX}px` : null}
          style:top={app.captionCustom ? `${(r.h - capH) * app.captionY}px` : null}
          style:font-size="{fs}px"
          style:color={app.captionColor}
          style:-webkit-text-stroke={app.captionOutline > 0 ? `${Math.max(1, (fs * app.captionOutline) / 100) * 2}px ${app.captionOutlineColor}` : null}
          style:font-family={captionCss?.family}
          style:font-weight={captionCss?.weight}
          style:font-style={captionCss?.style}>{app.captionText}</span
        >
      </div>
    {/if}
  {/if}

  <svg class="fx" aria-hidden="true">
    <filter id="cwm-adjust" color-interpolation-filters="sRGB">
      <feComponentTransfer>
        <feFuncR type="linear" slope={adj.contrast} intercept={0.5 - 0.5 * adj.contrast + adj.brightness} />
        <feFuncG type="linear" slope={adj.contrast} intercept={0.5 - 0.5 * adj.contrast + adj.brightness} />
        <feFuncB type="linear" slope={adj.contrast} intercept={0.5 - 0.5 * adj.contrast + adj.brightness} />
      </feComponentTransfer>
      <feComponentTransfer>
        <feFuncR type="gamma" amplitude="1" exponent={1 / adj.gamma} offset="0" />
        <feFuncG type="gamma" amplitude="1" exponent={1 / adj.gamma} offset="0" />
        <feFuncB type="gamma" amplitude="1" exponent={1 / adj.gamma} offset="0" />
      </feComponentTransfer>
      <feColorMatrix type="saturate" values={String(adj.saturation)} />
      <feColorMatrix type="hueRotate" values={String(adj.hue)} />
      {#if table}
        <feComponentTransfer>
          <feFuncR type="table" tableValues={table} />
          <feFuncG type="table" tableValues={table} />
          <feFuncB type="table" tableValues={table} />
        </feComponentTransfer>
      {/if}
    </filter>
  </svg>

  {#if app.proxyFraction !== null}
    <div class="note">
      <div>This file cannot be played directly. Making a preview copy...</div>
      <div class="bar"><div style:width="{Math.round(app.proxyFraction * 100)}%"></div></div>
      <div class="mono muted">{Math.round(app.proxyFraction * 100)}%</div>
      <button class="btn small" onclick={() => ipc.proxyCancel().catch(() => {})}>Cancel</button>
    </div>
  {/if}
  {#if app.videoSrc}
    <div class="vctl">
    {#if hasSound}
    <div class="sound" class:off={app.muted} onwheel={soundWheel}>
      <input
        type="range"
        min="0"
        max="1"
        step="0.01"
        aria-label="Preview volume"
        value={app.muted ? 0 : app.volume}
        oninput={(e) => app.setVolume(Number((e.currentTarget as HTMLInputElement).value))}
      />
      <button
        class="spk"
        aria-label={app.muted ? "Unmute the preview (M)" : "Mute the preview (M)"}
        title="{app.muted ? 'Unmute' : 'Mute'} the preview (M). The encode's sound is set in the sidebar."
        onclick={() => app.toggleMute()}
      >
        <Icon name={app.muted ? "volumeOff" : app.volume < 0.5 ? "volumeLow" : "volume"} size={18} />
      </button>
    </div>
    {/if}
    <button
      class="spk loopbtn"
      class:on={app.loop}
      aria-pressed={app.loop}
      aria-label="Loop the selected range (L)"
      title="Loop the selected range (L)"
      onclick={() => app.toggleLoop()}
    ><Icon name="loop" size={16} /></button>
    </div>
  {/if}
  {#if app.seekPending && app.paused && app.media && !app.audioOnly}
    <!-- shown after 150 ms, so quick seeks do not flash it -->
    <span class="loading" title="The player is still decoding the frame at the playhead; the picture is the previous one until this goes away">Loading frame...</span>
  {/if}
  {#if app.usingProxy}
    <span class="badge" title={app.proxyRemuxed ? "The source's own video and audio, copied into an MP4 the player can read" : "A 480p H.264 copy for the player; encoding uses the original"}>{app.proxyRemuxed ? "Preview copy (full quality)" : "Preview copy (480p)"}</span>
  {/if}
</div>

<style>
  .loading {
    position: absolute;
    top: 8px;
    left: 8px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 11px;
    pointer-events: none;
    opacity: 0;
    animation: loading-show 0s linear 150ms forwards;
  }
  @keyframes loading-show {
    to {
      opacity: 1;
    }
  }
  .logo {
    position: absolute;
    cursor: move;
    touch-action: none;
  }
  .logo img {
    position: absolute;
    pointer-events: none;
  }
  .logo.custom {
    outline: 1px dashed rgb(255 255 255 / 0.6);
    outline-offset: 2px;
  }
  .tiles {
    position: absolute;
    overflow: hidden;
    pointer-events: none;
  }
  .tiles .grid {
    position: absolute;
    transform-origin: center;
  }
  .tiles .cell {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .fade {
    position: absolute;
    background: #000;
    pointer-events: none;
  }
  .caption {
    position: absolute;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    padding: 4% 0;
    box-sizing: border-box;
    pointer-events: none;
    overflow: hidden;
  }
  .caption.top {
    justify-content: flex-start;
  }
  .caption.custom {
    display: block;
    padding: 0;
  }
  .caption.custom span {
    position: absolute;
  }
  .caption span {
    color: #fff;
    font-family: Arial, "DejaVu Sans", Helvetica, sans-serif;
    font-weight: 700;
    line-height: 1.15;
    text-align: center;
    white-space: pre;
    paint-order: stroke fill;
    pointer-events: auto;
    cursor: move;
    touch-action: none;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    background: var(--bg-stage);
    overflow: hidden;
  }
  .pic {
    position: absolute;
  }
  /* the picture of a sound file, over the (empty) video, which still plays the sound */
  .still {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }
  .fx {
    position: absolute;
    width: 0;
    height: 0;
  }
  video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }
  .note {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: rgba(0, 0, 0, 0.75);
    color: var(--text-secondary);
  }
  .bar {
    width: 260px;
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--accent);
  }
  .followhint {
    position: absolute;
    left: 50%;
    top: 10px;
    transform: translateX(-50%);
    padding: 3px 10px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 12px;
    pointer-events: none;
  }
  .outmask {
    position: absolute;
    box-shadow: 0 0 0 9999px var(--bg-stage);
    pointer-events: none;
  }
  .stage.panning {
    cursor: grab;
  }
  /* the corner controls, bottom right: the speaker, and Loop in the corner */
  .vctl {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .loopbtn {
    background: rgb(0 0 0 / 0.55) !important;
    color: #fff;
    opacity: 0.7;
  }
  .loopbtn:hover {
    opacity: 1;
  }
  .loopbtn.on {
    opacity: 1;
    color: var(--accent);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  /* the preview's sound, bottom right: the speaker, and the volume slider beside it while the pointer is over it */
  .sound {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px;
    border-radius: 18px;
    background: rgb(0 0 0 / 0.55);
    color: #fff;
    opacity: 0.7;
    transition: opacity 0.15s;
  }
  .sound:hover {
    opacity: 1;
  }
  .sound.off {
    opacity: 0.95;
  }
  .sound input {
    width: 0;
    opacity: 0;
    margin: 0;
    transition:
      width 0.15s,
      opacity 0.15s;
  }
  .sound:hover input {
    width: 90px;
    opacity: 1;
    margin-left: 6px;
  }
  .spk {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }
  .spk:hover {
    background: rgb(255 255 255 / 0.15);
  }
  .badge {
    position: absolute;
    right: 8px;
    top: 8px;
    padding: 1px 6px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.6);
    color: var(--text-muted);
    font-size: 11px;
    pointer-events: none;
  }
</style>
