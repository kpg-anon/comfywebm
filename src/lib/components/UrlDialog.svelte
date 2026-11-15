<script lang="ts">
  // Open URL (yt-dlp): look up, pick a quality and optionally a part, download,
  // then open the downloaded file like a local one.
  import { onMount } from "svelte";
  import { app } from "$lib/state.svelte";
  import * as ipc from "$lib/ipc";
  import { fmtTime } from "$lib/time";
  import type { DownloadProgress, UrlInfo } from "$lib/types";
  import Icon from "./Icon.svelte";
  import TimeField from "./TimeField.svelte";
  import ToolsMissing from "./ToolsMissing.svelte";

  let url = $state(app.urlInitial);
  let info = $state<UrlInfo | null>(null);
  let looking = $state(false);
  let error = $state<string | null>(null);
  let height = $state<number | null>(null);
  let partOn = $state(false);
  let partStart = $state(0);
  let partEnd = $state(0);
  let downloading = $state<string | null>(null); // the URL being downloaded
  let progress = $state<DownloadProgress | null>(null);
  let thumbOk = $state(true);
  let lookedUp = ""; // the URL that `info` belongs to
  let input: HTMLInputElement;

  onMount(() => {
    input?.focus();
    if (url.trim()) void lookUp();
    const un = ipc.onDownloadProgress((p) => {
      if (downloading && p.url === downloading) progress = p;
    });
    return () => void un.then((u) => u());
  });

  async function lookUp() {
    const u = url.trim();
    if (!u || looking || downloading) return;
    looking = true;
    error = null;
    info = null;
    thumbOk = true;
    try {
      const i = await ipc.urlInfo(u);
      info = i;
      lookedUp = u;
      height = i.heights.find((h) => h <= 1080) ?? i.heights[0] ?? null;
      partStart = 0;
      partEnd = i.duration ?? 0;
    } catch (e) {
      error = ipc.errText(e);
    } finally {
      looking = false;
    }
  }

  async function download() {
    const u = url.trim();
    if (!info || downloading) return;
    const section = partOn && partEnd > partStart ? { start: partStart, end: partEnd } : null;
    downloading = u;
    progress = null;
    error = null;
    const title = info.title;
    try {
      const path = await ipc.urlDownload(u, height, section);
      downloading = null;
      close();
      if (await app.open(path)) app.title = title;
    } catch (e) {
      // a cancel also ends here; the backend's message says so
      error = ipc.errText(e);
      downloading = null;
    }
  }

  async function cancel() {
    if (!downloading) return;
    try {
      await ipc.urlCancel(downloading);
    } catch (e) {
      error = ipc.errText(e);
    }
  }

  function close() {
    if (downloading) return;
    app.urlOpen = false;
  }
</script>

<div
  class="backdrop"
  role="presentation"
  onpointerdown={(e) => {
    if (e.target === e.currentTarget) close();
  }}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="url-title"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (downloading) void cancel();
        else close();
      }
    }}
  >
    <h2 id="url-title">Open URL</h2>

    {#if app.ytdlpMissing}
      <ToolsMissing which="ytdlp" />
    {/if}

    <div class="line">
      <input
        bind:this={input}
        class="field"
        type="text"
        spellcheck="false"
        placeholder="https://..."
        bind:value={url}
        disabled={!!downloading}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            if (info && lookedUp === url.trim()) void download();
            else void lookUp();
          }
        }}
      />
      <button class="btn" disabled={!url.trim() || looking || !!downloading} onclick={lookUp}>{looking ? "Looking up..." : "Look up"}</button>
    </div>

    {#if info}
      <div class="info">
        {#if info.thumbnail && thumbOk}
          <img src={info.thumbnail} alt="" onerror={() => (thumbOk = false)} />
        {/if}
        <div class="meta">
          <div class="vtitle">{info.title}</div>
          {#if info.uploader}<div class="muted">{info.uploader}</div>{/if}
          <div class="muted mono small">{info.duration !== null ? fmtTime(info.duration) : "unknown length"}{info.isLive ? " · live" : ""}</div>
        </div>
      </div>

      <label class="row">
        <span>Quality</span>
        <select class="field" bind:value={height} disabled={!!downloading}>
          <option value={null}>Best available</option>
          {#each info.heights as h (h)}
            <option value={h}>Up to {h}p</option>
          {/each}
        </select>
      </label>

      <label class="check"><input type="checkbox" bind:checked={partOn} disabled={!!downloading} /> Only download part</label>
      {#if partOn}
        <div class="part">
          <TimeField label="From" value={partStart} onset={(t) => (partStart = Math.max(0, t))} />
          <TimeField label="To" value={partEnd} onset={(t) => (partEnd = info?.duration ? Math.min(t, info.duration) : t)} />
        </div>
        {#if partEnd <= partStart}<div class="warn small">"To" must be after "From".</div>{/if}
      {/if}
    {/if}

    {#if downloading}
      <div class="progress">
        <div class="bar"><div style:width="{Math.round((progress?.fraction ?? 0) * 100)}%"></div></div>
        <div class="mono small muted">
          {progress?.stage === "merge" ? "Merging audio and video..." : `${Math.round((progress?.fraction ?? 0) * 100)}%`}
          {#if progress?.speed}· {progress.speed}{/if}
          {#if progress?.eta}· {progress.eta} left{/if}
        </div>
      </div>
    {/if}

    {#if error}<div class="err small msg">{error}</div>{/if}

    <div class="actions">
      {#if downloading}
        <button class="btn danger" onclick={cancel}><Icon name="stop" size={12} />Cancel download</button>
      {:else}
        <button class="btn" onclick={close}>Close</button>
        <button class="btn primary" disabled={!info || info.isLive || app.ytdlpMissing || (partOn && partEnd <= partStart)} onclick={download}>
          <Icon name="download" size={14} />Download and open
        </button>
      {/if}
    </div>
    {#if info?.isLive}<div class="warn small">Live streams cannot be downloaded here.</div>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
  }
  .dialog {
    width: min(560px, calc(100vw - 40px));
    max-height: calc(100vh - 40px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--bg-panel);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .line {
    display: flex;
    gap: 6px;
  }
  .line input {
    flex: 1;
    height: 28px;
  }
  .info {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  img {
    width: 160px;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    border-radius: var(--radius-sm);
    background: #000;
    flex: none;
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .vtitle {
    font-weight: 600;
    word-break: break-word;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .row select {
    width: 200px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  .part {
    display: grid;
    grid-template-columns: 120px 120px;
    gap: 8px;
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .bar {
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--accent);
  }
  .small {
    font-size: 11px;
  }
  .msg {
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
