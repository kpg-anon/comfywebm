<script lang="ts">
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { app } from "$lib/state.svelte";
  import { errText } from "$lib/ipc";
  import { fmtBytes, fmtEta } from "$lib/time";
  import type { Job } from "$lib/types";
  import Icon from "./Icon.svelte";

  let copiedId = $state<number | null>(null);
  let errorOpen = $state<Record<number, boolean>>({});

  const pending = $derived(app.jobs.filter((j) => j.status === "pending").length);

  const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const jobName = (j: Job) => baseName(j.output ?? j.spec.outputName ?? j.spec.input);

  const STATUS: Record<Job["status"], string> = {
    pending: "Waiting",
    running: "Encoding",
    done: "Done",
    failed: "Failed",
    canceled: "Canceled",
  };

  async function run(f: () => Promise<unknown>) {
    try {
      await f();
    } catch (e) {
      app.error = errText(e);
    }
  }

  async function copyPath(j: Job) {
    if (!j.output) return;
    await run(() => navigator.clipboard.writeText(j.output!));
    copiedId = j.id;
    setTimeout(() => (copiedId = null), 1200);
  }
</script>

<div class="queue">
  <div class="head">
    <button class="toggle" aria-expanded={app.queueOpen} onclick={() => (app.queueOpen = !app.queueOpen)}>
      <span class="chev" class:open={app.queueOpen}><Icon name="chevron" size={12} /></span>
      Queue
      <span class="muted">{app.jobs.length ? `${app.jobs.length} job${app.jobs.length === 1 ? "" : "s"}` : "empty"}{pending ? `, ${pending} waiting` : ""}</span>
    </button>
    <button class="btn small" disabled={!pending} onclick={() => app.startQueue()}><Icon name="start" size={11} />Start</button>
  </div>

  {#if app.queueOpen}
    <div class="list">
      {#if !app.jobs.length}
        <div class="muted empty">Nothing queued. "Add to queue" collects jobs here; "Start" runs them one at a time.</div>
      {/if}
      {#each app.jobs as j (j.id)}
        <div class="job {j.status}">
          <div class="top">
            <span class="name" title={j.output ?? j.spec.input}>{jobName(j)}</span>
            <span class="status">{STATUS[j.status]}</span>
          </div>

          {#if j.status === "running" || j.status === "pending"}
            <div class="bar"><div style:width="{Math.round((j.progress?.fraction ?? 0) * 100)}%"></div></div>
            <div class="meta mono">
              {#if j.progress}
                <span>{Math.round(j.progress.fraction * 100)}%</span>
                <span>pass {j.progress.pass}/{j.progress.passes}</span>
                {#if j.progress.attempt > 1}<span>attempt {j.progress.attempt}</span>{/if}
                {#if j.progress.speed !== null}<span>{j.progress.speed.toFixed(2)}x</span>{/if}
                {#if j.progress.etaSecs !== null}<span>{fmtEta(j.progress.etaSecs)} left</span>{/if}
              {/if}
              <span class="grow"></span>
              <button class="btn small danger" onclick={() => app.cancelJob(j.id)}><Icon name="stop" size={11} />Cancel</button>
            </div>
          {:else}
            <div class="meta mono">
              {#if j.status === "done" && j.result}
                <span>{fmtBytes(j.result.sizeBytes)}</span>
                {#if j.result.overLimit}<span class="over">over limit</span>{/if}
                <span class="muted">{fmtEta(j.result.elapsedSecs)}</span>
                {#if j.result.attempts > 1}<span class="muted">{j.result.attempts} attempts</span>{/if}
              {/if}
              {#if j.status === "failed" && j.error}
                <button class="btn small" onclick={() => (errorOpen[j.id] = !errorOpen[j.id])}>{errorOpen[j.id] ? "Hide error" : "Show error"}</button>
              {/if}
              <span class="grow"></span>
              {#if j.status === "done" && j.output}
                <button class="btn small" onclick={() => run(() => openPath(j.output!))}><Icon name="play" size={11} />Play</button>
                <button class="btn small" onclick={() => run(() => revealItemInDir(j.output!))}><Icon name="reveal" size={11} />Reveal</button>
                <button class="btn small" onclick={() => copyPath(j)}><Icon name={copiedId === j.id ? "check" : "copy"} size={11} />{copiedId === j.id ? "Copied" : "Copy path"}</button>
              {/if}
              <button class="btn small icon danger" aria-label="Remove from list" onclick={() => app.removeJob(j.id)}><Icon name="x" size={11} /></button>
            </div>
            {#if j.status === "failed" && j.error && errorOpen[j.id]}
              <pre class="error mono">{j.error}</pre>
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .queue {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-top: 1px solid var(--border);
    background: var(--bg-panel-alt);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
  }
  .toggle {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0;
    border: 0;
    background: none;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
    text-align: left;
    cursor: pointer;
  }
  .toggle .muted {
    font-weight: 500;
  }
  .chev {
    display: inline-flex;
    transform: rotate(-90deg);
  }
  .chev.open {
    transform: none;
  }
  .list {
    max-height: 190px;
    overflow-y: auto;
    padding: 0 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .empty {
    font-size: 12px;
    padding: 4px 0;
  }
  .job {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    border: 1px solid var(--border-muted);
    border-radius: var(--radius);
    background: var(--bg-panel);
  }
  .top {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  .status {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .running .status {
    color: var(--accent);
  }
  .done .status {
    color: var(--green);
  }
  .failed .status {
    color: var(--red);
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 10px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .meta .btn {
    font-family: var(--sans);
  }
  .grow {
    flex: 1;
  }
  .over {
    color: var(--orange);
    font-weight: 600;
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
  .pending .bar > div {
    background: var(--text-muted);
  }
  .error {
    margin: 0;
    max-height: 120px;
    overflow: auto;
    padding: 6px;
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    color: var(--red);
    font-size: 11px;
    white-space: pre-wrap;
    user-select: text;
  }
</style>
