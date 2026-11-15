<script lang="ts">
  // A collapsible sidebar group. Open/closed state is kept in localStorage.
  import { untrack, type Snippet } from "svelte";
  import { load, save } from "$lib/persist";
  import Icon from "./Icon.svelte";

  let { id, title, note = "", children }: { id: string; title: string; note?: string; children: Snippet } = $props();

  const stored = () => load<Record<string, boolean>>("collapsed") ?? {};
  // `id` is fixed for a group's lifetime; read it once
  let closed = $state(untrack(() => stored()[id] === true));

  function toggle() {
    closed = !closed;
    const all = stored();
    all[id] = closed;
    save("collapsed", all);
  }
</script>

<section>
  <button class="head" aria-expanded={!closed} onclick={toggle}>
    <span class="chev" class:closed><Icon name="chevron" size={12} /></span>
    <span class="title">{title}</span>
    {#if note}<span class="note">{note}</span>{/if}
  </button>
  {#if !closed}
    <div class="body">{@render children()}</div>
  {/if}
</section>

<style>
  section {
    border-bottom: 1px solid var(--border-muted);
  }
  .head {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
    text-align: left;
  }
  .title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-secondary);
  }
  .note {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chev {
    display: inline-flex;
    color: var(--text-muted);
  }
  .chev.closed {
    transform: rotate(-90deg);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 0 0 10px;
  }
</style>
