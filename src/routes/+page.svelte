<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { app } from "$lib/state.svelte";
  import * as ipc from "$lib/ipc";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { pickVideos } from "$lib/dialogs";
  import { installKeys } from "$lib/keys";
  import { fmtBytes } from "$lib/time";
  import Icon from "$lib/components/Icon.svelte";
  import Player from "$lib/components/Player.svelte";
  import Timeline from "$lib/components/Timeline.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import Queue from "$lib/components/Queue.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import DropZone from "$lib/components/DropZone.svelte";
  import UrlDialog from "$lib/components/UrlDialog.svelte";
  import SampleDialog from "$lib/components/SampleDialog.svelte";

  async function openFile() {
    try {
      const files = await pickVideos();
      if (files.length) await app.openPaths(files);
    } catch (e) {
      app.error = ipc.errText(e);
    }
  }

  function openUrl(url = "") {
    if (app.urlOpen) return;
    app.urlInitial = url;
    app.urlOpen = true;
  }

  async function reveal(path: string) {
    try {
      await revealItemInDir(path);
    } catch (e) {
      app.error = ipc.errText(e);
    }
  }

  onMount(() => {
    const unlisten: (() => void)[] = [installKeys({ openFile, openUrl })];
    let disposed = false;
    const keep = (p: Promise<() => void>) =>
      p.then((u) => (disposed ? u() : unlisten.push(u))).catch((e) => (app.error = ipc.errText(e)));

    keep(ipc.onJobUpdate((j) => app.upsertJob(j)));
    keep(
      ipc.onProxyProgress((p) => {
        if (app.proxyFraction !== null) app.proxyFraction = Math.min(1, Math.max(0, p.fraction));
      }),
    );
    // Drag-and-drop through Tauri: the browser's own drop event has no file paths.
    keep(
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "enter") app.dragging = true;
        else if (p.type === "leave") app.dragging = false;
        else if (p.type === "drop") {
          app.dragging = false;
          if (p.paths.length) void app.openPaths(p.paths);
        }
      }),
    );
    void app.init();
    return () => {
      disposed = true;
      for (const u of unlisten) u();
    };
  });

  // ask the engine for the plan about 150 ms after the spec stops changing
  $effect(() => {
    const spec = app.spec;
    void JSON.stringify(spec); // read every field so any change re-runs this
    void app.ffmpegMissing;
    const t = setTimeout(() => void app.runPlan(spec), 150);
    return () => clearTimeout(t);
  });

  // remember the option values (localStorage) shortly after they change
  $effect(() => {
    void JSON.stringify(app.optionsSnapshot());
    // before init() has restored the stored values there is nothing to save
    if (!app.ready) return;
    const t = setTimeout(() => app.saveOptions(), 300);
    return () => clearTimeout(t);
  });
</script>

<div class="app">
  <header>
    <span class="logo">comfywebm</span>
    <button class="btn" onclick={openFile}><Icon name="open" size={14} />Open</button>
    <button class="btn" onclick={() => openUrl()}><Icon name="link" size={14} />URL</button>
    {#if app.media}
      <span class="file">
        <span class="fname">{app.media.fileName}</span>
        <span class="mono muted">
          {app.media.video.width}x{app.media.video.height} · {Math.round(app.media.video.fps * 100) / 100} fps · {app.media.video.codec}
          · {fmtBytes(app.media.sizeBytes)}{app.media.video.interlaced ? " · interlaced" : ""}
        </span>
      </span>
    {:else}
      <span class="file"></span>
    {/if}
    {#if app.busy && app.media}<span class="muted">{app.busy}</span>{/if}
    <button class="btn icon" aria-label="Settings" onclick={() => (app.settingsOpen = true)}><Icon name="gear" size={15} /></button>
  </header>

  {#if app.ffmpegMissing && app.media}
    <div class="banner err">
      <Icon name="warn" size={14} />
      <span>ffmpeg or ffprobe was not found. Encoding is not possible until the path is set.</span>
      <button class="btn small" onclick={() => (app.settingsOpen = true)}>Settings</button>
    </div>
  {/if}
  {#if app.notice}
    <div class="banner note">
      <Icon name="check" size={14} />
      <span class="msg">{app.notice.text}</span>
      {#if app.notice.path}
        {@const path = app.notice.path}
        <button class="btn small" onclick={() => reveal(path)}><Icon name="reveal" size={11} />Reveal</button>
      {/if}
      <button class="btn small icon" aria-label="Dismiss" onclick={() => (app.notice = null)}><Icon name="x" size={12} /></button>
    </div>
  {/if}
  {#if app.error}
    <div class="banner err">
      <Icon name="warn" size={14} />
      <span class="msg">{app.error}</span>
      <button class="btn small icon" aria-label="Dismiss" onclick={() => (app.error = null)}><Icon name="x" size={12} /></button>
    </div>
  {/if}

  <div class="main">
    <div class="left">
      {#if app.media}
        <Player />
        <Timeline />
      {:else}
        <DropZone onopen={openFile} onurl={openUrl} />
      {/if}
      <Queue />
    </div>
    <Sidebar />
  </div>

  {#if app.dragging}
    <div class="dropping"><div>Drop to open</div></div>
  {/if}
  {#if app.urlOpen}
    <UrlDialog />
  {/if}
  {#if app.sample}
    <SampleDialog />
  {/if}
  {#if app.settingsOpen}
    <SettingsDialog />
  {/if}
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border-muted);
    background: var(--bg-panel);
  }
  .logo {
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--accent);
  }
  .file {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 10px;
    overflow: hidden;
    white-space: nowrap;
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    flex-shrink: 1;
  }
  .file .mono {
    font-size: 11px;
    flex-shrink: 0;
  }
  .banner {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    border-bottom: 1px solid color-mix(in srgb, var(--red) 35%, transparent);
    background: color-mix(in srgb, var(--red) 10%, var(--bg-panel));
    font-size: 12px;
  }
  .banner.note {
    border-bottom-color: color-mix(in srgb, var(--accent) 35%, transparent);
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-panel));
    color: var(--text-primary);
  }
  .banner .msg {
    flex: 1;
    min-width: 0;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .main {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .left {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .dropping {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--bg-app) 70%, transparent);
    pointer-events: none;
  }
  .dropping > div {
    padding: 24px 40px;
    border: 2px dashed var(--accent);
    border-radius: 12px;
    color: var(--accent-strong);
    font-size: 16px;
    font-weight: 600;
    background: var(--bg-panel);
  }
</style>
