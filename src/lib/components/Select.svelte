<script lang="ts" generics="T extends string | number | null">
  // A drop-down list drawn by the app instead of the native <select>: it
  // follows the theme, and it opens on click (after the mouse button is
  // released), so nothing that runs on mouse-up can close it.
  import { onDestroy, tick } from "svelte";
  import Icon from "./Icon.svelte";

  interface SelectOption<V> {
    value: V;
    label: string;
    /** shown right-aligned and muted in the list */
    hint?: string;
    disabled?: boolean;
    /** consecutive options with the same group get one heading */
    group?: string;
  }

  interface Props {
    value: T;
    options: SelectOption<T>[];
    label: string;
    disabled?: boolean;
    /** called instead of writing `value` */
    onchange?: (v: T) => void;
    /** text for the closed control when `value` is not in `options` */
    placeholder?: string;
  }

  let { value = $bindable(), options, label, disabled = false, onchange, placeholder = "" }: Props = $props();

  let open = $state(false);
  let active = $state(-1);
  let trigger: HTMLButtonElement;
  let list = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0, width: 0, maxH: 280, up: false });

  const current = $derived(options.find((o) => o.value === value) ?? null);

  function place() {
    const r = trigger.getBoundingClientRect();
    const below = window.innerHeight - r.bottom - 8;
    const above = r.top - 8;
    const want = Math.min(320, options.length * 26 + 12 + new Set(options.map((o) => o.group).filter(Boolean)).size * 24);
    const up = below < Math.min(want, 160) && above > below;
    const width = Math.max(r.width, 140);
    const left = Math.min(r.left, window.innerWidth - width - 6);
    pos = { left: Math.max(6, left), top: up ? r.top - 4 : r.bottom + 4, width, maxH: Math.max(120, Math.min(320, up ? above : below)), up };
  }

  async function show() {
    if (disabled || open) return;
    place();
    active = options.findIndex((o) => o.value === value);
    open = true;
    await tick();
    list?.querySelector<HTMLElement>(".opt.cur")?.scrollIntoView({ block: "nearest" });
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("keydown", keys, true);
    window.addEventListener("resize", hide);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("blur", hide);
  }

  function hide() {
    if (!open) return;
    open = false;
    window.removeEventListener("pointerdown", outside, true);
    window.removeEventListener("keydown", keys, true);
    window.removeEventListener("resize", hide);
    window.removeEventListener("scroll", onScroll, true);
    window.removeEventListener("blur", hide);
  }

  onDestroy(hide);

  function pick(i: number) {
    const o = options[i];
    if (!o || o.disabled) return;
    hide();
    if (o.value === value) return;
    if (onchange) onchange(o.value);
    else value = o.value;
  }

  function outside(e: PointerEvent) {
    const t = e.target as Node;
    if (list?.contains(t) || trigger.contains(t)) return;
    hide();
  }

  function onScroll(e: Event) {
    if (list && list.contains(e.target as Node)) return;
    hide();
  }

  function step(d: number) {
    if (!options.length) return;
    let i = active;
    for (let n = 0; n < options.length; n++) {
      i = (i + d + options.length) % options.length;
      if (!options[i].disabled) break;
    }
    active = i;
    void tick().then(() => list?.querySelector<HTMLElement>(".opt.act")?.scrollIntoView({ block: "nearest" }));
  }

  // typing letters while open jumps to the first option whose label starts with them
  let typed = "";
  let typedAt = 0;
  function jump(ch: string) {
    const now = Date.now();
    typed = now - typedAt > 800 ? ch : typed + ch;
    typedAt = now;
    const t = typed.toLowerCase();
    const i = options.findIndex((o) => !o.disabled && o.label.toLowerCase().startsWith(t));
    if (i < 0) return;
    active = i;
    void tick().then(() => list?.querySelector<HTMLElement>(".opt.act")?.scrollIntoView({ block: "nearest" }));
  }

  // while open, the list owns the keyboard wherever focus is
  function keys(e: KeyboardEvent) {
    const k = e.key;
    if (k.length === 1 && k !== " " && !e.ctrlKey && !e.altKey && !e.metaKey) jump(k);
    else if (k === " " && typed && Date.now() - typedAt < 800) jump(k);
    else if (k === "ArrowDown" || k === "ArrowUp") step(k === "ArrowDown" ? 1 : -1);
    else if (k === "Home") {
      active = -1;
      step(1);
    }
    else if (k === "Enter" || k === " ") pick(active);
    else if (k === "Escape" || k === "Tab") hide();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function onTriggerKey(e: KeyboardEvent) {
    if (open) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      void show();
    }
  }
</script>

<button
  bind:this={trigger}
  class="field sel"
  class:open
  type="button"
  aria-label={label}
  aria-haspopup="listbox"
  aria-expanded={open}
  {disabled}
  onclick={() => (open ? hide() : void show())}
  onkeydown={onTriggerKey}
>
  <span class="cur-label">{current?.label ?? placeholder}</span>
  <span class="chev"><Icon name="chevron" size={12} /></span>
</button>

{#if open}
  <div
    bind:this={list}
    class="pop"
    class:up={pos.up}
    role="listbox"
    aria-label={label}
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    style:min-width="{pos.width}px"
    style:max-height="{pos.maxH}px"
  >
    {#each options as o, i (i)}
      {#if o.group && o.group !== options[i - 1]?.group}
        <div class="grp">{o.group}</div>
      {/if}
      <div
        class="opt"
        class:cur={o.value === value}
        class:act={i === active}
        class:dis={o.disabled}
        role="option"
        tabindex="-1"
        aria-selected={o.value === value}
        aria-disabled={o.disabled}
        onpointerenter={() => !o.disabled && (active = i)}
        onpointerdown={(e) => e.preventDefault()}
        onclick={() => pick(i)}
        onkeydown={() => {}}
      >
        <span class="tick">{#if o.value === value}<Icon name="check" size={12} />{/if}</span>
        <span class="lbl">{o.label}</span>
        {#if o.hint}<span class="hint">{o.hint}</span>{/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .sel {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    text-align: left;
    cursor: pointer;
  }
  .sel:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .sel.open {
    border-color: var(--accent-line);
  }
  .cur-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    display: inline-flex;
    color: var(--text-muted);
  }
  .open .chev {
    transform: rotate(180deg);
  }
  .pop {
    position: fixed;
    z-index: 200;
    max-width: 420px;
    overflow-y: auto;
    padding: 4px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-panel-alt);
    box-shadow: var(--shadow-pop);
    font-size: 12px;
  }
  .pop.up {
    transform: translateY(-100%);
  }
  .grp {
    padding: 6px 8px 3px;
    font-family: var(--font-head);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 26px;
    padding: 0 8px 0 4px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    white-space: nowrap;
    cursor: pointer;
  }
  .opt.act {
    background: var(--bg-hover);
  }
  .opt.cur {
    color: var(--accent-strong);
  }
  .opt.dis {
    opacity: 0.4;
    cursor: default;
  }
  .tick {
    width: 14px;
    display: inline-flex;
    justify-content: center;
  }
  .lbl {
    flex: 1;
  }
  .hint {
    margin-left: 14px;
    color: var(--text-muted);
    font-size: 11px;
  }
</style>
