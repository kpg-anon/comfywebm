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
  const finished = $derived(app.jobs.filter((j) => j.status !== "pending" && j.status !== "running").length);

  const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const jobName = (j: Job) => baseName(j.output ?? j.spec.outputName ?? j.spec.input);

  const STATUS: Record<Job["status"], string> = {
    pending: "Waiting",
    running: "Encoding",
    done: "Done",
    failed: "Failed",
    canceled: "Canceled",
  };

  /** Short line of what the job makes: codec, limit or quality, and the cut. */
  function describe(j: Job): string {
    const s = j.spec;
    const rate = s.rate.mode === "size" ? `${(s.rate.maxBytes / 1048576).toFixed(s.rate.maxBytes % 1048576 ? 1 : 0)} MiB` : `CRF ${s.rate.crf}`;
    const len = s.segments.reduce((a, g) => a + (g.end - g.start), 0);
    const cut = s.segments.length ? `${s.segments.length > 1 ? `${s.segments.length} ranges, ` : ""}${fmtEta(len / (s.speed || 1))}` : "whole file";
    return `${s.codec.toUpperCase()} Â· ${rate} Â· ${cut}`;
  }

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
    <span class="title head-text">Queue</span>
    <span class="muted count">
      {app.jobs.length ? `${app.jobs.length} job${app.jobs.length === 1 ? "" : "s"}` : "empty"}{pending ? `, ${pending} waiting` : ""}
    </span>
    <span class="grow"></span>
    <button class="btn small" disabled={!finished} onclick={() => app.clearFinished()}><Icon name="trash" size={11} />Clear finished</button>
    {#if app.queueStarted && pending}
      <button class="btn small" title="Finish the running job, then stop" onclick={() => app.pauseQueue()}><Icon name="pause" size={11} />Pause after this</button>
    {:else}
      <button class="btn small primary" disabled={!pending} onclick={() => app.startQueue()}><Icon name="start" size={11} />Start</button>
    {/if}
  </div>

    <div class="list">
      {#if !app.jobs.length}
        <div class="muted empty">Nothing queued. "Add to queue" collects jobs here and "Start" runs them one at a time; "Encode" adds a job and starts it.</div>
      {/if}
      {#each app.jobs as j, i (j.id)}
        <div class="job {j.status}">
          <div class="top">
            <span class="name" title={j.output ?? j.spec.input}>{jobName(j)}</span>
            <span class="muted desc mono">{describe(j)}</span>
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
              {#if j.status === "pending"}
                <button class="btn small icon" aria-label="Move up" disabled={i === 0} onclick={() => app.moveJob(j.id, -1)}><Icon name="up" size={11} /></button>
                <button class="btn small icon" aria-label="Move down" disabled={i === app.jobs.length - 1} onclick={() => app.moveJob(j.id, 1)}><Icon name="chevron" size={11} /></button>
              {/if}
              <button class="btn small danger" onclick={() => app.cancelJob(j.id)}><Icon name="stop" size={11} />{j.status === "pending" ? "Remove" : "Cancel"}</button>
            </div>
          {:else}
            <div class="meta mono">
              {#if j.status === "done" && j.result}
                <span>{fmtBytes(j.result.sizeBytes)}</span>
                {#if j.result.overLimit}<span class="over">over limit</span>{/if}
                {#if j.result.warnings?.length}<span class="over" title={j.result.warnings.join("\n")}>check output</span>{/if}
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
              <button class="btn small" title="Put this job back in the queue with the same settings" onclick={() => app.retryJob(j.id)}><Icon name="retry" size={11} />{j.status === "done" ? "Encode again" : "Retry"}</button>
              <button class="btn small icon danger" aria-label="Remove from list" onclick={() => app.removeJob(j.id)}><Icon name="x" size={11} /></button>
            </div>
            {#if j.status === "failed" && j.error && errorOpen[j.id]}
              <pre class="error mono">{j.error}</pre>
            {/if}
            {#if j.status === "done" && j.result?.warnings?.length}
              <div class="warns">{#each j.result.warnings as w (w)}<div>{w}</div>{/each}</div>
            {/if}
          {/if}
        </div>
      {/each}
    </div>
</div>

<style>
  .queue {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-app);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px var(--pad-panel);
    border-bottom: 1px solid var(--border-muted);
    background: var(--bg-panel);
  }
  .title {
    font-size: 13px;
  }
  .count {
    font-size: 12px;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 10px var(--pad-panel);
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
    padding: 8px 10px;
    border: var(--bw) solid var(--border-muted);
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
  .desc {
    font-size: 11px;
    white-space: nowrap;
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
  .warns {
    font-size: 11px;
    color: var(--orange);
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
