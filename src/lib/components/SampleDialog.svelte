<script lang="ts">
  // Plays the 3-second sample from preview_sample, looped, with its size and bitrate.
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { app } from "$lib/state.svelte";
  import { fmtBytes, fmtTime } from "$lib/time";

  const s = $derived(app.sample!);
  let box: HTMLDivElement;
  // Side by side with the source over the same 3 seconds; the source restarts
  // whenever the sample loops, so the two stay roughly in step.
  let compare = $state(false);
  let src = $state<HTMLVideoElement>();
  let lastT = 0;
  function sampleTime(e: Event) {
    const v = e.currentTarget as HTMLVideoElement;
    const t = v.currentTime;
    if (src) {
      // follow the sample: restart with it, and correct a drift over 0.25 s
      if (t < lastT - 0.5 || Math.abs(src.currentTime - (s.at + t)) > 0.25) src.currentTime = s.at + t;
      if (src.paused !== v.paused) void (v.paused ? src.pause() : src.play().catch(() => {}));
    }
    lastT = t;
  }
  function sourceTime() {
    if (src && src.currentTime > s.at + 3.05) src.currentTime = s.at;
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
    <h2 id="sample-title">Sample at {fmtTime(s.at)}</h2>

    {#if s.busy}
      <div class="wait muted">Encoding a 3-second sample with the current settings...</div>
    {:else if s.error}
      <div class="err msg">{s.error}</div>
    {:else if s.path}
      <!-- svelte-ignore a11y_media_has_caption -->
      <div class="panes" class:two={compare}>
        <div class="pane">
          {#if compare}<div class="cap small muted">Sample (encoded)</div>{/if}
          {#if /\.(gif|webp)$/i.test(s.path)}
            <img src={convertFileSrc(s.path)} alt="Sample" />
          {:else}
            <video src={convertFileSrc(s.path)} autoplay loop controls playsinline ontimeupdate={sampleTime}></video>
          {/if}
        </div>
        {#if compare && app.videoSrc}
          <div class="pane">
            <div class="cap small muted">Source{app.usingProxy ? " (preview copy)" : ""}, no filters or crop</div>
            <video
              bind:this={src}
              src={app.videoSrc}
              muted
              autoplay
              playsinline
              onloadedmetadata={(e) => ((e.currentTarget as HTMLVideoElement).currentTime = s.at)}
              ontimeupdate={sourceTime}
            ></video>
          </div>
        {/if}
      </div>
      <label class="small cmp"><input type="checkbox" bind:checked={compare} /> Compare with the source</label>
      <div class="mono small">
        {s.sizeBytes !== null ? fmtBytes(s.sizeBytes) : "size unknown"}
        {#if s.kbps !== null}· {Math.round(s.kbps)} kbps total{/if}
        {#if app.plan?.videoKbps}· full job: video {Math.round(app.plan.videoKbps)} kbps{/if}
      </div>
      <div class="muted small">One pass at the bitrate the full job would get; the final file uses two passes and can look slightly better.</div>
    {/if}

    <div class="actions">
      <button class="btn" onclick={() => app.closeSample()}>Close</button>
    </div>
  </div>
</div>

<style>
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
