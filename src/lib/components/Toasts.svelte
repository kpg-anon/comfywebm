<script lang="ts">
  // Notifications in the top right, newest on top. Each fades out 6 s after it
  // appears (failures after 12 s); the pointer over a toast holds it. The x
  // dismisses one at once.
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { app, type Toast } from "$lib/state.svelte";
  import { errText } from "$lib/ipc";
  import Icon from "./Icon.svelte";

  const LIFE = { done: 6000, info: 6000, over: 10000, failed: 12000 };
  const FADE = 400;

  let hovered = $state<number | null>(null);
  let fading = $state<Set<number>>(new Set());
  const timers = new Map<number, number>();

  function schedule(t: Toast, ms: number) {
    clearTimeout(timers.get(t.id));
    timers.set(
      t.id,
      window.setTimeout(() => {
        if (hovered === t.id) return;
        fading = new Set([...fading, t.id]);
        timers.set(t.id, window.setTimeout(() => close(t.id), FADE));
      }, ms),
    );
  }

  function close(id: number) {
    clearTimeout(timers.get(id));
    timers.delete(id);
    fading.delete(id);
    app.dismissToast(id);
  }

  // start a timer for every new toast
  $effect(() => {
    for (const t of app.toasts) if (!timers.has(t.id)) schedule(t, LIFE[t.kind]);
  });

  async function run(f: () => Promise<unknown>) {
    try {
      await f();
    } catch (e) {
      app.error = errText(e);
    }
  }

  const ICON = { done: "check", info: "check", over: "warn", failed: "warn" } as const;
</script>

<div class="stack" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div
      class="toast {t.kind}"
      class:fading={fading.has(t.id)}
      role="status"
      onpointerenter={() => {
        hovered = t.id;
        if (fading.has(t.id)) {
          fading.delete(t.id);
          fading = new Set(fading);
        }
        clearTimeout(timers.get(t.id));
      }}
      onpointerleave={() => {
        hovered = null;
        schedule(t, 2500);
      }}
    >
      <span class="ic"><Icon name={ICON[t.kind]} size={14} /></span>
      <div class="body">
        <div class="title">{t.title}</div>
        {#if t.text}<div class="text">{t.text}</div>{/if}
        {#if t.path}
          {@const path = t.path}
          <div class="acts">
            {#if !path.toLowerCase().endsWith(".png")}
              <button class="btn small" onclick={() => run(() => openPath(path))}><Icon name="play" size={10} />Play</button>
            {/if}
            <button class="btn small" onclick={() => run(() => revealItemInDir(path))}><Icon name="reveal" size={10} />Reveal</button>
          </div>
        {/if}
        {#if t.kind === "failed" && t.title.startsWith("Encode")}
          <div class="acts"><button class="btn small" onclick={() => (app.setView("queue"), close(t.id))}>Show in queue</button></div>
        {/if}
      </div>
      <button class="x" aria-label="Dismiss" onclick={() => close(t.id)}><Icon name="x" size={12} /></button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    top: 48px;
    right: 12px;
    z-index: 120;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    gap: 9px;
    padding: 9px 8px 9px 11px;
    border: var(--bw) solid var(--border);
    border-left: 3px solid var(--green);
    border-radius: var(--radius);
    background: var(--bg-panel-alt);
    box-shadow: var(--shadow-pop);
    animation: drop 220ms ease-out;
    transition:
      opacity 400ms ease,
      transform 400ms ease;
  }
  .toast.info {
    border-left-color: var(--accent);
  }
  .toast.over {
    border-left-color: var(--orange);
  }
  .toast.failed {
    border-left-color: var(--red);
  }
  .toast.fading {
    opacity: 0;
    transform: translateX(16px);
  }
  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-10px);
    }
  }
  .ic {
    flex: none;
    margin-top: 1px;
    color: var(--green);
  }
  .info .ic {
    color: var(--accent);
  }
  .over .ic {
    color: var(--orange);
  }
  .failed .ic {
    color: var(--red);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    font-size: 12.5px;
    font-weight: 600;
  }
  .text {
    font-size: 11.5px;
    color: var(--text-secondary);
    white-space: pre-line;
    word-break: break-word;
    user-select: text;
  }
  .acts {
    display: flex;
    gap: 4px;
    margin-top: 3px;
  }
  .x {
    flex: none;
    align-self: flex-start;
    display: inline-flex;
    padding: 2px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .x:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
</style>
