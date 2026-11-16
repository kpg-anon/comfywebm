<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import * as ipc from "$lib/ipc";
  import { app, isNeutral } from "$lib/state.svelte";
  import { sample } from "$lib/curve";
  import CropOverlay from "./CropOverlay.svelte";

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

  $effect(() => {
    app.video = video ?? null;
    return () => {
      if (app.video === video) app.video = null;
    };
  });

  // The picture box inside the stage. object-fit would letterbox; the box is
  // computed here instead so the crop overlay and the rotate/mirror preview
  // share it. The source display size gives the aspect, so the proxy (smaller
  // pixel size, same aspect) maps to the same box. In crop mode the preview is
  // shown unrotated, because the crop is stored in unrotated source pixels.
  const turned = $derived(!app.cropMode && (app.rotate === 90 || app.rotate === 270));
  const box = $derived.by(() => {
    const m = app.media;
    // a sound file: the shape of its picture
    const still = app.stillSize;
    const aw = (m?.audioOnly ? still?.[0] : m?.video.width) || 16;
    const ah = (m?.audioOnly ? still?.[1] : m?.video.height) || 9;
    if (sw <= 0 || sh <= 0) return { x: 0, y: 0, w: 0, h: 0 };
    const a = turned ? ah / aw : aw / ah; // aspect as shown on screen
    let w = sw;
    let h = sw / a;
    if (h > sh) {
      h = sh;
      w = sh * a;
    }
    // the element itself keeps the unrotated shape; the transform turns it
    if (turned) [w, h] = [h, w];
    return { x: (sw - w) / 2, y: (sh - h) / 2, w, h };
  });
  // the flips apply after the rotation, as in the encode (rotate, then hflip, then vflip)
  const transform = $derived(
    app.cropMode ? "none" : `${app.vflip ? "scaleY(-1) " : ""}${app.mirror ? "scaleX(-1) " : ""}rotate(${app.rotate}deg)`,
  );

  // Color adjustments on the picture: an SVG filter close to ffmpeg's eq,
  // hue and curves (not exact: eq works on YUV, this on RGB). Sample shows
  // the real result.
  // the speed setting plays in the preview too (pitch kept, as atempo does)
  // (defaultPlaybackRate too: loading a new source resets the rate to it)
  $effect(() => {
    const r = Math.min(16, Math.max(0.0625, Number(app.speed) || 1));
    if (video) {
      video.defaultPlaybackRate = r;
      video.playbackRate = r;
    }
  });
  // Fade preview: how dark the picture is at the playhead, from the fades at
  // the start of the first output range and the end of the last (in source
  // seconds, so the speed counts).
  const fadeDark = $derived.by(() => {
    const fi = Number(app.fadeIn) || 0;
    const fo = Number(app.fadeOut) || 0;
    if ((!fi && !fo) || app.cropMode || app.playback !== "forward") return 0;
    const segs = app.outSegments;
    const sp = Number(app.speed) || 1;
    const start = segs.length ? segs[0].start : 0;
    const end = segs.length ? segs[segs.length - 1].end : app.duration;
    const t = app.current;
    let d = 0;
    if (fi > 0 && t >= start && t < start + fi * sp) d = Math.max(d, 1 - (t - start) / (fi * sp));
    if (fo > 0 && t <= end && t > end - fo * sp) d = Math.max(d, 1 - (end - t) / (fo * sp));
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
    // keep the playhead when switching to the proxy
    if (app.current > 0) v.currentTime = app.current;
  }
</script>

<div class="stage" bind:clientWidth={sw} bind:clientHeight={sh} onwheel={onWheel}>
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
        if (video && video.paused) app.current = video.currentTime;
      }}
      onended={() => app.onEnded()}
      onclick={() => {
        if (!app.cropMode) app.togglePlay();
      }}
      ></video>
      {#if app.audioOnly && app.stillUrl}
        <img class="still" src={app.stillUrl} alt="" draggable="false" style:filter={showAdj ? "url(#cwm-adjust)" : null} />
      {/if}
      <CropOverlay rect={{ x: 0, y: 0, w: box.w, h: box.h }} />
    </div>
    {#if fadeDark > 0}
      <div class="fade" style:left="{box.x}px" style:top="{box.y}px" style:width="{box.w}px" style:height="{box.h}px" style:opacity={fadeDark}></div>
    {/if}
    {#if app.logoPath && app.plan && !app.cropMode && app.container !== "audio"}
      {@const c = app.crop && !turned ? app.crop : null}
      {@const r = c && app.srcW ? { x: box.x + (c.x / app.srcW) * box.w, y: box.y + (c.y / app.srcH) * box.h, w: (c.w / app.srcW) * box.w, h: (c.h / app.srcH) * box.h } : { x: box.x, y: box.y, w: box.w, h: box.h }}
      {@const m = Math.min(r.w, r.h) * 0.03}
      {#key logoTry}
      <img
        class="logo"
        onerror={() => logoTry < 5 && setTimeout(() => logoTry++, 400)}
        src={convertFileSrc(app.logoPath)}
        alt=""
        draggable="false"
        style:width="{(r.w * app.logoSize) / 100}px"
        style:opacity={app.logoOpacity}
        style:left={app.logoCorner.endsWith("l") ? `${r.x + m}px` : null}
        style:right={app.logoCorner.endsWith("r") ? `${sw - (r.x + r.w) + m}px` : null}
        style:top={app.logoCorner.startsWith("t") ? `${r.y + m}px` : null}
        style:bottom={app.logoCorner.startsWith("b") ? `${sh - (r.y + r.h) + m}px` : null}
      />
      {/key}
    {/if}
    {#if app.captionText.trim() && !app.cropMode && app.container !== "audio"}
      {@const c = app.crop && !turned ? app.crop : null}
      {@const r = c && app.srcW ? { x: box.x + (c.x / app.srcW) * box.w, y: box.y + (c.y / app.srcH) * box.h, w: (c.w / app.srcW) * box.w, h: (c.h / app.srcH) * box.h } : { x: box.x, y: box.y, w: box.w, h: box.h }}
      <div class="caption" class:top={app.captionTop} style:left="{r.x}px" style:top="{r.y}px" style:width="{r.w}px" style:height="{r.h}px">
        <span style:font-size="{(r.h * app.captionSize) / 100}px" style:-webkit-text-stroke="{Math.max(1, (r.h * app.captionSize) / 100 / 14)}px black">{app.captionText}</span>
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
  {#if app.usingProxy}
    <span class="badge" title={app.proxyRemuxed ? "The source's own video and audio, copied into an MP4 the player can read" : "A 480p H.264 copy for the player; encoding uses the original"}>{app.proxyRemuxed ? "Preview copy (full quality)" : "Preview copy (480p)"}</span>
  {/if}
</div>

<style>
  .logo {
    position: absolute;
    pointer-events: none;
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
  .caption span {
    color: #fff;
    font-family: Arial, "DejaVu Sans", Helvetica, sans-serif;
    font-weight: 700;
    line-height: 1.15;
    text-align: center;
    white-space: pre;
    paint-order: stroke fill;
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
