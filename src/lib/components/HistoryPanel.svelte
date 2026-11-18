<script lang="ts">
  // The History panel (H, or the clock button in the top bar): undo and redo, and every edit of the
  // open file with what changed and when, newest first. Click a step to go
  // back (or forward) to it; steps after the current one are undone and stay
  // until a new edit replaces them.
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  const rows = $derived(app.steps.map((s, i) => ({ ...s, i })).reverse());
  /** the "Start fresh" warning is shown */
  let asking = $state(false);

  $effect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        // Esc closes the warning first, then the panel
        if (asking) asking = false;
        else app.historyOpen = false;
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  });
</script>

<div class="scrim" role="presentation" onpointerdown={() => (app.historyOpen = false)}></div>
<div class="panel" aria-label="Edit history">
  <header>
    <Icon name="history" size={15} />
    <h2>History</h2>
    <span class="grow"></span>
    <button class="btn small icon" disabled={!app.canUndo} aria-label="Undo (Ctrl+Z)" title="Undo (Ctrl+Z)" onclick={() => app.undo()}><Icon name="undo" size={14} /></button>
    <button class="btn small icon" disabled={!app.canRedo} aria-label="Redo (Ctrl+Y)" title="Redo (Ctrl+Y)" onclick={() => app.redo()}><Icon name="redo" size={14} /></button>
    <button class="btn small icon" aria-label="Close (Esc)" onclick={() => (app.historyOpen = false)}><Icon name="x" size={12} /></button>
  </header>
  <div class="body">
    {#if rows.length <= 1}
      <div class="empty muted">No edits yet. Ranges, their order, the crop, rotation, flips, adjustments and the speed ramp are recorded.</div>
    {/if}
    {#each rows as r (r.i)}
      <button class="step" class:cur={r.i === app.at} class:undone={r.i > app.at} title={r.i > app.at ? "Undone: click to redo up to here" : r.i === app.at ? "The current state" : "Go back to this step"} onclick={() => app.goToStep(r.i)}>
        <span class="label">{r.label}</span>
        <span class="time mono">{time(r.time)}</span>
      </button>
    {/each}
  </div>
  <footer class="muted">
    <span>Click a step to go back to it. <kbd>Ctrl+Z</kbd> / <kbd>Ctrl+Y</kbd> step back and forward.</span>
    <button class="btn small danger" disabled={!app.media} title="Remove every edit of this file and its history" onclick={() => (asking = true)}>Start fresh</button>
  </footer>
</div>

{#if asking}
  <div class="backdrop fresh" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (asking = false)}>
    <div class="dialog ask" role="dialog" aria-modal="true" aria-labelledby="fresh-title" aria-describedby="fresh-text">
      <h2 id="fresh-title"><span class="warnic"><Icon name="warn" size={18} /></span>Start fresh?</h2>
      <p id="fresh-text">
        Every edit of {app.media?.fileName ?? "this file"} goes: the ranges and their order, the crop, rotation and flips, the
        adjustments, the speed ramp, the zoom and the follow path. The history is cleared, so this cannot be undone, and the file
        opens without them next time. The encode settings (preset, size limit and the rest of the sidebar) stay.
      </p>
      <div class="askbtns">
        <button class="btn" onclick={() => (asking = false)}>Cancel</button>
        <button
          class="btn danger"
          onclick={() => {
            app.startFresh();
            asking = false;
          }}>Start fresh</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 55;
  }
  .panel {
    position: fixed;
    z-index: 56;
    right: 14px;
    top: 52px;
    width: 340px;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-top: 2px solid var(--accent);
    border-radius: var(--radius);
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-muted);
    color: var(--accent);
  }
  h2 {
    margin: 0;
    font-size: 13px;
    color: var(--text-primary);
  }
  .grow {
    flex: 1;
  }
  .body {
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }
  .empty {
    padding: 20px 10px;
    font-size: 12px;
    text-align: center;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-secondary);
    text-align: left;
    cursor: pointer;
    font-size: 12px;
  }
  .step:hover {
    background: var(--bg-panel-alt, var(--bg-chip));
  }
  .step.cur {
    color: var(--text-primary);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .step.undone {
    opacity: 0.45;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time {
    flex: none;
    font-size: 10.5px;
    color: var(--text-muted);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border-top: 1px solid var(--border-muted);
    font-size: 10.5px;
  }
  footer span {
    flex: 1;
  }
  /* the Start fresh warning, over the panel */
  .fresh {
    z-index: 60;
  }
  .ask {
    width: min(420px, calc(100vw - 40px));
  }
  .ask h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 15px;
  }
  .warnic {
    display: inline-flex;
    color: var(--yellow);
  }
  .ask p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .askbtns {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  kbd {
    font-family: var(--mono);
    font-size: 10px;
  }
</style>
