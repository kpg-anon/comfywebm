<script lang="ts">
  import { app } from "$lib/state.svelte";
  import CropOverlay from "./CropOverlay.svelte";

  let video = $state<HTMLVideoElement>();
  let sw = $state(0);
  let sh = $state(0);

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
    const aw = m?.video.width || 16;
    const ah = m?.video.height || 9;
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
  const transform = $derived(
    app.cropMode ? "none" : `${app.mirror ? "scaleX(-1) " : ""}rotate(${app.rotate}deg)`,
  );

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
    if (v.videoWidth === 0) {
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
      <CropOverlay rect={{ x: 0, y: 0, w: box.w, h: box.h }} />
    </div>
  {/if}

  {#if app.proxyFraction !== null}
    <div class="note">
      <div>This file cannot be played directly. Making a preview copy...</div>
      <div class="bar"><div style:width="{Math.round(app.proxyFraction * 100)}%"></div></div>
      <div class="mono muted">{Math.round(app.proxyFraction * 100)}%</div>
    </div>
  {/if}
  {#if app.usingProxy}
    <span class="badge">Preview copy</span>
  {/if}
</div>

<style>
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
