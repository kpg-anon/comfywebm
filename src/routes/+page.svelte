<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { installAppFeel } from "$lib/appfeel";
  import { installTooltips } from "$lib/tooltip";
  import KeysDialog from "$lib/components/KeysDialog.svelte";
  import Logo from "$lib/components/Logo.svelte";
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

  let version = $state("");
  const queueTitle = $derived(
    app.activeJobs
      ? `${app.activeJobs} job${app.activeJobs === 1 ? "" : "s"} running or waiting`
      : app.unseen.done || app.unseen.failed
        ? `${app.unseen.done} done, ${app.unseen.failed} failed since you last looked`
        : "Queue",
  );
  // the drop target: the player or the empty-state drop zone, or the queue list when the queue is shown
  let preview = $state<HTMLDivElement>();
  let queueView = $state<HTMLDivElement>();

  /** Is a drag position (physical pixels, from Tauri) over the drop target of the shown view? */
  function overPreview(p: { x: number; y: number }): boolean {
    const el = app.view === "queue" ? queueView : preview;
    if (!el) return false;
    const r = el.getBoundingClientRect();
    const x = p.x / devicePixelRatio;
    const y = p.y / devicePixelRatio;
    return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
  }

  onMount(() => {
    const unlisten: (() => void)[] = [installKeys({ openFile, openUrl }), installAppFeel(), installTooltips()];
    getVersion()
      .then((v) => (version = v))
      .catch(() => {});
    // dev builds: the state object for testing over the DevTools protocol
    if (import.meta.env.DEV) (window as unknown as { __cwm: typeof app }).__cwm = app;
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
        if (p.type === "enter" || p.type === "over") app.dragging = overPreview(p.position);
        else if (p.type === "leave") app.dragging = false;
        else if (p.type === "drop") {
          app.dragging = false;
          if (p.paths.length && overPreview(p.position)) {
            // one file opens in the editor; several become queue jobs
            if (p.paths.length === 1) app.setView("editor");
            void app.openPaths(p.paths);
          }
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

  // an undo step once ranges, order or crop have stopped changing for a moment (a drag is one step)
  $effect(() => {
    void app.editKey();
    if (!app.media) return;
    const t = setTimeout(() => app.noteEdit(), 350);
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
    <span class="brand">
      <Logo size={22} />
      <span class="logo">comfywebm</span>
      {#if version}<span class="ver mono">v{version}</span>{/if}
    </span>
    <div class="views" role="tablist" aria-label="View">
      <button class="view" class:on={app.view === "editor"} role="tab" aria-selected={app.view === "editor"} onclick={() => app.setView("editor")}>
        <Icon name="film" size={13} />Editor
      </button>
      <button
        class="view"
        class:on={app.view === "queue"}
        role="tab"
        aria-selected={app.view === "queue"}
        title={queueTitle}
        onclick={() => app.setView("queue")}
      >
        <Icon name="queue" size={13} />Queue
        {#if app.activeJobs}
          <span class="badge busy" class:run={!!app.runningJob}>
            {#if app.runningJob}<i class="pulse"></i>{/if}{app.activeJobs}{#if app.runningJob?.progress}<span class="pct">· {Math.round(app.runningJob.progress.fraction * 100)}%</span>{/if}
          </span>
        {:else if app.unseen.failed}
          <span class="badge failed">{app.unseen.failed + app.unseen.done}</span>
        {:else if app.unseen.done}
          <span class="badge done">{app.unseen.done}</span>
        {/if}
      </button>
    </div>
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
    <button class="btn icon" aria-label="Keys (? or F1)" onclick={() => (app.keysOpen = true)}><Icon name="keyboard" size={16} /></button>
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
    <div class="left" class:hidden={app.view !== "editor"}>
      <div class="preview" bind:this={preview}>
        {#if app.media}
          <Player />
        {:else}
          <DropZone onopen={openFile} onurl={openUrl} />
        {/if}
        {#if app.dragging}
          <div class="dropping"><div>Drop to open</div></div>
        {/if}
      </div>
      {#if app.media}
        <Timeline />
      {/if}
    </div>
    {#if app.view === "queue"}
      <div class="left" bind:this={queueView}>
        <Queue />
        {#if app.dragging}
          <div class="dropping"><div>Drop to open</div></div>
        {/if}
      </div>
    {/if}
    <Sidebar />
  </div>

  {#if app.urlOpen}
    <UrlDialog />
  {/if}
  {#if app.sample}
    <SampleDialog />
  {/if}
  {#if app.settingsOpen}
    <SettingsDialog />
  {/if}
  {#if app.keysOpen}
    <KeysDialog />
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
    background: var(--header-bg);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-right: 4px;
  }
  .logo {
    font-family: var(--font-head);
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--text-primary);
  }
  .ver {
    font-size: 10.5px;
    color: var(--text-muted);
    padding: 0 5px;
    border: 1px solid var(--border-muted);
    border-radius: var(--radius-sm);
  }
  .preview {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
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
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  /* the editor stays mounted while the queue is shown, so playback state and the video element survive */
  .left.hidden {
    display: none;
  }
  .views {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-seg);
    background: var(--bg-chip);
  }
  .view {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: calc(var(--ctl-h) - 6px);
    padding: 0 10px;
    border: 0;
    border-radius: calc(var(--radius-seg) - 2px);
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: var(--btn-weight);
    text-transform: var(--btn-case);
    cursor: pointer;
  }
  .view:hover {
    color: var(--text-primary);
  }
  .view.on {
    background: var(--accent-bg);
    color: var(--accent-strong);
    box-shadow: inset 0 0 0 1px var(--accent-line);
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 17px;
    height: 17px;
    padding: 0 5px;
    border-radius: 9px;
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    text-transform: none;
  }
  .badge.busy {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .badge.done {
    background: var(--green);
    color: var(--text-inverse);
  }
  .badge.failed {
    background: var(--red);
    color: var(--text-inverse);
  }
  .pct {
    font-weight: 600;
    opacity: 0.85;
  }
  .pulse {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.25;
    }
  }
  .dropping {
    position: absolute;
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
    border-radius: var(--radius-dialog);
    color: var(--accent-strong);
    font-size: 16px;
    font-weight: 600;
    background: var(--bg-panel);
  }
</style>
