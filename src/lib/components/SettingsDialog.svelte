<script lang="ts">
  import { app, splitArgs, REFRESH_RATES } from "$lib/state.svelte";
  import { pickAnyFile, pickFolder } from "$lib/dialogs";
  import { THEMES, applyTheme, storedTheme } from "$lib/themes";
  import Select from "./Select.svelte";

  // appearance applies at once and is kept per machine (not part of Save)
  let theme = $state(storedTheme());
  function pickTheme(id: string) {
    theme = id;
    applyTheme(id);
  }

  // edit a copy; nothing is saved until Save
  let ffmpegPath = $state(app.settings.ffmpegPath ?? "");
  let outputDir = $state(app.settings.outputDir ?? "");
  let ytdlpPath = $state(app.settings.ytdlpPath ?? "");
  let ytdlpArgs = $state(app.settings.ytdlpArgs.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" "));
  let downloadDir = $state(app.settings.downloadDir ?? "");
  let keepDir = $state(app.settings.keepDir ?? "");
  let nameTemplate = $state(app.settings.nameTemplate);
  let msg = $state<string | null>(null);
  let saving = $state(false);



  async function save() {
    saving = true;
    const err = await app.saveSettings({
      ...app.settings,
      ffmpegPath: ffmpegPath.trim() || null,
      outputDir: outputDir.trim() || null,
      ytdlpPath: ytdlpPath.trim() || null,
      ytdlpArgs: splitArgs(ytdlpArgs),
      downloadDir: downloadDir.trim() || null,
      keepDir: keepDir.trim() || null,
      nameTemplate: nameTemplate.trim() || "{name}_{start}-{end}",
    });
    saving = false;
    if (err) {
      msg = err;
      return;
    }
    await app.refreshTools();
    app.settingsOpen = false;
  }

  const close = () => (app.settingsOpen = false);
</script>

<div
  class="backdrop"
  role="presentation"
  onpointerdown={(e) => {
    if (e.target === e.currentTarget) close();
  }}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-title"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      } else if (e.key === "Enter" && (e.target as HTMLElement).tagName === "INPUT") {
        e.preventDefault();
        void save();
      }
    }}
  >
    <h2 id="settings-title">Settings</h2>

    <div class="col">
      <span>Theme</span>
      <div class="themes" role="radiogroup" aria-label="Theme">
        {#each THEMES as t (t.id)}
          <button class="theme" class:on={theme === t.id} role="radio" aria-checked={theme === t.id} onclick={() => pickTheme(t.id)}>
            <span class="sw">{#each t.swatches as c (c)}<i style:background={c}></i>{/each}</span>
            <span class="tname">{t.name}</span>
            <span class="twhat">{t.what}</span>
          </button>
        {/each}
      </div>
    </div>

    <div class="col">
      <span>Display refresh rate</span>
      <div class="line">
        <Select
          label="Display refresh rate"
          value={app.refreshHz}
          onchange={(v) => app.setRefreshHz(v)}
          options={REFRESH_RATES.map((r) => ({ value: r, label: `${r} Hz` }))}
        />
      </div>
      <span class="hint">The speed and frame rate controls check that the output plays evenly at this rate. Most screens are 60 Hz.</span>
    </div>

    <label class="col">
      <span>ffmpeg location</span>
      <div class="line">
        <input class="field mono" type="text" spellcheck="false" placeholder="Automatic: beside the app, then PATH" bind:value={ffmpegPath} />
        <button class="btn small" onclick={async () => { const d = await pickFolder("Folder with ffmpeg and ffprobe"); if (d) ffmpegPath = d; }}>Folder</button>
        <button class="btn small" onclick={async () => { const f = await pickAnyFile("ffmpeg executable"); if (f) ffmpegPath = f; }}>File</button>
      </div>
      <span class="hint">A folder that holds ffmpeg and ffprobe, or the ffmpeg file itself. Leave empty to search automatically.</span>
    </label>

    <div class="status">
      {#if app.tools}
        <div><span class="k">ffmpeg</span> <span class="mono" class:err={!app.tools.ffmpeg}>{app.tools.ffmpeg ?? "not found"}</span></div>
        <div><span class="k">ffprobe</span> <span class="mono" class:err={!app.tools.ffprobe}>{app.tools.ffprobe ?? "not found"}</span></div>
        {#if app.tools.version}<div><span class="k">version</span> <span class="mono">{app.tools.version}</span></div>{/if}
        <div>
          <span class="k">encoders</span>
          {#each Object.entries(app.tools.encoders) as [k, ok] (k)}
            <span class="mono enc" class:err={!ok}>{k}{ok ? "" : " (missing)"}</span>
          {/each}
        </div>
        <div>
          <span class="k">filters</span>
          {#each Object.entries(app.tools.filters) as [k, ok] (k)}
            <span class="mono enc" class:err={!ok}>{k}{ok ? "" : " (missing)"}</span>
          {/each}
        </div>
        <div><span class="k">yt-dlp</span> <span class="mono" class:err={!app.tools.ytdlp}>{app.tools.ytdlp ?? "not found"}</span>{#if app.tools.ytdlpVersion} <span class="mono muted">{app.tools.ytdlpVersion}</span>{/if}</div>
      {:else}
        <div class="muted">Tool status not loaded.</div>
      {/if}
    </div>

    <label class="col">
      <span>yt-dlp location</span>
      <div class="line">
        <input class="field mono" type="text" spellcheck="false" placeholder="Automatic: beside the app, then PATH" bind:value={ytdlpPath} />
        <button class="btn small" onclick={async () => { const d = await pickFolder("Folder with yt-dlp"); if (d) ytdlpPath = d; }}>Folder</button>
        <button class="btn small" onclick={async () => { const f = await pickAnyFile("yt-dlp executable"); if (f) ytdlpPath = f; }}>File</button>
      </div>
    </label>

    <label class="col">
      <span>Extra yt-dlp arguments</span>
      <input class="field mono" type="text" spellcheck="false" placeholder="for example --cookies-from-browser firefox" bind:value={ytdlpArgs} />
    </label>

    <label class="col">
      <span>Download folder (for editing)</span>
      <div class="line">
        <input class="field mono" type="text" spellcheck="false" placeholder="The app's cache folder" bind:value={downloadDir} />
        <button class="btn small" onclick={async () => { const d = await pickFolder("Download folder"); if (d) downloadDir = d; }}>Choose</button>
      </div>
    </label>

    <label class="col">
      <span>"Download only" folder</span>
      <div class="line">
        <input class="field mono" type="text" spellcheck="false" placeholder="Your Downloads folder" bind:value={keepDir} />
        <button class="btn small" onclick={async () => { const d = await pickFolder("Folder for kept downloads"); if (d) keepDir = d; }}>Choose</button>
      </div>
      <span class="hint">Where the Open URL dialog's "Download only" keeps originals. The download folder above holds files opened for editing.</span>
    </label>

    <label class="col">
      <span>Default output folder</span>
      <div class="line">
        <input class="field mono" type="text" spellcheck="false" placeholder="Beside the source file" bind:value={outputDir} />
        <button class="btn small" onclick={async () => { const d = await pickFolder("Default output folder"); if (d) outputDir = d; }}>Choose</button>
      </div>
    </label>

    <label class="col">
      <span>File name template</span>
      <input class="field mono" type="text" spellcheck="false" bind:value={nameTemplate} />
      <span class="hint">Tokens: <code>{"{name}"}</code> <code>{"{start}"}</code> <code>{"{end}"}</code> <code>{"{codec}"}</code> <code>{"{height}"}</code> <code>{"{date}"}</code>. The extension (.webm or .mp4) is added. Existing files are never overwritten.</span>
    </label>

    {#if msg}<div class="err">{msg}</div>{/if}

    <div class="actions">
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-secondary);
    font-weight: 600;
  }
  .line {
    display: flex;
    gap: 4px;
  }
  .line input {
    flex: 1;
    height: var(--ctl-h);
  }
  .line :global(.sel) {
    width: 140px;
    height: var(--ctl-h);
  }
  .themes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .theme {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto;
    column-gap: 8px;
    align-items: center;
    padding: 7px 9px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-chip);
    color: var(--text-secondary);
    text-align: left;
    cursor: pointer;
  }
  .theme:hover {
    background: var(--bg-hover);
  }
  .theme.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
    color: var(--text-primary);
  }
  .sw {
    grid-row: span 2;
    display: grid;
    grid-template-columns: 9px 9px;
    gap: 2px;
  }
  .sw i {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    box-shadow: inset 0 0 0 1px rgba(127, 127, 127, 0.35);
  }
  .tname {
    font-size: 12px;
    font-weight: 600;
  }
  .twhat {
    font-size: 10.5px;
    font-weight: 400;
    color: var(--text-muted);
  }
  .col > input {
    height: var(--ctl-h);
  }
  .hint {
    font-weight: 400;
    font-size: 11px;
    color: var(--text-muted);
  }
  code {
    font-family: var(--mono);
    color: var(--text-secondary);
  }
  .status {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--bg-input);
    font-size: 11px;
    word-break: break-all;
  }
  .k {
    display: inline-block;
    width: 64px;
    color: var(--text-muted);
  }
  .enc {
    margin-right: 8px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
