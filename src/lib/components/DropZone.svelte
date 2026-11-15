<script lang="ts">
  // Empty state: drop target, Open button, URL field, and which tools are missing.
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";
  import ToolsMissing from "./ToolsMissing.svelte";

  let { onopen, onurl }: { onopen: () => void; onurl: (url: string) => void } = $props();

  let url = $state("");
</script>

<div class="empty">
  <div class="box">
    <Icon name="film" size={40} />
    <div class="big">Drop a video here</div>
    <div class="muted small">Several files at once each become a whole-file job in the queue.</div>
    <button class="btn primary" onclick={onopen}><Icon name="open" size={14} />Open file <kbd>Ctrl+O</kbd></button>
    {#if app.busy}<div class="muted">{app.busy}</div>{/if}

    <div class="url">
      <input
        class="field"
        type="text"
        spellcheck="false"
        placeholder="or paste a video URL"
        bind:value={url}
        onkeydown={(e) => {
          if (e.key === "Enter" && url.trim()) onurl(url.trim());
        }}
      />
      <button class="btn" disabled={!url.trim()} onclick={() => onurl(url.trim())}><Icon name="link" size={14} />Open URL</button>
    </div>

    {#if app.ffmpegMissing}<ToolsMissing which="ffmpeg" />{/if}
    {#if app.ytdlpMissing}<ToolsMissing which="ytdlp" />{/if}

    <div class="keys muted">
      <span><kbd>Space</kbd> play</span>
      <span><kbd>I</kbd> <kbd>O</kbd> in / out</span>
      <span><kbd>N</kbd> new range</span>
      <span><kbd>,</kbd> <kbd>.</kbd> frame</span>
      <span><kbd>C</kbd> crop</span>
      <span><kbd>P</kbd> sample</span>
      <span><kbd>S</kbd> save frame</span>
      <span><kbd>Ctrl+U</kbd> URL</span>
    </div>
  </div>
</div>

<style>
  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    min-height: 0;
    overflow-y: auto;
  }
  .box {
    width: min(540px, 100%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 28px 24px;
    border: 2px dashed var(--border);
    border-radius: 12px;
    color: var(--text-muted);
    text-align: center;
  }
  .big {
    font-size: 16px;
    color: var(--text-secondary);
  }
  .small {
    font-size: 11px;
    margin-top: -8px;
  }
  .url {
    display: flex;
    gap: 6px;
    width: 100%;
  }
  .url input {
    flex: 1;
    height: 28px;
  }
  .btn kbd {
    font-size: 10px;
    background: transparent;
    color: inherit;
    border-color: color-mix(in srgb, var(--text-inverse) 40%, transparent);
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px 14px;
    font-size: 11px;
  }
</style>
