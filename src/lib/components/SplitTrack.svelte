<script lang="ts">
  // One clip's trim track in Splits: the whole file with a filmstrip, the used
  // range with handles at its ends, and where the preview is in the file.
  // Drag an end to trim, drag the range to move it (same length), click in it
  // to show that moment. Ends snap to the length of the other clips and to the
  // file's ends; Alt while dragging turns snapping off.
  import { convertFileSrc } from "@tauri-apps/api/core";
  import * as ipc from "$lib/ipc";
  import { app } from "$lib/state.svelte";
  import { splits, type SplitClip } from "$lib/splits.svelte";
  import { fmtTime } from "$lib/time";

  let { c, i }: { c: SplitClip; i: number } = $props();

  const THUMBS = 60;
  const SNAP_PX = 7;
  let track = $state<HTMLDivElement>();
  let trackW = $state(0);
  let film = $state<string | null>(null);
  let hoverT = $state<number | null>(null);

  $effect(() => {
    const path = c.path;
    film = null;
    if (app.ffmpegMissing) return;
    let live = true;
    // key frames only: the clip rows are small, and an exact strip per clip would decode every file
    ipc.timelineThumbs(path, false).then(([p]) => live && (film = convertFileSrc(p))).catch(() => {});
    return () => (live = false);
  });

  const dur = $derived(c.info.duration || 0);
  const pct = (t: number) => (dur > 0 ? (Math.min(Math.max(t, 0), dur) / dur) * 100 : 0);
  const slots = $derived.by(() => {
    if (!trackW || !c.info.video.height) return [];
    const slotW = (24 * c.info.video.width) / c.info.video.height;
    const n = Math.max(1, Math.min(200, Math.round(trackW / Math.max(12, slotW))));
    return Array.from({ length: n }, (_, k) => Math.min(THUMBS - 1, Math.floor(((k + 0.5) / n) * THUMBS)));
  });
  /** where the preview is in this file, while this clip shows */
  const head = $derived.by(() => {
    const a = splits.at(i, splits.time);
    return splits.split || a.active || (!splits.split && splits.indexAt(splits.time) === i) ? a.local : null;
  });

  function timeAt(e: { clientX: number }) {
    const b = track!.getBoundingClientRect();
    const f = b.width > 0 ? (e.clientX - b.left) / b.width : 0;
    return Math.min(Math.max(f, 0), 1) * dur;
  }

  type Drag = { kind: "start" | "end"; snaps: number[] } | { kind: "body"; t0: number; s0: number; e0: number; moved: boolean };
  let drag = $state<Drag | null>(null);

  /** the output time at which this clip shows file time t */
  function outAt(t: number) {
    const o = splits.split ? 0 : splits.offsets[i];
    return o + Math.max(0, t - c.start);
  }

  function down(e: PointerEvent, kind: "start" | "end" | "body") {
    if (e.button !== 0 || !dur) return;
    e.preventDefault();
    e.stopPropagation();
    track!.setPointerCapture(e.pointerId);
    splits.playing = false;
    if (kind === "body") {
      drag = { kind, t0: timeAt(e), s0: c.start, e0: c.end, moved: false };
      return;
    }
    // the other clips' lengths, measured from the end that stays
    const snaps = [0, dur];
    splits.clips.forEach((o, k) => {
      if (k === i || (splits.split && o.still)) return;
      const len = splits.lenOf(k);
      snaps.push(kind === "end" ? c.start + len : c.end - len);
    });
    drag = { kind, snaps };
  }

  function moveEv(e: PointerEvent) {
    if (dur) hoverT = timeAt(e);
    if (!drag) return;
    let t = timeAt(e);
    if (drag.kind === "body") {
      const d = t - drag.t0;
      if (Math.abs(d) > 0.01) drag.moved = true;
      const len = drag.e0 - drag.s0;
      const s = Math.min(Math.max(0, drag.s0 + d), dur - len);
      splits.setTrim(c.id, s, s + len);
      splits.peek = { id: c.id, t: s };
      return;
    }
    if (!e.altKey && trackW > 0) {
      let best = (SNAP_PX / trackW) * dur;
      for (const s of drag.snaps) {
        if (Math.abs(s - t) < best) {
          best = Math.abs(s - t);
          t = s;
        }
      }
    }
    if (drag.kind === "start") splits.setTrim(c.id, Math.min(t, c.end - 0.1), c.end);
    else splits.setTrim(c.id, c.start, Math.max(t, c.start + 0.1));
    splits.peek = { id: c.id, t: drag.kind === "start" ? c.start : c.end };
  }

  function up(e: PointerEvent) {
    if (!drag) return;
    track!.releasePointerCapture(e.pointerId);
    const d = drag;
    drag = null;
    splits.peek = null;
    // a click in the range (no drag): show that moment
    if (d.kind === "body" && !d.moved) splits.seek(outAt(Math.min(Math.max(d.t0, c.start), c.end)));
    else splits.seek(splits.time);
  }
</script>

<div
  class="strack"
  bind:this={track}
  bind:clientWidth={trackW}
  role="presentation"
  onpointermove={moveEv}
  onpointerup={up}
  onpointercancel={up}
  onpointerleave={() => (hoverT = null)}
>
  <div class="film">
    {#if film}
      {#each slots as k, n (n)}
        <span class="slot" style:background-image="url('{film}')" style:background-size="{THUMBS * 100}% 100%" style:background-position="{(k / (THUMBS - 1)) * 100}% 0"></span>
      {/each}
    {/if}
  </div>
  <div class="out" style:left="0" style:width="{pct(c.start)}%"></div>
  <div class="out" style:left="{pct(c.end)}%" style:right="0"></div>
  <div
    class="range"
    style:left="{pct(c.start)}%"
    style:width="{pct(c.end) - pct(c.start)}%"
    role="presentation"
    onpointerdown={(e) => down(e, "body")}
  >
    <span class="len mono">{fmtTime(c.end - c.start)}</span>
  </div>
  <div class="h start" style:left="{pct(c.start)}%" role="presentation" aria-label="Start of clip {i + 1}" onpointerdown={(e) => down(e, "start")}></div>
  <div class="h end" style:left="{pct(c.end)}%" role="presentation" aria-label="End of clip {i + 1}" onpointerdown={(e) => down(e, "end")}></div>
  {#if head !== null}<div class="head" style:left="{pct(head)}%"></div>{/if}
  {#if hoverT !== null && !drag}<span class="hover mono" style:left="{Math.min(92, Math.max(2, pct(hoverT)))}%">{fmtTime(hoverT)}</span>{/if}
</div>

<style>
  .strack {
    position: relative;
    height: 28px;
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    overflow: hidden;
    touch-action: none;
    user-select: none;
  }
  .film {
    position: absolute;
    inset: 0;
    display: flex;
    opacity: 0.75;
  }
  .slot {
    flex: 1;
    min-width: 0;
    background-repeat: no-repeat;
  }
  .out {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    pointer-events: none;
  }
  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    border-top: 2px solid var(--accent);
    border-bottom: 2px solid var(--accent);
    cursor: grab;
  }
  .range:active {
    cursor: grabbing;
  }
  .len {
    position: absolute;
    right: 4px;
    bottom: 1px;
    padding: 0 3px;
    border-radius: 2px;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    font-size: 10px;
    pointer-events: none;
  }
  .h {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 8px;
    background: var(--accent);
    cursor: ew-resize;
  }
  .h.start {
    margin-left: 0;
    border-radius: 3px 0 0 3px;
    transform: translateX(-2px);
  }
  .h.end {
    margin-left: -8px;
    border-radius: 0 3px 3px 0;
    transform: translateX(2px);
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: #fff;
    box-shadow: 0 0 2px rgba(0, 0, 0, 0.8);
    pointer-events: none;
  }
  .hover {
    position: absolute;
    top: 1px;
    transform: translateX(-50%);
    padding: 0 3px;
    border-radius: 2px;
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    font-size: 10px;
    pointer-events: none;
  }
</style>
