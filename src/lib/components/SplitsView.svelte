<script lang="ts">
  // The Splits tab: clips combined into one output, one after another or at
  // the same time. Plain choices on the left, a live preview on the right; the
  // sidebar's settings (preset, size limit, format, caption...) apply to the
  // result.
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { app } from "$lib/state.svelte";
  import { splits, SHAPES, SIZES, type Shape } from "$lib/splits.svelte";
  import { pickVideos } from "$lib/dialogs";
  import { fmtTime } from "$lib/time";
  import * as ipc from "$lib/ipc";
  import type { Rect } from "$lib/types";
  import Icon from "./Icon.svelte";
  import Seg from "./Seg.svelte";
  import Select from "./Select.svelte";
  import TimeField from "./TimeField.svelte";

  // plan again shortly after anything that changes the job
  $effect(() => {
    void JSON.stringify(splits.spec());
    const t = setTimeout(() => void splits.refreshPlan(), 300);
    return () => clearTimeout(t);
  });

  // small pictures of the clips
  let posters = $state<Record<string, string>>({});
  $effect(() => {
    for (const c of splits.clips) {
      if (posters[c.path] !== undefined) continue;
      posters[c.path] = "";
      ipc.poster(c.path).then((p) => (posters[c.path] = convertFileSrc(p))).catch(() => {});
    }
  });

  async function addFiles() {
    const files = await pickVideos();
    if (files.length) await splits.addPaths(files);
  }

  const n = $derived(splits.clips.length);
  const [cw, ch] = $derived(splits.canvas);

  // --- preview ---
  // the boxes of the clips on the canvas: the engine's (plan.cells) once it has
  // planned, an even split until then; one after another: the whole canvas
  const boxes = $derived.by((): Rect[] => {
    if (!splits.split) return [{ x: 0, y: 0, w: cw, h: ch }];
    const fromPlan = splits.plan?.cells ?? [];
    if (fromPlan.length === n) return fromPlan;
    const [cols, rows] = splits.grid;
    return splits.clips.map((_, i) => {
      const c = i % cols;
      const r = Math.floor(i / cols);
      return { x: (c * cw) / cols, y: (r * ch) / rows, w: cw / cols, h: ch / rows };
    });
  });
  let stageW = $state(0);
  let stageH = $state(0);
  const scale = $derived(cw && ch && stageW && stageH ? Math.min(stageW / cw, stageH / ch) : 0);
  let videos = $state<HTMLVideoElement[]>([]);
  let playing = $state(false);
  /** one after another: the clip being shown */
  let current = $state(0);

  const muted = (i: number) => {
    if (!app.audioOn) return true;
    if (!splits.split) return false;
    return splits.sound === "none" || (typeof splits.sound === "number" && splits.sound !== i);
  };

  function stopAll() {
    playing = false;
    for (const v of videos) v?.pause();
  }

  async function play() {
    if (playing) return stopAll();
    playing = true;
    if (!splits.split) {
      current = 0;
      await startClip(0);
      return;
    }
    await Promise.all(
      splits.clips.map((c, i) => {
        const v = videos[i];
        if (!v) return;
        v.currentTime = c.start;
        return v.play().catch(() => {});
      }),
    );
  }

  async function startClip(i: number) {
    const v = videos[0];
    const c = splits.clips[i];
    if (!v || !c) return stopAll();
    current = i;
    // the element's src follows `current`; wait for it to load before seeking
    await new Promise((r) => setTimeout(r, 30));
    v.currentTime = c.start;
    await v.play().catch(() => {});
  }

  // each frame: a clip past its end holds (split, longest) or the whole thing stops / moves on
  function tick(i: number) {
    const v = videos[i];
    if (!v || !playing) return;
    if (!splits.split) {
      const c = splits.clips[current];
      if (c && v.currentTime >= c.end - 0.03) {
        if (current + 1 < n) void startClip(current + 1);
        else stopAll();
      }
      return;
    }
    const c = splits.clips[i];
    const t = v.currentTime - c.start;
    if (t >= splits.length_ - 0.03) {
      stopAll();
      return;
    }
    if (v.currentTime >= c.end - 0.03) v.pause();
  }

  const fitCss = $derived(splits.fill === "crop" ? "cover" : "contain");
  const summary = $derived.by(() => {
    const p = splits.plan;
    if (!p) return null;
    const parts =
      splits.split && p.cells?.length ? `, ${n} parts (${p.cells.map((c) => `${c.w}x${c.h}`).join(", ")} on the ${cw}x${ch} canvas)` : "";
    const snd = !p.audioKbps
      ? "no sound"
      : !splits.split
        ? "each clip's sound"
        : splits.sound === "mix"
          ? "all sounds mixed"
          : `sound from clip ${(splits.sound as number) + 1}`;
    return `${p.outWidth}x${p.outHeight}${parts}, ${splits.fps} fps, ${fmtTime(p.durationOut)}, ${snd}`;
  });

  const shapeOpts = SHAPES.map((s) => ({ value: s.value, label: s.label, title: s.hint }));
  const partOpts = SHAPES.map((s) => ({ value: s.value, label: s.value === "auto" ? "Auto" : s.value, title: s.value === "auto" ? "each part takes its share of the final shape" : s.hint }));
  const soundOpts = $derived([
    ...splits.clips.map((c, i) => ({ value: i as number | "mix" | "none", label: `Clip ${i + 1} (${c.name})` })),
    { value: "mix" as const, label: "All, mixed" },
    { value: "none" as const, label: "None" },
  ]);
</script>

<div class="splits">
  <div class="head">
    <span class="title head-text">Splits</span>
    <span class="muted small">{n ? `${n} clips` : "no clips"}{#if splits.busy} · {splits.busy}{/if}</span>
    <span class="grow"></span>
    <button class="btn small" onclick={addFiles}><Icon name="plus" size={11} />Add clips</button>
    <button class="btn small danger" disabled={!n} onclick={() => splits.clear()}><Icon name="trash" size={11} />Clear</button>
  </div>

  {#if !n}
    <div class="empty">
      <Icon name="stack" size={36} />
      <div class="big">Combine two or more clips into one</div>
      <div class="muted">
        Add clips (or drop them here). They can have different sizes, shapes and frame rates. Play them one after another, or at the same time side by
        side, stacked or in a grid.
      </div>
      <button class="btn primary" onclick={addFiles}><Icon name="plus" size={12} />Add clips</button>
    </div>
  {:else}
    <div class="body">
      <div class="choices">
        <div class="q">How to combine</div>
        <Seg
          label="How to combine"
          bind:value={splits.mode}
          options={[
            { value: "sequence", label: "One after another" },
            { value: "columns", label: "Side by side" },
            { value: "rows", label: "Stacked" },
            { value: "grid", label: "Grid" },
          ]}
        />

        {#if splits.split}
          <div class="q" title="Picking one sets the final shape: two 9:16 parts side by side make a 9:8 picture.">Shape of each part</div>
          <Seg label="Shape of each part" bind:value={splits.partShape} options={partOpts} />
        {/if}
        <div class="q">Final shape</div>
        {#if splits.split && splits.partShape !== "auto"}
          <div class="muted small">Set by the shape of each part: {cw}:{ch} ({(cw / ch).toFixed(2)}).</div>
        {:else}
          <Seg label="Final shape" bind:value={splits.shape} options={shapeOpts as { value: Shape; label: string; title: string }[]} />
        {/if}

        <div class="q">Size</div>
        <Seg
          label="Size"
          bind:value={splits.size}
          options={[
            { value: "auto", label: "Auto", title: "as large as the size limit allows for a clean picture" },
            ...(["small", "medium", "large"] as const).map((s) => ({ value: s, label: SIZES[s].label, title: splits.sizeLabel(s) })),
          ]}
        />
        <div class="muted small">
          {splits.size === "auto" ? `Up to ${splits.sizeLabel("large")}, smaller when the size limit needs it.` : splits.sizeLabel(splits.size)}
        </div>

        <div class="q">Fill each space</div>
        <Seg
          label="Fill each space"
          bind:value={splits.fill}
          options={[
            { value: "bars", label: "Whole picture", title: "the whole picture, with black bars where the shapes differ" },
            { value: "crop", label: "Fill, cut edges", title: "fills the space; the edges that do not fit are cut off" },
            { value: "blur", label: "Blurred background", title: "the whole picture over a blurred, enlarged copy of itself" },
          ]}
        />

        {#if splits.split}
          {#if splits.mode !== "grid" && n > 1}
            <div class="q">Sizes of the parts</div>
            <Seg
              label="Sizes of the parts"
              bind:value={splits.parts}
              options={[
                { value: "fit", label: "Fit each clip", title: "each part as wide (side by side) or tall (stacked) as its clip's shape needs" },
                { value: "equal", label: "Equal" },
                { value: "first", label: "First clip larger" },
              ]}
            />
          {/if}
          <div class="q">Sound from</div>
          <Select label="Sound from" bind:value={splits.sound} options={soundOpts} />
          <div class="q">Length</div>
          <Seg
            label="Length"
            bind:value={splits.length}
            options={[
              { value: "shortest", label: "Shortest clip" },
              { value: "longest", label: "Longest clip", title: "the shorter clips hold their last frame" },
            ]}
          />
          <div class="q">Gap between parts</div>
          <div class="line">
            <Seg
              label="Gap between parts"
              bind:value={splits.gap}
              options={[
                { value: "none", label: "None" },
                { value: "thin", label: "Thin" },
                { value: "thick", label: "Thick" },
              ]}
            />
            {#if splits.gap !== "none"}<input type="color" aria-label="Gap color" bind:value={splits.gapColor} />{/if}
          </div>
        {/if}
      </div>

      <div class="right">
        <div class="stage" bind:clientWidth={stageW} bind:clientHeight={stageH}>
          {#if scale}
            <div class="canvas" style:width="{cw * scale}px" style:height="{ch * scale}px" style:background={splits.split && splits.gap !== "none" ? splits.gapColor : "#000"}>
              {#each boxes as b, i (i)}
                {@const c = splits.split ? splits.clips[i] : splits.clips[current]}
                {#if c}
                  <div class="cell" style:left="{b.x * scale}px" style:top="{b.y * scale}px" style:width="{b.w * scale}px" style:height="{b.h * scale}px">
                    {#if splits.fill === "blur"}
                      <img class="bg" src={posters[c.path] || undefined} alt="" />
                    {/if}
                    <video
                      bind:this={videos[i]}
                      src={convertFileSrc(c.path)}
                      poster={posters[c.path] || undefined}
                      muted={muted(splits.split ? i : current)}
                      preload="auto"
                      playsinline
                      style:object-fit={fitCss}
                      ontimeupdate={() => tick(i)}
                      onloadedmetadata={(e) => ((e.currentTarget as HTMLVideoElement).currentTime = c.start)}
                    ></video>
                  </div>
                {/if}
              {/each}
            </div>
          {/if}
        </div>
        <div class="pbar">
          <button class="btn small" onclick={play} disabled={n < 2}><Icon name={playing ? "pause" : "play"} size={12} />{playing ? "Pause" : "Play"}</button>
          {#if !splits.split && playing}<span class="muted small">Clip {current + 1} of {n}</span>{/if}
          <span class="grow"></span>
          {#if summary}<span class="mono small">{summary}</span>{/if}
        </div>
        {#if splits.planError}<div class="err small">{splits.planError}</div>{/if}
        {#each splits.plan?.warnings ?? [] as w (w)}<div class="warn small"><Icon name="warn" size={11} /> {w}</div>{/each}

        <div class="clips">
          {#each splits.clips as c, i (c.id)}
            <div class="clip">
              <span class="num mono">{i + 1}</span>
              {#if posters[c.path]}<img class="poster" src={posters[c.path]} alt="" draggable="false" />{:else}<span class="poster"></span>{/if}
              <div class="cmain">
                <div class="name" title={c.path}>{c.name}</div>
                <div class="meta mono">
                  {c.info.video.width}x{c.info.video.height} · {Math.round(c.info.video.fps * 100) / 100} fps · {fmtTime(c.info.duration)}{c.info.audio.length ? "" : " · no sound"}
                </div>
              </div>
              <div class="trim">
                <TimeField label="Start of clip {i + 1}" value={c.start} onset={(v) => splits.setTrim(c.id, v, c.end)} />
                <span class="muted">to</span>
                <TimeField label="End of clip {i + 1}" value={c.end} onset={(v) => splits.setTrim(c.id, c.start, v)} />
              </div>
              <button class="btn small icon" aria-label="Move up" disabled={i === 0} onclick={() => splits.move(c.id, -1)}><Icon name="up" size={11} /></button>
              <button class="btn small icon" aria-label="Move down" disabled={i === n - 1} onclick={() => splits.move(c.id, 1)}><span class="down"><Icon name="up" size={11} /></span></button>
              <button class="btn small icon" aria-label="Remove {c.name}" onclick={() => splits.remove(c.id)}><Icon name="x" size={11} /></button>
            </div>
          {/each}
          {#if n === 1}<div class="muted small">Add at least one more clip.</div>{/if}
        </div>
      </div>
    </div>

    <div class="foot">
      <span class="muted small">The sidebar's settings (preset, size limit, format, caption, watermark) apply to the result.</span>
      <span class="grow"></span>
      <button class="btn" disabled={n < 2 || app.ffmpegMissing || !!splits.planError} onclick={() => splits.enqueue(false)}>Add to queue</button>
      <button class="btn primary" disabled={n < 2 || app.ffmpegMissing || !!splits.planError} onclick={() => splits.enqueue(true)}>Encode</button>
    </div>
  {/if}
</div>

<style>
  .splits {
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
    max-width: 460px;
    font-size: 12px;
  }
  .big {
    font-size: 16px;
    color: var(--text-secondary);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 14px;
    padding: 12px var(--pad-panel);
    overflow: hidden;
  }
  .choices {
    flex: none;
    width: 330px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 4px;
  }
  .choices :global(.seg) {
    flex-wrap: wrap;
  }
  .q {
    margin-top: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .line input[type="color"] {
    width: 32px;
    height: 22px;
    padding: 0;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-sm);
    background: none;
  }
  .right {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .stage {
    flex: 1;
    min-height: 200px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-stage);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .canvas {
    position: relative;
    overflow: hidden;
    outline: 1px solid var(--border);
  }
  .cell {
    position: absolute;
    overflow: hidden;
    background: #000;
  }
  .cell video,
  .cell .bg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .cell .bg {
    object-fit: cover;
    filter: blur(14px) brightness(0.9);
    transform: scale(1.1);
  }
  .pbar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .err {
    color: var(--red);
  }
  .warn {
    color: var(--yellow);
  }
  .clips {
    flex: none;
    max-height: 40%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .clip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: var(--bw) solid var(--border-muted);
    border-radius: var(--radius);
    background: var(--bg-panel);
  }
  .num {
    width: 16px;
    color: var(--text-muted);
    font-size: 11px;
    text-align: center;
  }
  .poster {
    width: 64px;
    height: 36px;
    flex: none;
    object-fit: cover;
    border-radius: var(--radius-sm, 3px);
    background: var(--bg-input);
  }
  .cmain {
    flex: 1;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
  }
  .meta {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .trim {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .trim :global(input) {
    width: 92px;
  }
  .down {
    display: inline-flex;
    transform: rotate(180deg);
  }
</style>
