<script lang="ts">
  // Plays the 3-second sample from preview_sample, looped, with its size and bitrate.
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { app } from "$lib/state.svelte";
  import { fmtBytes, fmtTime } from "$lib/time";
  import * as ramp from "$lib/ramp";
  import type { SpeedPoint } from "$lib/types";

  const s = $derived(app.sample!);
  let box: HTMLDivElement;
  // Side by side with the source over the same 3 seconds (the source on the
  // left, cut to the same crop); the source restarts whenever the sample
  // loops, so the two stay roughly in step.
  let compare = $state(false);
  let src = $state<HTMLVideoElement>();
  let lastT = 0;
  // The source plays at the job's speed, so the two show the same moment: a
  // second of the sample is `speed` seconds of the source. With a speed ramp
  // the rate follows the ramp, and only the restarts keep them together.
  const pts = $derived(ramp.clean($state.snapshot(app.speedRamp) as SpeedPoint[]));
  const speed = $derived(Math.min(16, Math.max(0.0625, Number(app.speed) || 1)));
  function sampleTime(e: Event) {
    const v = e.currentTarget as HTMLVideoElement;
    const t = v.currentTime;
    if (src) {
      // follow the sample: restart with it, and correct a drift over 0.25 s
      const want = s.at + t * speed;
      if (t < lastT - 0.5) src.currentTime = pts.length ? s.at : want;
      else if (!pts.length && Math.abs(src.currentTime - want) > 0.25) src.currentTime = want;
      const rate = pts.length ? Math.min(16, Math.max(0.0625, ramp.speedAt(pts, src.currentTime))) : speed;
      if (Math.abs(src.playbackRate - rate) > 0.005) src.playbackRate = rate;
      if (src.paused !== v.paused) void (v.paused ? src.pause() : src.play().catch(() => {}));
    }
    lastT = t;
  }
  function sourceTime() {
    if (src && src.currentTime > s.at + app.sampleSecs + 0.05) src.currentTime = s.at;
  }

  $effect(() => {
    box?.focus();
  });
</script>

<div
  class="backdrop"
  role="presentation"
  onpointerdown={(e) => {
    if (e.target === e.currentTarget) app.closeSample();
  }}
>
  <div
    class="dialog wide"
    role="dialog"
    aria-modal="true"
    aria-labelledby="sample-title"
    tabindex="-1"
    bind:this={box}
    onkeydown={(e) => {
      if (e.key === "Escape" || e.key === "p" || e.key === "P") {
        e.preventDefault();
        app.closeSample();
      }
    }}
  >
    <div class="titlerow">
      <h2 id="sample-title">Sample at {fmtTime(s.at)}</h2>
      <span class="grow"></span>
      <span class="small muted">Length</span>
      {#each [3, 6, 10] as n (n)}
        <button class="btn small" class:on={app.sampleSecs === n} disabled={s.busy} onclick={() => { app.sampleSecs = n; void app.runSample(s.at); }}>{n} s</button>
      {/each}
    </div>

    {#if s.busy}
      <div class="wait muted">Encoding a {app.sampleSecs}-second sample with the current settings...</div>
    {:else if s.error}
      <div class="err msg">{s.error}</div>
    {:else if s.path}
      <!-- svelte-ignore a11y_media_has_caption -->
      <!-- compare: the source on the left, with the same crop, and the sample on the right, at the same size -->
      <div class="panes" class:two={compare}>
        {#if compare && app.videoSrc}
          {@const c = app.crop}
          <div class="pane">
            <div class="cap small muted">Source{app.usingProxy ? " (preview copy)" : ""}{c ? ", same crop" : ""}, no filters</div>
            <div class="cropbox" style:aspect-ratio={c ? `${c.w} / ${c.h}` : `${app.srcW} / ${app.srcH}`} style:--ratio={c ? c.w / c.h : app.srcW / app.srcH}>
              <video
                bind:this={src}
                src={app.videoSrc}
                muted
                autoplay
                playsinline
                style:left={c ? `${(-c.x / c.w) * 100}%` : "0"}
                style:top={c ? `${(-c.y / c.h) * 100}%` : "0"}
                style:width={c ? `${(app.srcW / c.w) * 100}%` : "100%"}
                style:height={c ? `${(app.srcH / c.h) * 100}%` : "100%"}
                onloadedmetadata={(e) => ((e.currentTarget as HTMLVideoElement).currentTime = s.at)}
                ontimeupdate={sourceTime}
              ></video>
            </div>
          </div>
        {/if}
        <div class="pane">
          {#if compare}<div class="cap small muted">Sample (encoded)</div>{/if}
          {#if /\.(gif|webp)$/i.test(s.path)}
            <img src={convertFileSrc(s.path)} alt="Sample" />
          {:else}
            <video src={convertFileSrc(s.path)} autoplay loop controls playsinline ontimeupdate={sampleTime}></video>
          {/if}
        </div>
      </div>
      <label class="small cmp"><input type="checkbox" bind:checked={compare} /> Compare with the source</label>
      <div class="mono small">
        {s.sizeBytes !== null ? fmtBytes(s.sizeBytes) : "size unknown"}
        {#if s.kbps !== null}· {Math.round(s.kbps)} kbps total{/if}
        {#if app.plan?.videoKbps}· full job: video {Math.round(app.plan.videoKbps)} kbps{/if}
      </div>
      {#if app.rateMode === "crf" && s.kbps !== null && app.plan}
        <div class="small">
          At this rate the whole output ({fmtTime(app.plan.durationOut)}) would be about
          <b class="mono">{fmtBytes((s.kbps * 1000 * app.plan.durationOut) / 8)}</b>. A rough guess: other parts of the clip may need more or fewer bits.
        </div>
      {/if}
      <div class="muted small">One pass at the bitrate the full job would get; the final file uses two passes and can look slightly better.</div>
    {/if}

    <div class="actions">
      <button class="btn" onclick={() => app.closeSample()}>Close</button>
    </div>
  </div>
</div>

<style>
  .titlerow {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .titlerow h2 {
    margin: 0;
  }
  .grow {
    flex: 1;
  }
  .wide {
    width: min(820px, calc(100vw - 40px));
    gap: 10px;
  }
  .wide:has(.panes.two) {
    width: min(1500px, calc(100vw - 40px));
  }
  .panes.two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .pane {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  /* the source cut to the crop: the box has the crop's shape, the whole video is moved and scaled inside it */
  .cropbox {
    position: relative;
    overflow: hidden;
    width: min(100%, calc((100vh - 200px) * var(--ratio)));
    margin: 0 auto;
    background: #000;
    border-radius: var(--radius-sm);
  }
  .cropbox video {
    position: absolute;
    max-width: none;
    max-height: none;
    border-radius: 0;
    object-fit: fill;
  }
  .cmp {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  video,
  img {
    width: 100%;
    max-height: calc(100vh - 200px);
    background: #000;
    border-radius: var(--radius-sm);
  }
  .wait {
    padding: 40px 0;
    text-align: center;
  }
  .msg {
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
    font-size: 12px;
  }
  .small {
    font-size: 11px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
