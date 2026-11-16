<script lang="ts">
  // Font, colors and outline of the caption. The installed fonts are read when
  // this first shows (the Caption group is open).
  import { onMount } from "svelte";
  import { app } from "$lib/state.svelte";
  import { pickFont } from "$lib/dialogs";
  import Select from "./Select.svelte";

  onMount(() => app.loadFonts());

  const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const faceLabel = (f: { family: string; style: string }) =>
    /^(regular|normal|book|roman)$/i.test(f.style) ? f.family : `${f.family} ${f.style}`;

  const fontOptions = $derived.by(() => {
    const opts: { value: string | null; label: string; hint?: string }[] = [{ value: null, label: "Default (bold sans)" }];
    const listed = app.fonts.some((f) => f.path === app.captionFont);
    // a font file chosen with "File" that is not installed
    if (app.captionFont && app.fonts.length && !listed) opts.push({ value: app.captionFont, label: baseName(app.captionFont), hint: "file" });
    for (const f of app.fonts) opts.push({ value: f.path, label: faceLabel(f) });
    return opts;
  });

  async function chooseFile() {
    const f = await pickFont();
    if (f) app.captionFont = f;
  }
</script>

<div class="row">
  <span title="The font the caption is drawn in. Type letters while the list is open to jump to a font. File picks a .ttf or .otf that is not installed.">Font</span>
  <span class="pair">
    <Select label="Caption font" bind:value={app.captionFont} options={fontOptions} placeholder={app.fonts.length ? "" : "Reading fonts..."} />
    <button class="btn small" onclick={chooseFile}>File</button>
  </span>
</div>
<div class="row">
  <span>Color</span>
  <span class="pair">
    <label class="color" title="Text color"><input type="color" bind:value={app.captionColor} aria-label="Caption text color" /> Text</label>
    <label class="color" title="Outline color"><input type="color" bind:value={app.captionOutlineColor} aria-label="Caption outline color" disabled={!app.captionOutline} /> Outline</label>
  </span>
</div>
<div class="row">
  <span title="Width of the outline around each letter, as a share of the text height.">Outline</span>
  <Select
    label="Caption outline width"
    bind:value={app.captionOutline}
    options={[
      { value: 0, label: "None" },
      { value: 4, label: "Thin" },
      { value: 7, label: "Normal" },
      { value: 12, label: "Thick" },
    ]}
  />
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 26px;
  }
  .row > span:first-child {
    flex: none;
    color: var(--text-secondary);
    font-size: 12px;
  }
  .row :global(.sel) {
    flex: 1;
    max-width: 190px;
    min-width: 0;
  }
  .pair {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 0;
    max-width: 230px;
  }
  .color {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .color input[type="color"] {
    width: 30px;
    height: 20px;
    padding: 0;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-sm);
    background: none;
    cursor: pointer;
  }
  .color input:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
