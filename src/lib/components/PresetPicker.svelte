<script lang="ts">
  // Preset select plus the manager. Every preset is a TOML file in the config
  // folder (presets/platform, presets/saved): save as new, overwrite, rename,
  // delete, import and export (JSON), open the folder, reload after editing
  // the files by hand, restore the platform presets.
  import { app } from "$lib/state.svelte";
  import { errText, openItem } from "$lib/ipc";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";

  let manage = $state(false);
  let mode = $state<"new" | "rename" | null>(null);
  let name = $state("");
  let msg = $state<{ text: string; bad: boolean } | null>(null);
  let confirmDelete = $state(false);

  const entry = $derived(app.presetEntries.find((e) => e.preset.name === app.presetName) ?? null);
  const noFile = $derived(!entry?.file);
  const savedCount = $derived(app.presetEntries.filter((e) => e.group === "saved").length);
  const options = $derived(
    app.presetEntries.map((e) => ({ value: e.preset.name, label: e.preset.name, group: e.group === "platform" ? "Platform" : "Saved" })),
  );

  async function openFolder() {
    if (!app.config) return;
    try {
      await openItem(app.config.dir);
    } catch (e) {
      msg = { text: errText(e), bad: true };
    }
  }

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

  async function act(f: () => Promise<string | null | undefined>, ok: string) {
    const err = await f();
    if (err === undefined) return; // dialog canceled
    msg = err ? { text: err, bad: true } : { text: ok, bad: false };
  }

  async function exportAll() {
    if (!savedCount) {
      msg = { text: "There are no saved presets to export.", bad: false };
      return;
    }
    await act(() => app.exportPresets(), `Exported ${savedCount} presets.`);
  }

  async function importFile() {
    await act(() => app.importPresets(), "Presets imported.");
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

  {#if app.presetChanges.length && !app.streamCopy}
    <div class="changed small">
      <span class="muted">Changed: {app.presetChanges.join(", ")}</span>
      <button class="btn small" title="Set every field the preset has back to its value" onclick={() => app.applyPreset(app.presetName, false)}>Reset</button>
    </div>
  {/if}

  {#if manage}
    <div class="tools">
      <button class="btn small" onclick={() => begin("new")}><Icon name="save" size={11} />Save as new</button>
      <button class="btn small" disabled={noFile} title="Write the current settings into this preset's file" onclick={() => act(() => app.overwritePreset(app.presetName), `Saved "${app.presetName}".`)}>Overwrite</button>
      <button class="btn small" disabled={noFile} onclick={() => begin("rename")}>Rename</button>
      {#if confirmDelete}
        <button class="btn small danger" onclick={() => { void act(() => app.deletePreset(app.presetName), "Preset deleted."); confirmDelete = false; }}>Delete "{app.presetName}"?</button>
      {:else}
        <button class="btn small danger" disabled={noFile} onclick={() => (confirmDelete = true)}><Icon name="trash" size={11} />Delete</button>
      {/if}
      <button class="btn small" title="Add presets from a JSON file exported by comfywebm" onclick={importFile}><Icon name="download" size={11} />Import</button>
      <button class="btn small" title="Save your own presets (presets/saved) as one JSON file to share" onclick={exportAll}><Icon name="upload" size={11} />Export</button>
    </div>
    <div class="tools">
      <button class="btn small" disabled={!app.config} title="The folder with config.toml and the presets folder. Each preset is a .toml file you can edit in a text editor." onclick={openFolder}><Icon name="open" size={11} />Open config folder</button>
      <button class="btn small" title="Read config.toml and the preset files again after editing them" onclick={() => app.reloadConfig()}>Reload config</button>
      <button class="btn small" title="Write the platform presets (4chan, Discord, X, Quality) again with their default values. Your saved presets are not touched." onclick={() => act(() => app.restorePlatformPresets(), "Platform presets restored.")}>Restore platform presets</button>
    </div>
    {#if entry?.file}<div class="muted small mono">presets/{entry.file}</div>{/if}
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
  .changed {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: space-between;
  }
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
