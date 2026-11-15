<script lang="ts">
  // The joined output as a strip: one block per range in output order, sized
  // by length. Drag a block to change the order, click it to select the range
  // and jump to its start. "Play edit" plays the ranges one after another in
  // this order. Block numbers are the range numbers on the source timeline.
  import { app } from "$lib/state.svelte";
  import { fmtTime } from "$lib/time";
  import Icon from "./Icon.svelte";

  let strip: HTMLDivElement;
  let drag = $state<{ from: number; x0: number; moved: boolean; to: number } | null>(null);

  const list = $derived(app.outSegments);
  const total = $derived(list.reduce((n, s) => n + (s.end - s.start), 0));
  const num = (s: (typeof list)[number]) => app.segments.indexOf(s) + 1;
  const lenText = (t: number) => (t < 60 ? `${t.toFixed(1)}s` : fmtTime(t).replace(/\.\d+$/, ""));

  /** Output position the pointer is over: the gap before the block whose middle is right of x. */
  function slotAt(x: number): number {
    const blocks = [...strip.querySelectorAll<HTMLElement>(".blk")];
    for (let i = 0; i < blocks.length; i++) {
      const r = blocks[i].getBoundingClientRect();
      if (x < r.left + r.width / 2) return i;
    }
    return blocks.length;
  }

  function down(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { from: i, x0: e.clientX, moved: false, to: i };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    if (!drag.moved && Math.abs(e.clientX - drag.x0) < 5) return;
    drag.moved = true;
    drag.to = slotAt(e.clientX);
  }

  function up(e: PointerEvent, i: number) {
    if (!drag) return;
    const d = drag;
    drag = null;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    if (!d.moved) {
      const s = list[i];
      app.sel = app.segments.indexOf(s);
      app.seek(s.start);
      return;
    }
    // a slot after the dragged block's own position shifts by one once it is taken out
    const to = d.to > d.from ? d.to - 1 : d.to;
    app.moveOut(d.from, to);
  }
</script>

<div class="edit">
  <span class="lbl">Output</span>
  <div class="strip" bind:this={strip}>
    {#each list as s, i (s.id ?? i)}
      {#if drag?.moved && drag.to === i && drag.to !== drag.from && drag.to !== drag.from + 1}<span class="ins"></span>{/if}
      <div
        class="blk"
        class:active={app.segments.indexOf(s) === app.sel}
        class:dragging={drag?.moved && drag.from === i}
        style:flex-grow={Math.max(0.0001, s.end - s.start)}
        role="button"
        tabindex="-1"
        title="Range {num(s)}: {fmtTime(s.start)} to {fmtTime(s.end)}. Drag to reorder."
        onpointerdown={(e) => down(e, i)}
        onpointermove={move}
        onpointerup={(e) => up(e, i)}
        onpointercancel={() => (drag = null)}
      >
        <span class="n">{num(s)}</span>
        <span class="len mono">{lenText(s.end - s.start)}</span>
      </div>
    {/each}
    {#if drag?.moved && drag.to === list.length && drag.from !== list.length - 1}<span class="ins"></span>{/if}
  </div>
  <span class="total mono" title="Length of the joined output before speed">{fmtTime(total)}</span>
  <button class="btn small" class:on={app.playEdit} aria-pressed={app.playEdit} title="Play the ranges one after another in output order" onclick={() => app.togglePlayEdit()}>
    <Icon name={app.playEdit && !app.paused ? "pause" : "play"} size={11} />Play edit
  </button>
  {#if app.reordered}
    <button class="btn small" title="Join the ranges in timeline order again" onclick={() => app.resetOrder()}>Timeline order</button>
  {/if}
</div>

<style>
  .edit {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .lbl {
    flex: none;
    font-family: var(--font-head);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: var(--head-track);
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .strip {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 3px;
    align-items: stretch;
    height: 24px;
  }
  .blk {
    flex-basis: 0;
    min-width: 46px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    padding: 0 6px;
    border: var(--bw) solid color-mix(in srgb, var(--range) 45%, transparent);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--range) 16%, var(--bg-chip));
    color: var(--text-secondary);
    font-size: 11px;
    overflow: hidden;
    cursor: grab;
    touch-action: none;
  }
  .blk:hover {
    background: color-mix(in srgb, var(--range) 26%, var(--bg-chip));
  }
  .blk.active {
    border-color: var(--range);
    background: color-mix(in srgb, var(--range) 34%, var(--bg-chip));
    color: var(--text-primary);
  }
  .blk.dragging {
    opacity: 0.4;
    cursor: grabbing;
  }
  .n {
    font-weight: 700;
  }
  .len {
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
  }
  .ins {
    flex: none;
    width: 3px;
    margin: 0 -3px;
    border-radius: 2px;
    background: var(--accent);
    z-index: 1;
  }
  .total {
    flex: none;
    font-size: 11px;
    color: var(--text-secondary);
  }
</style>
