<script lang="ts">
  // Plays the 3-second sample from preview_sample, looped, with its size and bitrate.
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { app } from "$lib/state.svelte";
  import { fmtBytes, fmtTime } from "$lib/time";

  const s = $derived(app.sample!);
  let box: HTMLDivElement;

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
      <video src={convertFileSrc(s.path)} autoplay loop controls playsinline></video>
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
  video {
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
