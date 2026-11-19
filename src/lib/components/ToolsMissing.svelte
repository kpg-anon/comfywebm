<script lang="ts">
  // Says which tool was not found and installs it in place (src-tauri
  // install.rs), so nobody has to leave the app; or opens Settings to point
  // at a copy that is already on the computer.
  import { app } from "$lib/state.svelte";
  import { ffmpegInstall } from "$lib/toolinstall";
  import ToolInstall from "./ToolInstall.svelte";
  let { which }: { which: "ffmpeg" | "ytdlp" } = $props();
  const missing = $derived(
    which === "ffmpeg"
      ? [!app.tools?.ffmpeg && "ffmpeg", !app.tools?.ffprobe && "ffprobe"].filter(Boolean).join(" and ")
      : "yt-dlp",
  );
</script>

<div class="missing">
  <div class="err"><b>{missing} was not found.</b></div>
  <div>
    {#if which === "ffmpeg"}
      comfywebm needs ffmpeg and ffprobe to read and encode video. Install them here, or point Settings at a copy you already have.
    {:else}
      Opening URLs needs yt-dlp. Install it here, or point Settings at a copy you already have.
    {/if}
  </div>
  {#if which === "ffmpeg"}
    <ToolInstall tool="ffmpeg" label={ffmpegInstall.label} note={ffmpegInstall.note} />
  {:else}
    <ToolInstall tool="ytdlp" label="Install yt-dlp" note="the latest release from yt-dlp's GitHub, 23 to 41 MB" />
  {/if}
  <button class="btn small" onclick={() => (app.settingsOpen = true)}>Set the path in Settings</button>
</div>

<style>
  .missing {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--red) 40%, transparent);
    border-radius: var(--radius);
    color: var(--text-secondary);
    font-size: 12px;
    text-align: left;
  }
</style>
