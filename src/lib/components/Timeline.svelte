<script lang="ts">
  import { untrack } from "svelte";
  import { app } from "$lib/state.svelte";
  import { fmtTime } from "$lib/time";
  import Icon from "./Icon.svelte";
  import TimeField from "./TimeField.svelte";
  import CropBar from "./CropBar.svelte";
  import EditStrip from "./EditStrip.svelte";
  import SpeedLane from "./SpeedLane.svelte";
  import Select from "./Select.svelte";

  import { convertFileSrc } from "@tauri-apps/api/core";
  import * as ipc from "$lib/ipc";

  let track = $state<HTMLDivElement>() as HTMLDivElement;

  // Filmstrip and waveform behind the track. Made in the background after a
  // file opens (cached on disk); a newer file or track replaces the request.
  let filmUrl = $state<string | null>(null);
  // Slots across the whole (zoomed) strip at the picture's aspect ratio, as
  // high as the track; each shows the sprite tile nearest its middle.
  const THUMBS = 60;
  const film = $derived.by(() => {
    const m = app.media;
    if (!m || m.audioOnly || !trackW || !m.video.height) return null;
    const slotW = (app.trackH * m.video.width) / m.video.height;
    const count = Math.max(1, Math.min(400, Math.round((trackW * zoom) / Math.max(12, slotW))));
    return {
      n: THUMBS,
      slots: Array.from({ length: count }, (_, i) => i),
      pick: (k: number) => Math.min(THUMBS - 1, Math.floor(((k + 0.5) / count) * THUMBS)),
    };
  });
  let waveUrl = $state<string | null>(null);
  $effect(() => {
    const path = app.media?.path;
    filmUrl = null;
    if (!path || app.audioOnly || !app.showFilm || app.ffmpegMissing) return;
    let live = true;
    ipc.timelineThumbs(path).then((p) => live && (filmUrl = convertFileSrc(p))).catch(() => {});
    return () => (live = false);
  });
  $effect(() => {
    const path = app.media?.path;
    const hasAudio = !!app.media?.audio.length;
    const t = app.audioTrack;
    waveUrl = null;
    if (!path || !hasAudio || !app.showWave || app.ffmpegMissing) return;
    let live = true;
    ipc.timelineWave(path, t).then((p) => live && (waveUrl = convertFileSrc(p))).catch(() => {});
    return () => (live = false);
  });
  let drag = $state<
    | { kind: "seek" }
    | { kind: "start" | "end"; i: number; snaps: number[] }
    // a range's body: a click seeks; past a few pixels the drag moves the range, keeping its length
    | { kind: "move"; i: number; x0: number; t0: number; start0: number; moved: boolean; snaps: number[] }
    | null
  >(null);
  /** Snap distance in pixels for range edges; Alt while dragging turns snapping off. */
  const SNAP_PX = 7;
  let trackW = $state(0);
  /** Time under the pointer while it is over the track. */
  let hoverT = $state<number | null>(null);

  const dur = $derived(app.duration || 0);
  const many = $derived(app.segments.length > 1);

  // Zoom: the track shows [viewStart, viewStart + viewLen]. 1 = the whole file.
  // The deepest zoom shows about 20 frames (at least 0.4 s).
  let zoom = $state(1);
  let viewStart = $state(0);
  const viewLen = $derived(dur > 0 ? dur / zoom : 0);
  const maxZoom = $derived(dur > 0 ? Math.max(1, dur / Math.max(0.4, 20 / app.fpsSrc)) : 1);
  const zoomed = $derived(zoom > 1.001);

  const clampStart = (s: number, len = viewLen) => Math.min(Math.max(0, s), Math.max(0, dur - len));
  /** Position of time t on the track in percent (outside 0..100 when off view). */
  const pct = (t: number) => (viewLen > 0 ? ((Math.min(Math.max(t, 0), dur) - viewStart) / viewLen) * 100 : 0);

  function timeAt(e: { clientX: number }) {
    const b = track.getBoundingClientRect();
    const f = b.width > 0 ? (e.clientX - b.left) / b.width : 0;
    return Math.min(Math.max(viewStart + Math.min(Math.max(f, 0), 1) * viewLen, 0), dur);
  }

  /** Zoom to z, keeping time `at` under the same spot (fraction `frac` of the width). */
  function setZoom(z: number, at = app.current, frac = viewLen > 0 ? (at - viewStart) / viewLen : 0.5) {
    const nz = Math.min(maxZoom, Math.max(1, z));
    const len = dur / nz;
    zoom = nz;
    viewStart = clampStart(at - Math.min(Math.max(frac, 0), 1) * len, len);
  }

  // a new file starts unzoomed
  $effect(() => {
    void app.media?.path;
    zoom = 1;
    viewStart = 0;
  });

  // keep the playhead in view: page forward or back when it leaves
  $effect(() => {
    const t = app.current;
    if (!zoomed || drag) return;
    const s = untrack(() => viewStart);
    if (t < s || t > s + viewLen) viewStart = clampStart(t - viewLen * 0.1);
  });

  // Ctrl+wheel zooms at the pointer; the wheel alone pans while zoomed.
  // Added by hand because the listener must be able to call preventDefault.
  $effect(() => {
    const el = track;
    const onWheel = (e: WheelEvent) => {
      if (!app.media) return;
      const d = e.deltaMode === 1 ? e.deltaY * 40 : e.deltaY;
      if (e.ctrlKey) {
        e.preventDefault();
        const b = el.getBoundingClientRect();
        const frac = b.width > 0 ? (e.clientX - b.left) / b.width : 0.5;
        setZoom(zoom * Math.pow(1.0015, -d), timeAt(e), frac);
      } else if (zoomed) {
        e.preventDefault();
        const dx = Math.abs(e.deltaX) > Math.abs(d) ? e.deltaX : d;
        viewStart = clampStart(viewStart + (dx / 600) * viewLen);
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  // ruler: about one label per 90 px
  const STEPS = [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];
  const ticks = $derived.by(() => {
    if (!viewLen || !trackW) return [];
    const want = viewLen / Math.max(1, trackW / 90);
    const step = STEPS.find((s) => s >= want) ?? 7200;
    const out: { t: number; label: string }[] = [];
    for (let t = Math.ceil(viewStart / step) * step; t <= viewStart + viewLen + 1e-9; t += step) out.push({ t, label: label(t, step) });
    return out;
  });

  // five small ticks per labelled step
  const minorTicks = $derived.by(() => {
    if (!viewLen || !trackW || ticks.length < 2) return [];
    const step = (ticks[1].t - ticks[0].t) / 5;
    const out: number[] = [];
    for (let t = Math.ceil(viewStart / step) * step; t <= viewStart + viewLen + 1e-9; t += step) {
      if (!ticks.some((k) => Math.abs(k.t - t) < step / 2)) out.push(t);
    }
    return out;
  });

  function label(t: number, step: number) {
    const s = fmtTime(t);
    // fmtTime gives m:ss.mmm; keep tenths only when the step needs them
    return step < 1 ? s.replace(/(\.\d)\d\d$/, "$1") : s.replace(/\.\d+$/, "");
  }

  // overview bar (shown while zoomed): click or drag to move the view
  let ovDrag = false;
  function ovMove(e: PointerEvent) {
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const t = ((e.clientX - b.left) / Math.max(1, b.width)) * dur;
    viewStart = clampStart(t - viewLen / 2);
  }

  function down(e: PointerEvent, kind: "seek" | "start" | "end" | "body", i = -1) {
    if (!app.media || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    track.setPointerCapture(e.pointerId);
    if (kind === "body") {
      app.sel = i;
      const t = timeAt(e);
      app.seek(t);
      const snaps = [0, dur, ...(app.cuts ?? [])];
      app.segments.forEach((g, k) => k !== i && snaps.push(g.start, g.end));
      drag = { kind: "move", i, x0: e.clientX, t0: t, start0: app.segments[i].start, moved: false, snaps };
    } else if (kind === "seek") {
      drag = { kind: "seek" };
      app.seek(timeAt(e));
    } else {
      app.sel = i;
      // targets: the playhead where the drag began, the other ranges' edges, the file's ends
      const snaps = [0, dur, app.current, ...(app.cuts ?? [])];
      app.segments.forEach((g, k) => k !== i && snaps.push(g.start, g.end));
      drag = { kind, i, snaps };
    }
  }

  function moveEv(e: PointerEvent) {
    if (app.media) hoverT = timeAt(e);
    if (!drag) return;
    let t = timeAt(e);
    if (drag.kind === "seek") app.seek(t);
    else if (drag.kind === "move") {
      if (!drag.moved) {
        if (Math.abs(e.clientX - drag.x0) < 3) return;
        drag.moved = true;
      }
      const s0 = app.segments[drag.i];
      if (!s0) return;
      const len = s0.end - s0.start;
      let start = drag.start0 + (t - drag.t0);
      // either end snaps to the other ranges' edges, scene cuts and the file's ends (Alt: no snapping)
      if (!e.altKey && trackW > 0) {
        let best = (SNAP_PX / trackW) * viewLen;
        for (const p of drag.snaps) {
          for (const [edge, off] of [[start, 0], [start + len, len]] as const) {
            if (Math.abs(p - edge) < best) {
              best = Math.abs(p - edge);
              start = p - off;
            }
          }
        }
      }
      app.moveRange(drag.i, start);
      // the picture shows the range's first frame
      const s = app.segments[drag.i];
      if (s) app.seek(s.start);
    } else {
      if (!e.altKey && trackW > 0) {
        const reach = (SNAP_PX / trackW) * viewLen;
        let best = reach;
        for (const s of drag.snaps) {
          if (Math.abs(s - t) < best) {
            best = Math.abs(s - t);
            t = s;
          }
        }
      }
      app.setEdge(drag.i, drag.kind, t);
      const s = app.segments[drag.i];
      if (s) app.seek(drag.kind === "start" ? s.start : s.end);
    }
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    track.releasePointerCapture(e.pointerId);
    drag = null;
  }
</script>

<div class="timeline">
  <section class="panel tlp">
  <header class="ph tlhead">
    <Icon name="film" size={14} /><span>Timeline</span>
    <span class="hgrow"></span>
        <div class="group zoom">
          <button class="btn icon" disabled={!app.media || !zoomed} aria-label="Zoom out (Ctrl+wheel)" onclick={() => setZoom(zoom / 2)}><Icon name="zoomOut" size={15} /></button>
          <button class="btn icon" disabled={!app.selSeg} aria-label="Zoom to the selected range" onclick={() => {
            const g = app.selSeg;
            if (!g || g.end <= g.start) return;
            const pad = (g.end - g.start) * 0.08;
            setZoom(dur / (g.end - g.start + 2 * pad), g.start - pad, 0);
          }}><Icon name="fitSel" size={15} /></button>
          <button class="btn zl mono" disabled={!app.media || !zoomed} aria-label="Show the whole file" onclick={() => setZoom(1)}>{zoomed ? `${zoom < 10 ? zoom.toFixed(1) : Math.round(zoom)}x` : "Fit"}</button>
          <button class="btn icon" disabled={!app.media || zoom >= maxZoom} aria-label="Zoom in (Ctrl+wheel)" onclick={() => setZoom(zoom * 2)}><Icon name="zoomIn" size={15} /></button>
        </div>
  </header>
  {#if filmUrl && hoverT !== null && !drag && app.media && dur > 0}
    {@const ph = 96}
    {@const pw = Math.round((ph * app.media.video.width) / Math.max(1, app.media.video.height))}
    {@const k = Math.min(THUMBS - 1, Math.max(0, Math.floor((hoverT / dur) * THUMBS)))}
    {@const x = (track?.offsetLeft ?? 0) + (pct(hoverT) / 100) * trackW}
    <div
      class="peek"
      style:left="{Math.min(Math.max(x - pw / 2, 4), (track?.offsetLeft ?? 0) + trackW - pw)}px"
      style:width="{pw}px"
      style:height="{ph}px"
      style:background-image="url('{filmUrl}')"
      style:background-size="{THUMBS * pw}px {ph}px"
      style:background-position="{-k * pw}px 0"
    ></div>
  {/if}
  <div class="ruler" aria-hidden="true">
    {#each minorTicks as t (t)}
      <span class="tick minor" style:left="{pct(t)}%"></span>
    {/each}
    {#each ticks as k (k.t)}
      <span class="tick" style:left="{pct(k.t)}%">{#if pct(k.t) < 97}<span class="tl mono" class:first={pct(k.t) < 3}>{k.label}</span>{/if}</span>
    {/each}
    {#if hoverT !== null}
      <span class="hover mono" style:left="{Math.min(94, Math.max(4, pct(hoverT)))}%">{fmtTime(hoverT)}</span>
    {/if}
  </div>
  <div
    class="track"
    style:height="{app.trackH}px"
    class:disabled={!app.media}
    class:zoomed
    bind:this={track}
    bind:clientWidth={trackW}
    role="presentation"
    onpointerdown={(e) => down(e, "seek")}
    onpointermove={moveEv}
    onpointerup={up}
    onpointercancel={up}
    onpointerleave={() => (hoverT = null)}
  >
    {#if filmUrl || waveUrl}
      <div class="clip">
        <div class="pics" style:left="{-(viewStart / (viewLen || 1)) * 100}%" style:width="{zoom * 100}%">
          {#if filmUrl && film}
            {#each film.slots as k (k)}
              <span
                class="film"
                style:left="{(k / film.slots.length) * 100}%"
                style:width="{100 / film.slots.length}%"
                style:background-image="url('{filmUrl}')"
                style:background-size="{film.n * 100}% 100%"
                style:background-position-x="{film.n > 1 ? (film.pick(k) / (film.n - 1)) * 100 : 0}%"
              ></span>
            {/each}
          {/if}
          {#if waveUrl}<div class="wave" style:mask-image="url('{waveUrl}')" style:-webkit-mask-image="url('{waveUrl}')"></div>{/if}
        </div>
      </div>
    {/if}
    <div class="rail" class:faint={filmUrl || waveUrl}></div>
    {#if app.cuts}
      {#each app.cuts as c (c)}
        {#if pct(c) >= 0 && pct(c) <= 100}<span class="cut" style:left="{pct(c)}%"></span>{/if}
      {/each}
    {/if}
    {#each app.segments as s, i (i)}
      <div
        class="sel"
        class:active={i === app.sel}
        style:left="{pct(s.start)}%"
        style:width="{pct(s.end) - pct(s.start)}%"
        role="presentation"
        onpointerdown={(e) => down(e, "body", i)}
      >
        {#if many}<span class="num">{i + 1}</span>{/if}
      </div>
      <div class="handle in" class:active={i === app.sel} style:left="{pct(s.start)}%" role="presentation" onpointerdown={(e) => down(e, "start", i)}></div>
      <div class="handle out" class:active={i === app.sel} style:left="{pct(s.end)}%" role="presentation" onpointerdown={(e) => down(e, "end", i)}></div>
    {/each}
    <div class="head" style:left="{pct(app.current)}%"></div>
  </div>

  {#if app.media && !app.audioOnly && app.rampLane}
    <SpeedLane {viewStart} {viewLen} />
  {/if}

  {#if zoomed}
    <div
      class="overview"
      role="scrollbar"
      aria-controls="timeline"
      aria-valuenow={Math.round(viewStart)}
      tabindex="-1"
      onpointerdown={(e) => {
        ovDrag = true;
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        ovMove(e);
      }}
      onpointermove={(e) => ovDrag && ovMove(e)}
      onpointerup={() => (ovDrag = false)}
    >
      {#each app.segments as s, i (i)}
        <span class="ovseg" style:left="{(s.start / dur) * 100}%" style:width="{((s.end - s.start) / dur) * 100}%"></span>
      {/each}
      <span class="ovwin" style:left="{(viewStart / dur) * 100}%" style:width="{(viewLen / dur) * 100}%"></span>
      <span class="ovhead" style:left="{(app.current / dur) * 100}%"></span>
    </div>
  {/if}

  {#if many}
    <EditStrip />
  {/if}
  </section>

  <!-- the controls: playback, the range, the video tools -->
  <section class="panel ctl">
    <div class="sec play">
      <header class="ph"><Icon name="playLine" size={14} /><span>Playback</span></header>
      <div class="bigtime mono" title="The playhead">{app.media ? fmtTime(app.current) : "0:00.000"}</div>
      <div class="transport">
        <button class="btn icon" disabled={!app.media} onclick={() => app.seek(app.selSeg ? app.selSeg.start : 0)} aria-label="To the start of the range (Shift+I), or of the file"><Icon name="prevFrame" /></button>
        <button class="btn icon" disabled={!app.media || app.audioOnly} onclick={() => app.stepFrame(-1)} aria-label="One frame back (,)"><Icon name="stepBack" /></button>
        <button class="btn icon playbtn" disabled={!app.media} onclick={() => app.togglePlay()} aria-label="Play or pause (Space)"><Icon name={app.paused ? "play" : "pause"} /></button>
        <button class="btn icon" disabled={!app.media || app.audioOnly} onclick={() => app.stepFrame(1)} aria-label="One frame forward (.)"><Icon name="stepFwd" /></button>
        <button class="btn icon" disabled={!app.media} onclick={() => app.seek(app.selSeg ? app.selSeg.end : app.duration)} aria-label="To the end of the range (Shift+O), or of the file"><Icon name="nextFrame" /></button>
      </div>
    </div>
    <div class="sec rng">
      <header class="ph"><Icon name="range" size={14} /><span>Range</span></header>
      <div class="fieldrow">
        <div class="times">
          <TimeField label="Time" value={app.media ? app.current : null} onset={(t) => app.seek(t)} />
          <TimeField label={many ? `In (range ${app.sel + 1})` : "In"} value={app.selSeg?.start ?? null} placeholder="start" onset={(t) => app.setSelStart(t)} />
          <TimeField label="Out" value={app.selSeg?.end ?? null} placeholder="end" onset={(t) => app.setSelEnd(t)} />
          <TimeField label="Length" value={app.media ? (app.selSeg ? app.selSeg.end - app.selSeg.start : app.duration) : null} onset={(t) => app.setSelLength(t)} />
        </div>
        {#if many}
          <div class="total">
            <span class="lbl">Total</span>
            <span class="mono">{fmtTime(app.clipLen)}</span>
          </div>
        {/if}
      </div>
        <div class="btnrow">
          <button class="btn" disabled={!app.media} onclick={() => app.markIn()}><Icon name="markIn" size={14} />In <kbd>I</kbd></button>
          <button class="btn" disabled={!app.media} onclick={() => app.markOut()}><Icon name="markOut" size={14} />Out <kbd>O</kbd></button>
          <button class="btn" disabled={!app.media} onclick={() => app.addRange()}><Icon name="plus" size={14} />Range <kbd>N</kbd></button>
          <button class="btn" disabled={app.sel < 0} onclick={() => app.removeRange()} aria-label="Remove the selected range">
            <Icon name="x" size={14} />Remove{#if many}&nbsp;<kbd>Del</kbd>{/if}
          </button>
          {#if many}
            <label class="sep"><input type="checkbox" bind:checked={app.separate} /> Separate files</label>
          {/if}
        </div>
    </div>
    <div class="sec tl-tools">
      <header class="ph"><Icon name="stack" size={14} /><span>Video tools</span></header>
        <div class="tools">
          <button
            class="btn"
            class:on={app.rampLane}
            disabled={!app.media || app.audioOnly}
            aria-pressed={app.rampLane}
            title="Speed ramp: change the speed over the clip with points on a lane under the timeline (0.25x to 4x). With points it replaces the Speed setting."
            onclick={() => (app.rampLane = !app.rampLane)}
          >
            <Icon name="speed" size={14} />Speed{#if app.speedRamp.length}<span class="cnt">{app.speedRamp.length}</span>{/if}
          </button>
          <button class="btn" class:on={app.cropMode} disabled={!app.media || app.audioOnly} aria-pressed={app.cropMode} onclick={() => (app.cropMode = !app.cropMode)}>
            <Icon name="crop" size={14} />Crop <kbd>C</kbd>
          </button>
          <button class="btn" disabled={!app.media || app.audioOnly || app.ffmpegMissing || app.findingCuts} title="Find where the picture cuts to a new shot (also jump cuts, and fades through black; flashes and lights are not cuts). Range edges then snap to the cuts, and [ and ] jump between them. Reads the whole video once." onclick={() => app.findCuts()}><Icon name="cut" size={14} />{app.findingCuts ? `Finding ${Math.round(app.cutsFraction * 100)}%` : app.cuts ? `${app.cuts.length} cuts` : "Find cuts"}</button>
          {#if app.cuts !== null}
            <div class="sens" title="Fewer: only clear cuts. More: also cuts between similar shots. Changing it needs no second read.">
              <Select
                label="Cut sensitivity"
                value={app.cutSensitivity}
                onchange={(v) => app.setCutSensitivity(v)}
                options={[
                  { value: "low", label: "Fewer cuts" },
                  { value: "normal", label: "Normal" },
                  { value: "high", label: "More cuts" },
                ]}
              />
            </div>
          {/if}
          <button class="btn" disabled={!app.media || app.ffmpegMissing} onclick={() => app.runSample()}><Icon name="film" size={14} />Sample <kbd>P</kbd></button>
          <button class="btn" disabled={!app.media || app.ffmpegMissing} title="Save the frame at the playhead as PNG with the crop. Shift+click (Shift+S): as the encode sees it, with filters, adjustments and the output size." onclick={(e) => app.exportFrame(e.shiftKey)}><Icon name="image" size={14} />Frame <kbd>S</kbd></button>
        </div>
    </div>
  </section>

  {#if app.cropMode}
    <!-- the crop as shown: turned and flipped like the picture -->
    <CropBar
      aspect={app.aspect}
      crop={app.viewCrop}
      W={app.viewW}
      H={app.viewH}
      onaspect={(a) => app.setAspect(a)}
      oncrop={(c) => app.setViewCrop(c)}
      ondone={() => (app.cropMode = false)}
    />
  {/if}
</div>

<style>
  .cnt {
    margin-left: 2px;
    padding: 0 5px;
    border-radius: 8px;
    background: var(--accent-bg);
    color: var(--accent-strong);
    font-size: 10px;
  }
  /* two panels: the timeline (header with its zoom, ruler, track, lanes) and the controls
     (playback, range, video tools), after the owner's mockup */
  .timeline {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px var(--pad-panel) 10px;
    border-top: 1px solid var(--border-muted);
    background: var(--bg-app);
  }
  .panel {
    position: relative;
    border: 1px solid var(--border-muted);
    border-radius: var(--radius, 8px);
    background: var(--bg-panel);
  }
  .tlp {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 12px 10px;
  }
  .ph {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-secondary);
    font-size: 11.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .tlhead {
    margin: 0 -12px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-muted);
  }
  .hgrow {
    flex: 1;
  }
  .ctl {
    display: flex;
    flex-wrap: wrap;
    align-items: stretch;
    padding: 10px 0;
  }
  .sec {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0 16px;
  }
  .sec + .sec {
    border-left: 1px solid var(--border-muted);
  }
  .sec .ph {
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border-muted);
  }
  .play {
    flex: 0 0 auto;
  }
  .rng {
    flex: 1 1 auto;
  }
  .tl-tools {
    flex: 0 1 auto;
  }
  .bigtime {
    padding: 4px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    color: var(--text-primary);
    font-size: 21px;
    text-align: center;
    letter-spacing: 0.02em;
  }
  .transport {
    display: flex;
    gap: 5px;
    justify-content: center;
  }
  .transport :global(.btn.icon) {
    width: 32px;
    height: 30px;
  }
  .transport .playbtn {
    width: 42px !important;
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .fieldrow {
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }
  .btnrow {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .tools {
    display: grid;
    grid-template-columns: repeat(3, auto);
    gap: 6px;
    align-items: center;
  }
  .track {
    position: relative;
    height: 26px;
    cursor: pointer;
    touch-action: none;
  }
  .track.zoomed {
    overflow: hidden;
  }
  .ruler {
    position: relative;
    height: 22px;
    margin-bottom: -2px;
    overflow: hidden;
    pointer-events: none;
  }
  .tick {
    position: absolute;
    bottom: 0;
    width: 1px;
    height: 7px;
    background: var(--text-muted);
    opacity: 0.7;
  }
  /* the label at the very left is not centred on its tick (it would be cut off) */
  .tl.first {
    transform: none;
  }
  .tick.minor {
    height: 4px;
    opacity: 0.4;
  }
  .tl {
    position: absolute;
    bottom: 9px;
    left: 0;
    transform: translateX(-50%);
    font-size: 10px;
    line-height: 1;
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .hover {
    position: absolute;
    top: 0;
    z-index: 1;
    transform: translateX(-50%);
    padding: 0 4px;
    border-radius: 3px;
    background: var(--bg-chip);
    box-shadow: 0 0 0 1px var(--border);
    color: var(--text-primary);
    font-size: 10px;
    line-height: 13px;
    white-space: nowrap;
  }
  .overview {
    position: relative;
    height: 8px;
    margin-top: -3px;
    border-radius: 3px;
    background: var(--bg-input);
    cursor: pointer;
    touch-action: none;
  }
  .ovseg {
    position: absolute;
    top: 2px;
    height: 4px;
    background: color-mix(in srgb, var(--range) 60%, transparent);
  }
  .ovwin {
    position: absolute;
    top: 0;
    height: 8px;
    border: 1px solid var(--accent);
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .ovhead {
    position: absolute;
    top: 0;
    width: 2px;
    height: 8px;
    margin-left: -1px;
    background: var(--playhead);
  }
  .zoom {
    gap: 2px;
  }
  .zl {
    min-width: 46px;
    padding: 0 6px;
    font-size: 11px;
  }
  .track.disabled {
    cursor: default;
    opacity: 0.5;
  }
  .timeline {
    position: relative;
  }
  .peek {
    position: absolute;
    bottom: calc(100% + 6px);
    z-index: 5;
    border-radius: var(--radius-sm, 4px);
    box-shadow: 0 0 0 1px var(--border), 0 4px 14px rgba(0, 0, 0, 0.5);
    background-repeat: no-repeat;
    pointer-events: none;
  }
  .cut {
    position: absolute;
    top: 2px;
    bottom: 2px;
    width: 2px;
    margin-left: -1px;
    background: var(--orange, #e8a33d);
    opacity: 0.85;
    pointer-events: none;
  }
  .clip {
    position: absolute;
    inset: 0;
    overflow: hidden;
    border-radius: 3px;
    pointer-events: none;
  }
  .pics {
    position: absolute;
    top: 0;
    bottom: 0;
    pointer-events: none;
  }
  .film {
    position: absolute;
    top: 0;
    bottom: 0;
    background-repeat: no-repeat;
    opacity: 0.45;
  }
  .wave {
    position: absolute;
    inset: 0;
    background: var(--accent);
    opacity: 0.55;
    mask-size: 100% 100%;
    -webkit-mask-size: 100% 100%;
  }
  .rail.faint {
    opacity: 0.35;
  }
  .rail {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(50% - 3px);
    height: 6px;
    border-radius: 3px;
    background: var(--border);
  }
  /* the cut sensitivity beside Find cuts */
  .sens {
    width: 112px;
  }
  .sel {
    position: absolute;
    top: 0;
    bottom: 0;
    cursor: grab;
    border-radius: 4px;
    background: color-mix(in srgb, var(--range) 12%, transparent);
    border: 2px solid color-mix(in srgb, var(--range) 50%, transparent);
    overflow: hidden;
  }
  .sel.active {
    background: color-mix(in srgb, var(--range) 22%, transparent);
    border-color: var(--range);
  }
  .num {
    position: absolute;
    left: 6px;
    top: -1px;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-primary);
    pointer-events: none;
  }
  /* the range's ends: a wider grip on its border */
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 10px;
    margin-left: -5px;
    cursor: ew-resize;
  }
  .handle::after {
    content: "";
    position: absolute;
    left: 3px;
    top: 0;
    bottom: 0;
    width: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--range) 55%, transparent);
  }
  .handle.active::after {
    background: var(--range);
  }
  .handle.in {
    border-radius: 3px 0 0 3px;
  }
  .handle.out {
    border-radius: 0 3px 3px 0;
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--playhead);
    pointer-events: none;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.35);
  }
  /* a marker on top of the playhead */
  .head::before {
    content: "";
    position: absolute;
    top: -1px;
    left: -5px;
    border-left: 6px solid transparent;
    border-right: 6px solid transparent;
    border-top: 8px solid var(--playhead);
  }
  .group {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .times {
    display: grid;
    grid-template-columns: repeat(4, 104px);
    gap: 8px;
  }
  .total {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: 4px;
    font-size: 12px;
  }
  .lbl {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
  }
  .sep {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 4px;
    font-size: 12px;
    cursor: pointer;
  }
  .btn kbd {
    font-size: 10px;
    padding: 0 3px;
    color: var(--text-muted);
  }
</style>
