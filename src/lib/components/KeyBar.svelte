<script lang="ts">
  // One keyframe's editor, a row under its lane (Rotate, Follow): which
  // keyframe it is and the ones beside it, its time a frame at a time, the
  // lane's own controls (`children`), Remove and close. It sits in the timeline
  // panel, so it never covers the picture, and it is always there while the
  // lane is: with no keyframe open (`i` null) its controls are greyed out, so
  // opening one does not move the lane.
  import type { Snippet } from "svelte";
  import TimeField from "./TimeField.svelte";
  import Icon from "./Icon.svelte";

  let {
    i,
    n,
    t,
    onprev,
    onnext,
    ontime,
    onstep,
    onremove,
    onclose,
    children,
  }: {
    i: number | null;
    n: number;
    t: number | null;
    onprev: () => void;
    onnext: () => void;
    ontime: (t: number) => void;
    onstep: (frames: number) => void;
    onremove: () => void;
    onclose: () => void;
    children: Snippet;
  } = $props();
</script>

<div class="kbar" class:idle={i === null} role="group" aria-label={i === null ? "No keyframe selected" : `Keyframe ${i + 1} of ${n}`}>
  <span class="nav">
    <button class="btn small icon" disabled={i === null || i === 0} title="Previous keyframe (PgUp)" aria-label="Previous keyframe" onclick={onprev}>‹</button>
    <span class="which mono" title={i === null ? "Click a keyframe on the lane to edit it" : ""}>{i === null ? "No keyframe" : `Keyframe ${i + 1}/${n}`}</span>
    <button class="btn small icon" disabled={i === null || i >= n - 1} title="Next keyframe (PgDn)" aria-label="Next keyframe" onclick={onnext}>›</button>
  </span>
  <span class="part">
    <span class="lbl">At</span>
    <TimeField label="" value={t} onset={ontime} readonly={i === null} />
    <button class="btn small icon" disabled={i === null} title="One frame earlier" aria-label="One frame earlier" onclick={() => onstep(-1)}>◂</button>
    <button class="btn small icon" disabled={i === null} title="One frame later" aria-label="One frame later" onclick={() => onstep(1)}>▸</button>
  </span>
  {@render children()}
  <span class="end">
    <button class="btn small" disabled={i === null} title="Remove this keyframe (Delete)" onclick={onremove}><Icon name="trash" size={12} />Remove</button>
    <button class="btn small icon" disabled={i === null} title="Close (Esc)" aria-label="Close" onclick={onclose}><Icon name="x" size={12} /></button>
  </span>
</div>

<style>
  .kbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    margin-top: 4px;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    border-left: 3px solid var(--accent);
    font-size: 12px;
  }
  .nav,
  .part,
  .end,
  .kbar :global(.kpart) {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .end {
    margin-left: auto;
  }
  /* no keyframe open: the same row, greyed out */
  .kbar.idle {
    border-left-color: var(--border);
  }
  .kbar.idle > :not(.nav) {
    opacity: 0.45;
  }
  .which {
    min-width: 92px;
    text-align: center;
  }
  .lbl,
  .kbar :global(.klbl) {
    color: var(--text-secondary);
  }
  .icon {
    min-width: 24px;
    justify-content: center;
  }
  .kbar :global(.tf .lbl) {
    display: none;
  }
  .kbar :global(.tf input) {
    width: 92px;
  }
</style>
