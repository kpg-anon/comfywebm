<script lang="ts">
  // Preset select plus the manager for user presets: save as new, overwrite,
  // rename, delete, import and export (JSON file).
  import { app } from "$lib/state.svelte";
  import { BUILTIN_PRESETS, isBuiltin } from "$lib/presets";
  import { pickPresetsFile, savePresetsFile } from "$lib/dialogs";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";

  let manage = $state(false);
  let mode = $state<"new" | "rename" | null>(null);
  let name = $state("");
  let msg = $state<{ text: string; bad: boolean } | null>(null);
  let confirmDelete = $state(false);

  const builtin = $derived(isBuiltin(app.presetName));
  const options = $derived([
    ...BUILTIN_PRESETS.map((p) => ({ value: p.name, label: p.name, group: "Built-in" })),
    ...app.settings.presets.map((p) => ({ value: p.name, label: p.name, group: "Saved" })),
  ]);

  function begin(m: "new" | "rename") {
    mode = m;
    name = m === "rename" ? app.presetName : "";
    msg = null;
    confirmDelete = false;
  }

  async function submit() {
    const err = mode === "rename" ? await app.renamePreset(app.presetName, name) : await app.savePresetAs(name);
    if (err) msg = { text: err, bad: true };
    else {
      mode = null;
      msg = null;
    }
  }

  async function act(f: () => Promise<string | null>, ok: string) {
    const err = await f();
    msg = err ? { text: err, bad: true } : { text: ok, bad: false };
  }

  async function exportAll() {
    if (!app.settings.presets.length) {
      msg = { text: "There are no saved presets to export.", bad: false };
      return;
    }
    const path = await savePresetsFile();
    if (path) await act(() => app.exportPresets(path), `Exported ${app.settings.presets.length} presets.`);
  }

  async function importFile() {
    const path = await pickPresetsFile();
    if (path) await act(() => app.importPresets(path), "Presets imported.");
  }
</script>

<div class="preset">
  <div class="line">
    <Select
      label="Preset"
      value={app.presetName}
      {options}
      onchange={(v) => {
        app.applyPreset(v);
        confirmDelete = false;
        mode = null;
        msg = null;
      }}
    />
    <button class="btn icon" class:on={manage} aria-label="Manage presets" aria-pressed={manage} onclick={() => { manage = !manage; mode = null; msg = null; }}>
      <Icon name="edit" size={14} />
    </button>
  </div>

  {#if manage}
    <div class="tools">
      <button class="btn small" onclick={() => begin("new")}><Icon name="save" size={11} />Save as new</button>
      <button class="btn small" disabled={builtin} onclick={() => act(() => app.overwritePreset(app.presetName), `Saved "${app.presetName}".`)}>Overwrite</button>
      <button class="btn small" disabled={builtin} onclick={() => begin("rename")}>Rename</button>
      {#if confirmDelete}
        <button class="btn small danger" onclick={() => { void app.deletePreset(app.presetName); confirmDelete = false; }}>Delete "{app.presetName}"?</button>
      {:else}
        <button class="btn small danger" disabled={builtin} onclick={() => (confirmDelete = true)}><Icon name="trash" size={11} />Delete</button>
      {/if}
      <button class="btn small" onclick={importFile}><Icon name="download" size={11} />Import</button>
      <button class="btn small" onclick={exportAll}><Icon name="upload" size={11} />Export</button>
    </div>
    {#if builtin}<div class="muted small">Built-in presets cannot be changed. "Save as new" makes an editable copy.</div>{/if}
  {/if}

  {#if mode}
    <div class="line">
      <input
        class="field"
        type="text"
        placeholder="Preset name"
        bind:value={name}
        onkeydown={(e) => {
          if (e.key === "Enter") void submit();
          if (e.key === "Escape") mode = null;
        }}
      />
      <button class="btn primary" onclick={submit}>{mode === "rename" ? "Rename" : "Save"}</button>
      <button class="btn" onclick={() => (mode = null)}>Cancel</button>
    </div>
  {/if}
  {#if msg}<div class="small" class:err={msg.bad} class:muted={!msg.bad}>{msg.text}</div>{/if}
</div>

<style>
  .preset {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .line {
    display: flex;
    gap: 4px;
  }
  .line :global(.sel),
  .line input {
    flex: 1;
    min-width: 0;
    height: var(--ctl-h);
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .small {
    font-size: 11px;
  }
</style>
