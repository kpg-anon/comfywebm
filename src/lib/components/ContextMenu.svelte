<script lang="ts">
  // The right-click menu (see menu.svelte.ts). Keeps inside the window, closes
  // on a click elsewhere, Escape, scrolling or the window losing focus.
  import { tick } from "svelte";
  import { menu, type MenuItem } from "$lib/menu.svelte";

  let box = $state<HTMLDivElement>();
  let left = $state(0);
  let top = $state(0);
  let active = $state(-1);

  const items = $derived(menu.items);
  const usable = (i: number) => {
    const it = items[i];
    return it !== undefined && it !== "sep" && !it.disabled;
  };

  $effect(() => {
    if (!menu.open) return;
    active = -1;
    left = menu.x;
    top = menu.y;
    void tick().then(() => {
      if (!box) return;
      const r = box.getBoundingClientRect();
      // the menu's own size in the page's coordinates (the UI scale zooms the page)
      const k = r.width / Math.max(1, box.offsetWidth) || 1;
      const w = r.width / k;
      const h = r.height / k;
      const vw = window.innerWidth / k;
      const vh = window.innerHeight / k;
      left = Math.max(4, Math.min(menu.x, vw - w - 4));
      top = menu.y + h > vh - 4 ? Math.max(4, menu.y - h) : menu.y;
    });
    const outside = (e: PointerEvent) => {
      if (!box?.contains(e.target as Node)) menu.hide();
    };
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
      } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        const d = e.key === "ArrowDown" ? 1 : -1;
        for (let k = 1; k <= items.length; k++) {
          const i = (active + d * k + items.length * 2) % items.length;
          if (usable(i)) {
            active = i;
            break;
          }
        }
      } else if (e.key === "Enter" && active >= 0) {
        e.preventDefault();
        e.stopPropagation();
        pick(items[active]);
      }
    };
    const hide = () => menu.hide();
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("keydown", keys, true);
    window.addEventListener("resize", hide);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("wheel", hide, { capture: true, passive: true });
    window.addEventListener("blur", hide);
    return () => {
      window.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("keydown", keys, true);
      window.removeEventListener("resize", hide);
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("wheel", hide, { capture: true });
      window.removeEventListener("blur", hide);
    };
  });

  function close() {
    menu.hide();
    menu.owner?.focus({ preventScroll: true });
  }

  function pick(it: MenuItem | undefined) {
    if (!it || it === "sep" || it.disabled) return;
    close();
    it.action();
  }
</script>

{#if menu.open}
  <div
    class="cmenu"
    role="menu"
    tabindex="-1"
    bind:this={box}
    style:left="{left}px"
    style:top="{top}px"
    onpointerdown={(e) => e.preventDefault()}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#each items as it, i (i)}
      {#if it === "sep"}
        <div class="sep" role="separator"></div>
      {:else}
        <div
          class="item"
          class:act={active === i}
          class:dis={it.disabled}
          class:danger={it.danger}
          role="menuitem"
          tabindex="-1"
          aria-disabled={it.disabled ? "true" : undefined}
          onpointerenter={() => (active = it.disabled ? -1 : i)}
          onpointerleave={() => (active = -1)}
          onclick={() => pick(it)}
          onkeydown={() => {}}
        >
          <span class="lbl">{it.label}</span>
          {#if it.keys}<span class="keys">{it.keys}</span>{/if}
        </div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .cmenu {
    position: fixed;
    z-index: 900;
    min-width: 170px;
    max-width: 360px;
    padding: 4px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-panel-alt);
    box-shadow: var(--shadow-pop);
    font-family: var(--font-ui);
    font-size: 12px;
    user-select: none;
    outline: none;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 18px;
    min-height: 26px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    white-space: nowrap;
    cursor: default;
  }
  .item.act {
    background: var(--bg-hover);
  }
  .item.danger {
    color: var(--red);
  }
  .item.dis {
    opacity: 0.4;
  }
  .lbl {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .keys {
    color: var(--text-muted);
    font-size: 11px;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border-muted);
  }
</style>
