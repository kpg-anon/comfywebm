<script lang="ts">
  // The update notice in the bottom-right corner: the new version, its notes
  // from the changelog, Update now / Later, then download progress.
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { updater, parseNotes, RELEASES_URL, type Seg } from "$lib/updater.svelte";
  import { app } from "$lib/state.svelte";

  // notes in the same shape release.ps1 publishes, for the dev preview
  const SAMPLE = `### Added

- Caption font, text color, outline color and outline width. The font list
  holds the installed fonts; **File** picks a \`.ttf\` that is not installed.
- Watermark placement: Custom, by dragging the image on the player, and Tile.

### Fixed

- The engine's contract test failed on Windows checkouts with CRLF line
  endings.

---
Full changelog: https://github.com/kpg-anon/comfywebm/blob/main/CHANGELOG.md`;

  onMount(() => {
    if (import.meta.env.DEV) {
      // dev: call __previewUpdate() in the devtools console to see the notice
      (window as unknown as Record<string, unknown>).__previewUpdate = () => updater.preview(SAMPLE);
      (window as unknown as Record<string, unknown>).__cwmUpdater = updater;
    }
    // a little after start, so the first file opens without waiting on the network
    const t = setTimeout(() => {
      if (updater.auto) void updater.check(false);
    }, 4000);
    return () => clearTimeout(t);
  });

  const parsed = $derived(parseNotes(updater.notes));

  function open(url: string | undefined) {
    if (url && /^https:\/\//.test(url)) void openUrl(url).catch(() => {});
  }

  const show = $derived(
    !updater.dismissed && (updater.phase === "available" || updater.phase === "downloading" || updater.phase === "restarting" || (updater.phase === "error" && !!updater.version)),
  );
</script>

{#snippet inline(segs: Seg[])}{#each segs as s, i (i)}{#if s.t === "bold"}<b>{s.v}</b>{:else if s.t === "code"}<code>{s.v}</code>{:else if s.t === "link"}<a href={s.href} onclick={(e) => { e.preventDefault(); open(s.href); }}>{s.v}</a>{:else}{s.v}{/if}{/each}{/snippet}

{#if show}
  <div class="upd" role="status">
    {#if updater.phase === "available"}
      <div class="head">
        <span class="dot"></span>
        <span class="title">Update available: <b>v{updater.version}</b></span>
      </div>
      {#if parsed.blocks.length}
        <div class="notes">
          {#each parsed.blocks as b, i (i)}
            {#if b.kind === "heading"}
              <div class="h">{b.text}</div>
            {:else if b.kind === "bullet"}
              <div class="li"><span class="bul">•</span><span>{@render inline(b.segs)}</span></div>
            {:else}
              <p>{@render inline(b.segs)}</p>
            {/if}
          {/each}
        </div>
      {/if}
      {#if parsed.changelogUrl}
        <button class="link" onclick={() => open(parsed.changelogUrl)}>Full changelog ↗</button>
      {/if}
      {#if updater.mode === "self"}
        <div class="row">
          <button class="btn primary grow" onclick={() => updater.install()}>Update now</button>
          <button class="btn" onclick={() => (updater.dismissed = true)}>Later</button>
        </div>
        <div class="hint">
          The app closes, installs the update and starts again.
          {#if app.runningJob}<span class="warn">An encode is running: it stops and is listed as interrupted.</span>{/if}
        </div>
      {:else}
        <div class="row">
          <button class="btn primary grow" onclick={() => open(RELEASES_URL)}>Open the release page</button>
          <button class="btn" onclick={() => (updater.dismissed = true)}>Later</button>
        </div>
        <div class="hint">
          {#if updater.mode === "portable"}
            This is the portable copy. Download the new portable zip from the release page and unpack it over this folder; the data folder (your settings) stays.
          {:else}
            This copy was installed from a package (.deb or .rpm), which the app cannot replace itself. Download the new package from the release page.
          {/if}
        </div>
      {/if}
    {:else if updater.phase === "downloading"}
      <div class="head"><span class="title">Downloading v{updater.version}...</span></div>
      <div class="bar"><div class="fill" style:width="{updater.progress}%"></div></div>
      <div class="hint">{updater.progress}%. The app restarts when it is installed.</div>
    {:else if updater.phase === "restarting"}
      <div class="head"><span class="title">Installing and restarting...</span></div>
    {:else}
      <div class="head"><span class="title err">The update failed</span></div>
      <div class="hint sel">{updater.error}</div>
      <div class="row"><button class="btn" onclick={() => (updater.dismissed = true)}>Close</button></div>
    {/if}
  </div>
{/if}

<style>
  .upd {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 60;
    width: 360px;
    padding: 13px 15px 14px;
    background: var(--bg-panel);
    border: var(--bw) solid var(--border);
    border-top: 2px solid var(--accent);
    border-radius: var(--radius-dialog, 10px);
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.5);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }
  .title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .title b {
    color: var(--accent-strong);
  }
  .title.err {
    color: var(--red);
  }
  .notes {
    margin-top: 10px;
    max-height: 220px;
    overflow-y: auto;
    padding-right: 6px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .h {
    margin: 10px 0 4px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--accent-strong);
  }
  .h:first-child {
    margin-top: 0;
  }
  .li {
    display: flex;
    gap: 7px;
    margin: 3px 0;
  }
  .bul {
    flex: none;
    color: var(--accent);
  }
  p {
    margin: 5px 0;
  }
  .notes :global(b) {
    color: var(--text-primary);
  }
  .notes :global(code) {
    padding: 0 4px;
    border: 1px solid var(--border-muted);
    border-radius: 4px;
    background: var(--bg-chip);
    font-family: var(--mono);
    font-size: 10.5px;
  }
  .notes :global(a),
  .link {
    color: var(--accent-strong);
    cursor: pointer;
  }
  .link {
    margin-top: 8px;
    padding: 0;
    border: 0;
    background: none;
    font-size: 11px;
  }
  .link:hover,
  .notes :global(a:hover) {
    text-decoration: underline;
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  .grow {
    flex: 1;
  }
  .hint {
    margin-top: 8px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .warn {
    display: block;
    margin-top: 3px;
    color: var(--yellow);
  }
  .sel {
    user-select: text;
    word-break: break-word;
  }
  .bar {
    height: 6px;
    margin-top: 10px;
    overflow: hidden;
    border-radius: 99px;
    background: var(--bg-chip);
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s ease-out;
  }
</style>
