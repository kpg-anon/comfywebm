<script lang="ts">
  // Says which tool was not found, where to get it, and where to set its path.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app } from "$lib/state.svelte";
  import { errText } from "$lib/ipc";

  let { which }: { which: "ffmpeg" | "ytdlp" } = $props();

  const missing = $derived(
    which === "ffmpeg"
      ? [!app.tools?.ffmpeg && "ffmpeg", !app.tools?.ffprobe && "ffprobe"].filter(Boolean).join(" and ")
      : "yt-dlp",
  );
  const link = $derived(which === "ffmpeg" ? "https://ffmpeg.org/download.html" : "https://github.com/yt-dlp/yt-dlp/releases");

  // the webview tells the system: WebView2 on Windows, WebKit on macOS and Linux
  const os = /Mac/.test(navigator.userAgent) ? "mac" : /Linux|X11/.test(navigator.userAgent) ? "linux" : "windows";
  /** a one-line install command for this system, or null on Windows (download page) */
  const install = $derived(
    os === "mac"
      ? which === "ffmpeg"
        ? "brew install ffmpeg"
        : "brew install yt-dlp"
      : os === "linux"
        ? which === "ffmpeg"
          ? "sudo apt install ffmpeg   (Fedora: sudo dnf install ffmpeg)"
          : "pipx install yt-dlp   (or: sudo apt install yt-dlp)"
        : null,
  );

  async function go() {
    try {
      await openUrl(link);
    } catch (e) {
      app.error = errText(e);
    }
  }
</script>

<div class="missing">
  <div class="err"><b>{missing} was not found.</b></div>
  <div>
    {#if which === "ffmpeg"}
      comfywebm needs ffmpeg and ffprobe to read and encode video.
    {:else}
      Opening URLs needs yt-dlp.
    {/if}
    {#if install}
      Install it with <code class="mono sel">{install}</code>{os === "mac" ? " (Homebrew, brew.sh)" : ""}, then restart comfywebm, or download it from
    {:else}
      Download it from
    {/if}
    <button class="link mono" onclick={go}>{link.replace("https://", "")}</button>. It looks in the path set in Settings, then beside the app, then on
    PATH{os === "mac" ? ", then in Homebrew's and MacPorts' folders" : os === "linux" ? ", then in /usr/local/bin, /snap/bin and ~/.local/bin" : ""}.
  </div>
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
  code {
    padding: 0 4px;
    border: 1px solid var(--border-muted);
    border-radius: 4px;
    background: var(--bg-chip);
    font-size: 11px;
  }
  .sel {
    user-select: all;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    text-decoration: underline;
    font-size: 11px;
    cursor: pointer;
  }
</style>
