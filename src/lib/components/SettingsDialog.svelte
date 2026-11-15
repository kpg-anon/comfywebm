<script lang="ts">
  import { app, splitArgs, REFRESH_RATES } from "$lib/state.svelte";
  import { pickAnyFile, pickFolder } from "$lib/dialogs";
  import * as ipc from "$lib/ipc";
  import {
    CUSTOM,
    CUSTOM_KEYS,
    THEMES,
    applyTheme,
    parseCustom,
    presetColors,
    saveCustom,
    storedCustom,
    storedTheme,
    type CustomKey,
    type CustomTheme,
  } from "$lib/themes";
  import Select from "./Select.svelte";

  // appearance applies at once and is kept per machine (not part of Save)
  let theme = $state(storedTheme());
  let custom = $state<CustomTheme>(storedCustom());
  let themeMsg = $state<string | null>(null);
  function pickTheme(id: string) {
    theme = id;
    applyTheme(id);
  }

  /** Every change to the custom palette shows at once and is kept. */
  function setCustom(next: CustomTheme) {
    custom = next;
    saveCustom($state.snapshot(next) as CustomTheme);
    if (theme === CUSTOM) applyTheme(CUSTOM);
  }

  const setColor = (k: CustomKey, v: string) => setCustom({ ...custom, colors: { ...custom.colors, [k]: v } });

  async function exportTheme() {
    themeMsg = null;
    try {
      const path = await ipc.themeExport(JSON.stringify({ comfywebmTheme: 1, ...$state.snapshot(custom) }, null, 2));
      if (path) themeMsg = `Saved ${path}`;
    } catch (e) {
      themeMsg = ipc.errText(e);
    }
  }

  async function importTheme() {
    themeMsg = null;
    try {
      const text = await ipc.themeImport();
      if (text === null) return;
      const t = parseCustom(JSON.parse(text));
      if (!t) {
        themeMsg = "That file is not a comfywebm theme.";
        return;
      }
      setCustom(t);
      pickTheme(CUSTOM);
      themeMsg = `Imported "${t.name}".`;
    } catch (e) {
      themeMsg = e instanceof SyntaxError ? "That file is not valid JSON." : ipc.errText(e);
    }
  }

  const BROWSERS = ["firefox", "chrome", "chromium", "edge", "brave", "opera", "vivaldi", "whale", "safari"];
  let ytdlpCookies = $state(app.settings.ytdlpCookies ?? "");

  // edit a copy; nothing is saved until Save
  let ffmpegPath = $state(app.settings.ffmpegPath ?? "");
  let outputDir = $state(app.settings.outputDir ?? "");
  let outputMode = $state<"desktop" | "beside" | "folder">(app.settings.outputMode ?? (app.settings.outputDir ? "folder" : "desktop"));
  let ytdlpPath = $state(app.settings.ytdlpPath ?? "");
  let ytdlpArgs = $state(app.settings.ytdlpArgs.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" "));
  let downloadDir = $state(app.settings.downloadDir ?? "");
  let keepDir = $state(app.settings.keepDir ?? "");
  let nameTemplate = $state(app.settings.nameTemplate);
  let cache = $state<ipc.CacheInfo | null>(null);
  let cacheBusy = $state(false);
  let rescanning = $state(false);
  const mib = (b: number) => (b >= 1073741824 ? `${(b / 1073741824).toFixed(2)} GiB` : `${(b / 1048576).toFixed(1)} MiB`);
  ipc.cacheInfo().then((c) => (cache = c)).catch(() => {});

  async function clearCache(downloads: boolean) {
    cacheBusy = true;
    try {
      await ipc.cacheClear(downloads);
      cache = await ipc.cacheInfo();
    } catch (e) {
      msg = ipc.errText(e);
    } finally {
      cacheBusy = false;
    }
  }

  async function rescan() {
    rescanning = true;
    try {
      app.tools = await ipc.toolsRescan();
    } catch (e) {
      msg = ipc.errText(e);
    } finally {
      rescanning = false;
    }
  }

  let msg = $state<string | null>(null);
  let saving = $state(false);



  async function save() {
    saving = true;
    const err = await app.saveSettings({
      ...app.settings,
      ffmpegPath: ffmpegPath.trim() || null,
      outputDir: outputDir.trim() || null,
      outputMode: outputMode === "folder" && !outputDir.trim() ? "desktop" : outputMode,
      ytdlpPath: ytdlpPath.trim() || null,
      ytdlpArgs: splitArgs(ytdlpArgs),
      downloadDir: downloadDir.trim() || null,
      keepDir: keepDir.trim() || null,
      ytdlpCookies: ytdlpCookies || null,
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
        <button class="theme" class:on={theme === CUSTOM} role="radio" aria-checked={theme === CUSTOM} onclick={() => pickTheme(CUSTOM)}>
          <span class="sw">{#each ["bg-panel", "accent", "primary", "range"] as const as k (k)}<i style:background={custom.colors[k]}></i>{/each}</span>
          <span class="tname">Custom: {custom.name}</span>
          <span class="twhat">Your own colors; import and export as a file</span>
        </button>
      </div>
    </div>

    {#if theme === CUSTOM}
      <div class="custom">
        <div class="line">
          <input class="field" type="text" aria-label="Theme name" placeholder="Theme name" value={custom.name} onchange={(e) => setCustom({ ...custom, name: (e.currentTarget as HTMLInputElement).value.trim() || "My theme" })} />
          <Select
            label="Start from a preset"
            value={null}
            placeholder="Start from..."
            options={THEMES.map((t) => ({ value: t.id as string | null, label: t.name }))}
            onchange={(id) => id && setCustom({ ...custom, colors: presetColors(id), light: false })}
          />
        </div>
        <div class="colors">
          {#each CUSTOM_KEYS as [k, label] (k)}
            <label class="color">
              <input type="color" value={custom.colors[k]} oninput={(e) => setColor(k, (e.currentTarget as HTMLInputElement).value)} />
              <span>{label}</span>
            </label>
          {/each}
        </div>
        <div class="line opts">
          <label class="color" title="Light themes tell the system to draw scrollbars and the color picker in light colors">
            <input type="checkbox" checked={custom.light} onchange={(e) => setCustom({ ...custom, light: (e.currentTarget as HTMLInputElement).checked })} /> Light
          </label>
          <label class="color">
            <input type="checkbox" checked={custom.mono} onchange={(e) => setCustom({ ...custom, mono: (e.currentTarget as HTMLInputElement).checked })} /> Monospace text
          </label>
          <label class="color radius">
            Corners
            <input type="range" min="0" max="16" step="1" value={custom.radius} oninput={(e) => setCustom({ ...custom, radius: Number((e.currentTarget as HTMLInputElement).value) })} />
            <span class="mono">{custom.radius}px</span>
          </label>
        </div>
        <div class="line">
          <button class="btn small" onclick={importTheme}>Import...</button>
          <button class="btn small" onclick={exportTheme}>Export...</button>
          {#if themeMsg}<span class="hint">{themeMsg}</span>{/if}
        </div>
      </div>
    {/if}

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
        <div><button class="btn small" disabled={rescanning} title="Search for ffmpeg, ffprobe and yt-dlp again, for example after installing or updating them" onclick={rescan}>{rescanning ? "Searching..." : "Search again"}</button></div>
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

    <div class="col">
      <span>yt-dlp cookies from browser</span>
      <div class="line">
        <Select
          label="Browser to take cookies from"
          value={ytdlpCookies}
          onchange={(v) => (ytdlpCookies = v)}
          options={[{ value: "", label: "None" }, ...BROWSERS.map((b) => ({ value: b, label: b[0].toUpperCase() + b.slice(1) }))]}
        />
      </div>
      <span class="hint">For videos that need a sign-in (age-restricted, private, members only): yt-dlp reads that browser's cookies for the site. Some Chromium-based browsers lock or encrypt their cookies so yt-dlp cannot read them; Firefox usually works. Adds <code>--cookies-from-browser</code>.</span>
    </div>

    <label class="col">
      <span>Extra yt-dlp arguments</span>
      <input class="field mono" type="text" spellcheck="false" placeholder="for example --sleep-requests 1" bind:value={ytdlpArgs} />
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

    <div class="col">
      <span>Output folder</span>
      <div class="line">
        <Select
          label="Output folder"
          value={outputMode}
          onchange={(v) => (outputMode = v)}
          options={[
            { value: "desktop", label: "Desktop" },
            { value: "beside", label: "Beside the source file" },
            { value: "folder", label: "This folder:" },
          ]}
        />
        <input class="field mono" type="text" spellcheck="false" placeholder="a folder" disabled={outputMode !== "folder"} bind:value={outputDir} />
        <button class="btn small" onclick={async () => { const d = await pickFolder("Output folder"); if (d) { outputDir = d; outputMode = "folder"; } }}>Choose</button>
      </div>
      <span class="hint">Where encoded files and saved frames go. Also set from the Output group in the sidebar.</span>
    </div>

    <label class="col">
      <span>File name template</span>
      <input class="field mono" type="text" spellcheck="false" bind:value={nameTemplate} />
      <span class="hint">Tokens: <code>{"{name}"}</code> <code>{"{start}"}</code> <code>{"{end}"}</code> <code>{"{codec}"}</code> <code>{"{height}"}</code> <code>{"{date}"}</code>. The extension (.webm or .mp4) is added. Existing files are never overwritten.</span>
    </label>

    <div class="col">
      <span>Cache</span>
      <div class="line cacheline">
        <span class="mono muted">{cache ? `${mib(cache.bytes)} in ${cache.files} files` : "..."}</span>
        <button class="btn small" disabled={cacheBusy || !cache?.files} title="Delete preview copies, samples and timeline pictures. They are made again when needed." onclick={() => clearCache(false)}>Clear</button>
        {#if cache && cache.downloadBytes > 0}
          <span class="mono muted">downloads {mib(cache.downloadBytes)}</span>
          <button class="btn small danger" disabled={cacheBusy} title="Also delete videos downloaded with &quot;Download and edit&quot; into the cache folder" onclick={() => clearCache(true)}>Clear downloads</button>
        {/if}
      </div>
      <span class="hint">Preview copies of files the player cannot show, samples and timeline pictures{cache ? ` (${cache.dir})` : ""}. Downloads saved with "Download only" are never in the cache.</span>
    </div>

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
  .cacheline {
    align-items: center;
    gap: 8px;
    font-weight: 400;
  }
  .line input {
    flex: 1;
    height: var(--ctl-h);
  }
  .line :global(.sel) {
    width: 140px;
    height: var(--ctl-h);
  }
  .custom {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: var(--bw) solid var(--border-muted);
    border-radius: var(--radius);
    background: var(--bg-panel-alt);
  }
  .custom .line :global(.sel) {
    width: 150px;
  }
  .colors {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px 12px;
  }
  .color {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    font-weight: 400;
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
  .opts {
    gap: 16px;
    flex-wrap: wrap;
  }
  .radius input {
    width: 100px;
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
