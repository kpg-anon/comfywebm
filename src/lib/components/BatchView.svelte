<script lang="ts">
  // The Batch view. The sidebar's settings apply to every file; each file can
  // be trimmed and cropped (in the editor), get its own frame rate cap, or
  // leave out the sound. Each file is planned with the current settings and
  // checked; issues come with one-click changes.
  import { app } from "$lib/state.svelte";
  import { batch } from "$lib/batch.svelte";
  import { pickFolder, pickVideos } from "$lib/dialogs";
  import { fmtBytes, fmtTime } from "$lib/time";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";

  // plan every file again shortly after the settings or the list change
  $effect(() => {
    void JSON.stringify(batch.specs);
    const t = setTimeout(() => void batch.planAll(), 300);
    return () => clearTimeout(t);
  });

  async function addFiles() {
    const files = await pickVideos();
    if (files.length) await batch.addPaths(files);
  }

  async function addFolder() {
    const d = await pickFolder("Add the videos in a folder");
    if (d) await batch.addFolder(d);
  }

  const fpsOptions = $derived([
    { value: "global", label: `As set (${app.fps ? `${app.fps} fps` : "source"})` },
    ...app.fpsCaps.map((f) => ({ value: String(f), label: `At most ${f} fps` })),
  ]);

  const counts = $derived.by(() => {
    let warn = 0;
    let err = 0;
    for (const i of batch.items) {
      const is = batch.issues(i);
      if (is.some((x) => x.level === "error")) err++;
      else if (is.some((x) => x.level === "warn")) warn++;
    }
    return { warn, err };
  });

  const range = (s: { start: number; end: number }[]) =>
    s.map((r) => `${fmtTime(r.start).replace(/\.000$/, "")}-${fmtTime(r.end).replace(/\.000$/, "")}`).join(", ");
</script>

<div class="batch">
  <div class="head">
    <span class="title head-text">Batch</span>
    <span class="muted small">
      {batch.items.length ? `${batch.included.length} of ${batch.items.length} files` : "empty"}
      {#if counts.warn}· <span class="warn">{counts.warn} to check</span>{/if}
      {#if counts.err}· <span class="err">{counts.err} with errors</span>{/if}
    </span>
    {#if batch.busy}<span class="muted small">{batch.busy}</span>{/if}
    <span class="grow"></span>
    <button class="btn small" onclick={addFiles}><Icon name="plus" size={11} />Add files</button>
    <button class="btn small" onclick={addFolder}><Icon name="open" size={11} />Add folder</button>
    <label class="check small" title="Add folder and dropped folders also take the videos in their subfolders">
      <input type="checkbox" bind:checked={batch.recursive} /> subfolders
    </label>
    <button class="btn small danger" disabled={!batch.items.length} onclick={() => batch.clear()}><Icon name="trash" size={11} />Clear</button>
  </div>

  {#if !batch.items.length}
    <div class="empty">
      <Icon name="stack" size={36} />
      <div class="big">Drop videos or folders here</div>
      <div class="muted">or use Add files / Add folder. The settings on the right apply to every file; each file is checked against them before anything is encoded.</div>
    </div>
  {:else}
    <div class="body">
      {#if batch.suggestions.length}
        <div class="suggest">
          <Icon name="warn" size={14} />
          <div class="slist">
            {#each batch.suggestions as s (s.label)}
              <div class="sline"><span>{s.text}</span><button class="btn small" onclick={s.apply}>{s.label}</button></div>
            {/each}
          </div>
        </div>
      {/if}

      {#each batch.items as it (it.id)}
        {@const issues = batch.issues(it)}
        {@const p = it.plan}
        <div class="item" class:off={!it.include} class:bad={issues.some((x) => x.level === "error")}>
          <div class="top">
            <input type="checkbox" aria-label="Include {it.name}" checked={it.include} disabled={!it.info} onchange={(e) => batch.update(it.id, { include: (e.currentTarget as HTMLInputElement).checked })} />
            <span class="name" title={it.path}>{it.name}</span>
            {#if p && it.include}
              {#if p.quality}<span class="badge {p.quality}">{p.quality}</span>{/if}
            {/if}
            <button class="btn small icon" aria-label="Remove from the batch" onclick={() => batch.remove(it.id)}><Icon name="x" size={11} /></button>
          </div>
          {#if it.info}
            {@const v = it.info.video}
            <div class="meta mono">
              <span>{v.width}x{v.height}</span>
              <span>{Math.round(v.fps * 100) / 100} fps</span>
              <span>{fmtTime(it.info.duration)}</span>
              <span>{fmtBytes(it.info.sizeBytes)}</span>
              {#if p && it.include}
                <span class="arrow">→</span>
                <span class="out">{p.outWidth}x{p.outHeight}</span>
                <span>{fmtTime(p.durationOut)}</span>
                <span>{p.videoKbps !== null ? `${Math.round(p.videoKbps)} kbps` : "CRF"}</span>
              {/if}
            </div>
            <div class="controls">
              <button class="btn small" title="Open this file in the editor to set its ranges and crop; Save to batch brings them back" onclick={() => batch.editInEditor(it.id)}><Icon name="crop" size={11} />Trim / crop</button>
              {#if it.segments.length}
                <span class="chip" title="Only these parts are encoded">{range(it.segments)}<button aria-label="Remove the trim" onclick={() => batch.update(it.id, { segments: [] })}><Icon name="x" size={9} /></button></span>
              {/if}
              {#if it.crop}
                <span class="chip">crop {it.crop.w}x{it.crop.h}<button aria-label="Remove the crop" onclick={() => batch.update(it.id, { crop: null })}><Icon name="x" size={9} /></button></span>
              {/if}
              <Select
                label="Frame rate for this file"
                value={it.fps === undefined ? "global" : String(it.fps)}
                options={fpsOptions}
                onchange={(v) => batch.update(it.id, { fps: v === "global" ? undefined : Number(v) })}
              />
              {#if app.audioOn && it.info.audio.length}
                <label class="check small" title="Leave the sound out of this file only; its bitrate goes to the video">
                  <input type="checkbox" checked={it.audioOff} onchange={(e) => batch.update(it.id, { audioOff: (e.currentTarget as HTMLInputElement).checked })} /> no sound
                </label>
              {/if}
            </div>
          {/if}
          {#if it.include || it.error}
            {#each issues as is, k (k)}
              <div class="issue {is.level}">
                <Icon name={is.level === "info" ? "info" : "warn"} size={11} />
                <span class="itext">{is.text}</span>
                {#each is.fixes as f (f.label)}
                  <button class="btn small" onclick={f.apply}>{f.label}</button>
                {/each}
              </div>
            {/each}
          {/if}
        </div>
      {/each}
    </div>

    <div class="foot">
      <span class="muted small">Files go to {app.outputPath ?? "the folder of each file"}.</span>
      <span class="grow"></span>
      <button
        class="btn"
        disabled={batch.included.length < 2 || app.ffmpegMissing || app.container === "audio" || app.streamCopy}
        title="One file from all included files in list order (each with its trim), at the first file's picture size and frame rate. Other sizes are fitted with black bars."
        onclick={() => batch.join(false)}>Join into one file</button
      >
      <button class="btn" disabled={!batch.included.length || app.ffmpegMissing} onclick={() => batch.enqueue(false)}>Add {batch.included.length} to queue</button>
      <button class="btn primary" disabled={!batch.included.length || app.ffmpegMissing} onclick={() => batch.enqueue(true)}>Encode {batch.included.length} files</button>
    </div>
  {/if}
</div>

<style>
  .batch {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-app);
  }
  .head,
  .foot {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 10px var(--pad-panel);
    background: var(--bg-panel);
  }
  .head {
    border-bottom: 1px solid var(--border-muted);
  }
  .foot {
    border-top: 1px solid var(--border-muted);
  }
  .title {
    font-size: 13px;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 11.5px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    cursor: pointer;
    color: var(--text-secondary);
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    color: var(--text-muted);
    text-align: center;
  }
  .empty .muted {
    max-width: 440px;
    font-size: 12px;
  }
  .big {
    font-size: 16px;
    color: var(--text-secondary);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 10px var(--pad-panel);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .suggest {
    display: flex;
    gap: 9px;
    padding: 9px 11px;
    border: var(--bw) solid color-mix(in srgb, var(--yellow) 40%, transparent);
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--yellow) 8%, var(--bg-panel));
    color: var(--yellow);
  }
  .slist {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: var(--text-primary);
    font-size: 12px;
  }
  .sline {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: space-between;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px 10px;
    border: var(--bw) solid var(--border-muted);
    border-radius: var(--radius);
    background: var(--bg-panel);
  }
  .item.off {
    opacity: 0.55;
  }
  .item.bad {
    border-color: color-mix(in srgb, var(--red) 45%, transparent);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 10px;
    font-size: 11px;
    color: var(--text-secondary);
    padding-left: 22px;
  }
  .arrow {
    color: var(--text-muted);
  }
  .out {
    color: var(--text-primary);
    font-weight: 600;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    padding-left: 22px;
  }
  .controls :global(.sel) {
    width: 150px;
    height: var(--ctl-h-sm);
    font-size: 11px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: var(--ctl-h-sm);
    padding: 0 4px 0 7px;
    border: var(--bw) solid var(--accent-line);
    border-radius: var(--radius-seg);
    background: var(--accent-bg);
    color: var(--accent-strong);
    font-family: var(--mono);
    font-size: 10.5px;
  }
  .chip button {
    display: inline-flex;
    padding: 1px;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
  }
  .issue {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding-left: 22px;
    font-size: 11.5px;
    color: var(--text-secondary);
  }
  .issue.warn {
    color: var(--yellow);
  }
  .issue.error {
    color: var(--red);
  }
  .itext {
    flex: 1 1 200px;
    min-width: 0;
  }
  .badge {
    padding: 0 6px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .badge.good {
    background: color-mix(in srgb, var(--green) 20%, transparent);
    color: var(--green);
  }
  .badge.fair {
    background: color-mix(in srgb, var(--yellow) 20%, transparent);
    color: var(--yellow);
  }
  .badge.poor {
    background: color-mix(in srgb, var(--red) 20%, transparent);
    color: var(--red);
  }
</style>
