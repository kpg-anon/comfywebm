<script lang="ts">
  import { app, splitArgs, REFRESH_RATES, SIZE_TARGET_DEFAULT, fmtPct } from "$lib/state.svelte";
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
  import ToolInstall from "./ToolInstall.svelte";
  import { ffmpegInstall } from "$lib/toolinstall";
  import type { Settings, StabMethod } from "$lib/types";
  import { updater } from "$lib/updater.svelte";
  import { getVersion } from "@tauri-apps/api/app";

  let appVersion = $state("");
  getVersion().then((v) => (appVersion = v)).catch(() => {});

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

  // --- the settings file: import, export, open, reload ---
  let fileMsg = $state<string | null>(null);

  /** Appearance values kept in this webview (not in settings.json). */
  const lookNow = () => ({
    theme: storedTheme(),
    customTheme: storedCustom(),
    uiScale: app.uiScale,
    refreshHz: app.refreshHz,
    timelineFilm: app.showFilm,
    timelineWave: app.showWave,
    timelineExact: app.exactThumbs,
    showRecent: app.showRecent,
    sideResize: app.sideResize,
    timelineResize: app.timelineResize,
    cutSensitivity: app.cutSensitivity,
    updateCheck: updater.auto,
  });

  function applyLook(v: unknown) {
    if (!v || typeof v !== "object") return;
    const o = v as Record<string, unknown>;
    const c = parseCustom(o.customTheme);
    if (c) setCustom(c);
    if (typeof o.theme === "string") pickTheme(o.theme);
    if (typeof o.uiScale === "number" && o.uiScale >= 0.5 && o.uiScale <= 2) app.setUiScale(o.uiScale);
    if (typeof o.refreshHz === "number" && REFRESH_RATES.includes(o.refreshHz)) app.setRefreshHz(o.refreshHz);
    if (typeof o.timelineFilm === "boolean" && typeof o.timelineWave === "boolean") app.setTimelinePics(o.timelineFilm, o.timelineWave);
    if (typeof o.timelineExact === "boolean") app.setExactThumbs(o.timelineExact);
    if (typeof o.showRecent === "boolean") app.setShowRecent(o.showRecent);
    if (typeof o.sideResize === "boolean") app.setSideResize(o.sideResize);
    if (typeof o.timelineResize === "boolean") app.setTimelineResize(o.timelineResize);
    if (o.cutSensitivity === "low" || o.cutSensitivity === "normal" || o.cutSensitivity === "high") app.setCutSensitivity(o.cutSensitivity);
    if (typeof o.updateCheck === "boolean") updater.setAuto(o.updateCheck);
  }

  /** Puts saved settings into the app and into this dialog's fields. */
  async function useSettings(s: Settings) {
    app.settings = s;
    ffmpegPath = s.ffmpegPath ?? "";
    outputDir = s.outputDir ?? "";
    outputMode = s.outputMode ?? (s.outputDir ? "folder" : "desktop");
    ytdlpPath = s.ytdlpPath ?? "";
    ytdlpArgs = s.ytdlpArgs.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" ");
    downloadDir = s.downloadDir ?? "";
    keepDir = s.keepDir ?? "";
    nameTemplate = s.nameTemplate;
    ytdlpCookies = s.ytdlpCookies ?? "";
    await app.refreshTools();
  }

  async function fileAction(f: () => Promise<string | null>) {
    fileMsg = null;
    try {
      fileMsg = await f();
    } catch (e) {
      fileMsg = ipc.errText(e);
    }
  }

  const exportSettings = () =>
    fileAction(async () => {
      const p = await ipc.settingsExport($state.snapshot(lookNow()));
      return p ? `Saved ${p}` : null;
    });

  const importSettings = () =>
    fileAction(async () => {
      const r = await ipc.settingsImport();
      if (!r) return null;
      await useSettings(r.settings);
      applyLook(r.look);
      return `Imported and saved ${r.path}.`;
    });

  const reloadSettings = () =>
    fileAction(async () => {
      await useSettings(await ipc.settingsReload());
      return "Read settings.json again.";
    });

  const BROWSERS = ["firefox", "chrome", "chromium", "edge", "brave", "opera", "vivaldi", "whale", "safari"];
  let ytdlpCookies = $state(app.settings.ytdlpCookies ?? "");

  // edit a copy; nothing is saved until Save
  let ffmpegPath = $state(app.settings.ffmpegPath ?? "");
  let outputDir = $state(app.settings.outputDir ?? "");
  let outputMode = $state<"desktop" | "beside" | "folder">(app.settings.outputMode ?? (app.settings.outputDir ? "folder" : "desktop"));
  let ytdlpPath = $state(app.settings.ytdlpPath ?? "");
  // a saved location that does not hold the tool falls through to beside the app and PATH;
  // say so instead of showing the other copy as if it came from the setting
  const norm = (x: string) => x.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
  function notFromSetting(found: string | null | undefined, set: string | null | undefined): boolean {
    if (!found || !set?.trim()) return false;
    const f = norm(found), s = norm(set.trim());
    if (f === s || f.startsWith(s + "/")) return false;
    // a file was set (the tool itself, or a file in the tool's folder)
    const slash = s.lastIndexOf("/");
    return !(/\.[a-z0-9]+$|ffmpeg$|yt-dlp$/.test(s.slice(slash + 1)) && f.startsWith(s.slice(0, slash + 1)));
  }
  let ytdlpArgs = $state(app.settings.ytdlpArgs.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" "));
  let downloadDir = $state(app.settings.downloadDir ?? "");
  let keepDir = $state(app.settings.keepDir ?? "");
  let nameTemplate = $state(app.settings.nameTemplate);
  // a missing tool opens the Tools tab, where its path is set
  let section = $state<"look" | "tools" | "files">(app.ffmpegMissing ? "tools" : "look");
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

  // Stabilize's method: "auto" = vid.stab when this ffmpeg has it, else deshake
  let stabChoice = $state<string>(app.settings.stabMethod ?? "auto");
  // Follow > Track's tracker; the Python ones count once installed (src-tauri install.rs)
  let trackChoice = $state<string>(app.settings.trackMethod ?? "builtin");
  let trackHave = $state<{ vittrack: boolean; sam2: boolean }>({ vittrack: false, sam2: false });
  async function refreshTrackers() {
    const [v, s] = await Promise.all([ipc.toolInstalled("vittrack").catch(() => null), ipc.toolInstalled("sam2").catch(() => null)]);
    trackHave = { vittrack: !!v, sam2: !!s };
  }
  void refreshTrackers();
  let trackTesting = $state(false);
  let trackResult = $state<{ method: string; text: string; bad: boolean } | null>(null);
  const trackName = (m: string) => (m === "vittrack" ? "VitTrack" : m === "sam2" ? "SAM 2" : "Built-in");
  async function testTrack() {
    const m = trackChoice;
    trackTesting = true;
    trackResult = null;
    try {
      const r = await ipc.trackTest(m);
      trackResult = {
        method: m,
        text: `${trackName(m)}: ${r.frames} frames in ${r.seconds.toFixed(1)} s; the tracked centre was ${r.meanPx.toFixed(1)} px from the square's on average, ${r.maxPx.toFixed(0)} px at most.`,
        bad: r.meanPx > 20,
      };
    } catch (e) {
      trackResult = { method: m, text: `${trackName(m)}: ${ipc.errText(e)}`, bad: true };
    } finally {
      trackTesting = false;
    }
  }
  const hasVidstab = $derived(!!app.tools?.filters.vidstab);
  const hasGyroflow = $derived(!!app.settings.gyroflowPath);
  /** the method shown and tested: Gyroflow as chosen even before it is installed (the install button shows) */
  const stabUsed = $derived<StabMethod>(
    stabChoice === "gyroflow" ? "gyroflow" : stabChoice === "vidstab" || (stabChoice === "auto" && hasVidstab) ? (hasVidstab ? "vidstab" : "deshake") : "deshake",
  );
  const methodName = (m: StabMethod) => (m === "vidstab" ? "vid.stab" : m === "gyroflow" ? "Gyroflow" : "deshake");
  let stabTesting = $state(false);
  let stabResult = $state<{ method: StabMethod; text: string; bad: boolean } | null>(null);
  async function testStab() {
    const method = stabUsed;
    stabTesting = true;
    stabResult = null;
    try {
      const r = await ipc.stabTest(method);
      const left = r.before > 0 ? r.after / r.before : 1;
      const pct = Math.round((1 - left) * 100);
      stabResult = {
        method,
        bad: left > 0.7,
        text: `${methodName(method)}: the test clip's shake went from ${r.before.toFixed(1)} to ${r.after.toFixed(1)} (${pct}% less) in ${r.secs.toFixed(1)} s.${left > 0.7 ? " That is little: this ffmpeg's stabilizer may be broken or very weak." : ""}`,
      };
    } catch (e) {
      stabResult = { method, bad: true, text: `The test failed: ${ipc.errText(e)}` };
    } finally {
      stabTesting = false;
    }
  }



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
      stabMethod: stabChoice === "auto" ? null : (stabChoice as StabMethod),
      trackMethod: trackChoice === "builtin" ? null : trackChoice,
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

    <div class="stabs" role="tablist">
      {#each [["look", "Look"], ["tools", "Tools"], ["files", "Files"]] as [k, label] (k)}
        <button class="stab" class:on={section === k} role="tab" aria-selected={section === k} onclick={() => (section = k as typeof section)}>{label}</button>
      {/each}
    </div>

    {#if section === "look"}
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
      <span>Interface size</span>
      <div class="line">
        <Select
          label="Interface size"
          value={app.uiScale}
          onchange={(v) => app.setUiScale(v)}
          options={[0.8, 0.9, 1, 1.1, 1.25, 1.4, 1.5].map((r) => ({ value: r, label: `${Math.round(r * 100)}%` }))}
        />
      </div>
      <span class="hint">Scales the whole window's text and controls, on top of the system's display scaling.</span>
    </div>

    <div class="col">
      <span>Sidebar</span>
      <div class="line cacheline">
        <label><input type="checkbox" checked={app.sideResize} onchange={(e) => app.setSideResize((e.currentTarget as HTMLInputElement).checked)} /> Resizable: drag its left edge to change its width</label>
      </div>
      <span class="hint">Off: the sidebar has its standard width. Double-click the edge to go back to it. Applies at once.</span>
    </div>

    <div class="col">
      <span>Timeline</span>
      <div class="line cacheline">
        <label><input type="checkbox" checked={app.timelineResize} onchange={(e) => app.setTimelineResize((e.currentTarget as HTMLInputElement).checked)} /> Resizable: drag its top edge up to make it taller</label>
      </div>
      <span class="hint">A taller timeline shows bigger thumbnails and waveform. Off: the standard height. Double-click the edge to go back to it.</span>
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

    <div class="col">
      <span>Updates</span>
      <div class="line cacheline">
        <label><input type="checkbox" checked={updater.auto} onchange={(e) => updater.setAuto((e.currentTarget as HTMLInputElement).checked)} /> Check when the app starts</label>
        <button class="btn small" disabled={updater.phase === "checking" || updater.phase === "downloading"} onclick={() => updater.check(true)}>Check now</button>
        {#if updater.phase === "available" && updater.dismissed}
          <button class="btn small primary" onclick={() => (updater.dismissed = false)}>Show v{updater.version}</button>
        {/if}
      </div>
      <span class="hint">
        This is version {appVersion || "..."}.
        {#if updater.phase === "checking"}Checking...
        {:else if updater.phase === "none"}It is the newest version.
        {:else if updater.phase === "available"}Version {updater.version} is available.
        {:else if updater.phase === "error"}The check failed: {updater.error}
        {/if}
        Updates come from the GitHub releases and are checked against the release's signature before they are installed.
      </span>
    </div>

    <div class="col">
      <span>Timeline</span>
      <div class="line cacheline">
        <label><input type="checkbox" checked={app.showFilm} onchange={(e) => app.setTimelinePics((e.currentTarget as HTMLInputElement).checked, app.showWave)} /> Thumbnails</label>
        <label><input type="checkbox" checked={app.showWave} onchange={(e) => app.setTimelinePics(app.showFilm, (e.currentTarget as HTMLInputElement).checked)} /> Audio waveform</label>
      </div>
      <div class="line">
        <Select
          label="Thumbnails from"
          value={app.exactThumbs ? "exact" : "key"}
          onchange={(v) => app.setExactThumbs(v === "exact")}
          options={[
            { value: "exact", label: "Every frame" },
            { value: "key", label: "Key frames only" },
          ]}
        />
      </div>
      <span class="hint">Drawn behind the ranges (the waveform is off to start with, so the thumbnails show clearly). Made once per file in the background and kept in the cache. Applies at once.</span>
      <span class="hint">Every frame: one thumbnail per second, each from the frame at its time; the whole file is decoded (about 3 s for 4 minutes of 1080p), and the key frame thumbnails show until it is done. Key frames only: ready at once, but a file with key frames far apart repeats one picture for several seconds.</span>
    </div>

    <div class="col">
      <span>Find cuts</span>
      <div class="line">
        <Select
          label="Cut sensitivity"
          value={app.cutSensitivity}
          onchange={(v) => app.setCutSensitivity(v)}
          options={[
            { value: "low", label: "Fewer cuts" },
            { value: "normal", label: "Normal" },
            { value: "high", label: "More cuts" },
          ]}
        />
      </div>
      <span class="hint">Fewer: only clear cuts. More: also cuts between similar shots. Found cuts change at once, without reading the video again.</span>
    </div>
    {:else if section === "tools"}
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
        {#if notFromSetting(app.tools.ffmpeg, app.settings.ffmpegPath) || notFromSetting(app.tools.ffprobe, app.settings.ffmpegPath)}
          <div class="err">Not found at the saved ffmpeg location; the copy shown above is used instead.</div>
        {/if}
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
        {#if notFromSetting(app.tools.ytdlp, app.settings.ytdlpPath)}
          <div class="err">Not found at the saved yt-dlp location; the copy shown above is used instead.</div>
        {/if}
        <div><button class="btn small" disabled={rescanning} title="Search for ffmpeg, ffprobe and yt-dlp again, for example after installing or updating them" onclick={rescan}>{rescanning ? "Searching..." : "Search again"}</button></div>
      {:else}
        <div class="muted">Tool status not loaded.</div>
      {/if}
    </div>
    <ToolInstall tool="ffmpeg" label={ffmpegInstall.label} note={ffmpegInstall.note} oninstalled={(p) => (ffmpegPath = p ?? "")} />

    <div class="col">
      <span>Stabilization</span>
      <div class="line stabline">
        <Select
          label="Stabilization method"
          value={stabChoice}
          onchange={(v) => {
            stabChoice = v;
            stabResult = null;
          }}
          options={[
            { value: "auto", label: `Automatic (${hasVidstab ? "vid.stab" : "deshake"})`, hint: "vid.stab when this ffmpeg has it" },
            { value: "vidstab", label: "vid.stab", hint: hasVidstab ? "two passes" : "not in this ffmpeg", disabled: !hasVidstab },
            { value: "deshake", label: "deshake", hint: "one pass" },
            { value: "gyroflow", label: "Gyroflow", hint: hasGyroflow ? "separate program" : "not installed" },
          ]}
        />
        <button class="btn small" disabled={stabTesting || !app.tools?.ffmpeg || (stabUsed === "gyroflow" && !hasGyroflow)} title="Shakes a test picture by a known amount (3 s), steadies it with this method and measures how much shake is left" onclick={testStab}>{stabTesting ? "Testing..." : "Test"}</button>
      </div>
      {#if stabUsed === "gyroflow"}
        <span class="hint">
          <b>Gyroflow</b> (a separate program). Works out how the camera turned in 3D, including the wobble of phone cameras, and renders a steadied copy that the
          encode uses. Slower than vid.stab and needs temporary disk space (about 1 GB per minute of 1080p). Splits use vid.stab or deshake instead.
        </span>
        <ToolInstall tool="gyroflow" label="Install Gyroflow" note={hasGyroflow ? "" : "the latest release from Gyroflow's GitHub, 84 to 153 MB"} oninstalled={() => (stabResult = null)} />
      {:else if stabUsed === "vidstab"}
        <span class="hint">
          <b>vid.stab</b> (used by Stabilize in the Advanced tab). Two passes over each range: first it follows points across the picture to work out how the camera moved, then it smooths
          that path over a second on each side and moves every frame onto it. It handles shifts and turns, and zooms in just enough to keep the moved edges out of the picture.
          The analysis is an extra pass over each range before the encode. Expect slow pans to stay but lag a little behind the camera, and a lot of motion in the picture itself
          (people close to the camera) to be taken for shake now and then. ffmpeg builds from August 2026 on carry vid.stab 1.4, which removed about 90% of the test shake; older
          ones about half.
        </span>
      {:else}
        <span class="hint">
          <b>deshake</b> (used by Stabilize in the Advanced tab). One pass, in every ffmpeg: each frame is compared with the one before and shifted back. It does not turn the
          picture and looks only a short distance, so it takes out small jitter but not a large shake or a tilt; it removed about a quarter of the test shake.
        </span>
      {/if}
      {#if (!hasVidstab && app.tools?.ffmpeg) || (stabResult?.bad && stabResult.method === "vidstab")}
        <span class="hint">
          {hasVidstab ? "This ffmpeg's vid.stab did little in the test." : "This ffmpeg has no vid.stab."} The button installs an ffmpeg with vid.stab 1.4 for
          comfywebm (your other programs keep theirs); Test again afterwards.
        </span>
        <ToolInstall tool="ffmpeg" label={ffmpegInstall.label} note={ffmpegInstall.note} oninstalled={(p) => { ffmpegPath = p ?? ""; stabResult = null; }} />
      {/if}
      {#if stabResult && stabResult.method === stabUsed}
        <span class="hint" class:err={stabResult.bad}>{stabResult.text}</span>
      {/if}
    </div>

    <div class="col">
      <span>Auto color</span>
      <div class="line stabline">
        <Select
          label="White balance method for Auto"
          value={app.wbMethod}
          onchange={(v) => app.setWbMethod(v)}
          options={[
            { value: "combined", label: "Combined", hint: "default, best measured" },
            { value: "greyEdge", label: "Grey-Edge", hint: "van de Weijer 2007" },
            { value: "greyPixels", label: "Grey Pixels", hint: "Yang 2015" },
            { value: "whitePatch", label: "White Patch", hint: "brightest parts are white" },
            { value: "shadesOfGrey", label: "Shades of Grey", hint: "Finlayson 2004" },
            { value: "greyWorld", label: "Grey World", hint: "the average is grey" },
          ]}
        />
      </div>
      <span class="hint">How Adjustments > Auto finds the colour of the light. All are built in. Measured on 11 clips with known casts added (angle from the true light; lower is better, mean / median): Combined 4.6 / 3.6 degrees, Grey-Edge 4.8 / 3.3, Grey Pixels 4.7 / 4.2, White Patch 5.2 / 4.4, Shades of Grey 5.9 / 4.5, Grey World 9.3 / 7.8. A cast within 3.5 degrees is left alone.</span>
    </div>

    <div class="col">
      <span>Tracking</span>
      <div class="line">
        <Select
          label="Tracker for Follow"
          value={trackChoice}
          onchange={(v) => {
            trackChoice = v;
            trackResult = null;
          }}
          options={[
            { value: "builtin", label: "Built-in", hint: "nothing to install" },
            { value: "vittrack", label: "VitTrack", hint: trackHave.vittrack ? "installed" : "not installed" },
            { value: "sam2", label: "SAM 2 *", hint: trackHave.sam2 ? "installed, best results" : "not installed, best results" },
          ]}
        />
        <button
          class="btn small"
          disabled={trackTesting || !app.tools?.ffmpeg || (trackChoice !== "builtin" && !trackHave[trackChoice as "vittrack" | "sam2"])}
          title="Follows a square moved along a known path in a 4 s test clip and measures how far off the tracker was"
          onclick={testTrack}>{trackTesting ? "Testing..." : "Test"}</button
        >
      </div>
      {#if trackChoice === "vittrack"}
        <span class="hint">
          <b>VitTrack</b> (OpenCV's small neural tracker). Follows the box much like the built-in one but keeps hold through turns and changes of size; runs on the
          processor, faster than the built-in one (a 4.6 s 4K clip in 5 s). Its OpenCV also runs Track face on the Follow lane.
        </span>
        <ToolInstall tool="vittrack" label="Install VitTrack" note={trackHave.vittrack ? "" : "a Python environment for comfywebm alone, about 250 MB"} oninstalled={() => void refreshTrackers()} />
      {:else if trackChoice === "sam2"}
        <span class="hint">
          <b>SAM 2</b> (Meta's video segmentation model; the best results). Follows the person's outline, not a box of pixels, so turns, arms and people passing in front throw it off
          least. Fast on an NVIDIA graphics card; without one it takes about 7 s per analysed frame (10 a second of video).
        </span>
        <ToolInstall tool="sam2" label="Install SAM 2" note={trackHave.sam2 ? "" : "PyTorch and SAM 2 in a Python environment for comfywebm alone: about 5 GB with an NVIDIA card, 1 GB without"} oninstalled={() => void refreshTrackers()} />
      {:else}
        <span class="hint"><b>Built-in</b>. Compares the box's picture frame by frame on a small grey copy of the video. Loses a person who turns around or is covered.</span>
      {/if}
      {#if trackChoice !== "builtin" && !trackHave[trackChoice as "vittrack" | "sam2"]}
        <span class="hint">Until it is installed, Track uses the built-in tracker.</span>
      {/if}
      <span class="hint">* best results. This is the tracker for Track box on the Follow lane. Track face needs VitTrack installed (it uses its OpenCV); Track points uses the built-in point tracker.</span>
      {#if trackResult && trackResult.method === trackChoice}
        <span class="hint" class:err={trackResult.bad}>{trackResult.text}</span>
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
    <ToolInstall tool="ytdlp" label="Install yt-dlp" note="the latest release from yt-dlp's GitHub, 23 to 41 MB" oninstalled={(p) => (ytdlpPath = p ?? "")} />

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
    {:else}
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

    <div class="col">
      <span>On start</span>
      <div class="line">
        <Select
          label="On start"
          value={app.startWith}
          onchange={(v) => app.setStartWith(v)}
          options={[
            { value: "preset", label: "Start from the preset" },
            { value: "last", label: "Keep the last options" },
          ]}
        />
      </div>
      <span class="hint">Start from the preset: the app opens with the last used preset's values, and speed, playback, fades, sound volume, stabilize, denoise and the logo back at their defaults. Keep the last options: everything as it was when the app was closed. Applies at the next start.</span>
    </div>

    <div class="col">
      <span>Size limit</span>
      <div class="line cacheline">
        <span>Aim for</span>
        <input class="field mono pct" type="number" min="50" max="100" step="0.1" value={fmtPct(app.sizeTarget)} onchange={(e) => app.setSizeTarget(parseFloat((e.currentTarget as HTMLInputElement).value))} />
        <span>% of the limit</span>
        <button class="btn small" disabled={app.sizeTarget === SIZE_TARGET_DEFAULT} onclick={() => app.setSizeTarget(SIZE_TARGET_DEFAULT)}>Default</button>
      </div>
      <span class="hint">Sound and video together. The default, {fmtPct(SIZE_TARGET_DEFAULT)}%, is what webm.py's <span class="mono">-l</span> aims for. The encoder can land a little over or under the aim; a file over the limit is kept and marked in the queue.</span>
      <label><input type="checkbox" checked={app.sizeRetries} onchange={(e) => app.setSizeRetries((e.currentTarget as HTMLInputElement).checked)} /> Encode again when the file misses the limit</label>
      <span class="hint">On: up to 4 attempts; after the first, they aim for 99% of the limit (the queue shows the attempt). Applies to the next encode.</span>
    </div>

    <div class="col">
      <span>Queue</span>
      <label><input type="checkbox" checked={app.queueAutoStart} onchange={(e) => app.setQueueAutoStart((e.currentTarget as HTMLInputElement).checked)} /> Add to queue starts encoding at once</label>
      <span class="hint">Off: Add to queue (Ctrl+Enter) leaves the job waiting until you press Start in the queue. On: it starts the queue, like Encode. Applies at once.</span>
    </div>

    <label class="col">
      <span>File name template</span>
      <input class="field mono" type="text" spellcheck="false" bind:value={nameTemplate} />
      <span class="hint">Tokens: <code>{"{name}"}</code> <code>{"{start}"}</code> <code>{"{end}"}</code> <code>{"{codec}"}</code> <code>{"{height}"}</code> <code>{"{date}"}</code> <code>{"{preset}"}</code> (the platform and the size limit, as <code>4chan-4MiB</code>). The extension (.webm, .mp4, .gif, .webp, .opus, .mp3 or .m4a) is added. Existing files are never overwritten.</span>
    </label>

    <div class="col">
      <span>Recent files</span>
      <div class="line cacheline">
        <label><input type="checkbox" checked={app.showRecent} onchange={(e) => app.setShowRecent((e.currentTarget as HTMLInputElement).checked)} /> Keep and show recent files</label>
        <button class="btn small" disabled={!app.recent.length} onclick={() => app.clearRecent()}>Clear list</button>
      </div>
      <span class="hint">The last 10 files opened, listed on the start screen. Turning this off also clears the list. Applies at once.</span>
    </div>

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
    {/if}

    <div class="col">
      <span>Settings file</span>
      <div class="line cacheline">
        <button class="btn small" title="Read a settings file exported here or on another computer. Its settings are saved at once; your presets are kept." onclick={importSettings}>Import...</button>
        <button class="btn small" title="Save the settings on all three tabs to one file (presets have their own export)" onclick={exportSettings}>Export...</button>
        <button class="btn small" title="Show settings.json in the file manager" onclick={() => fileAction(async () => (await ipc.settingsOpen("folder"), null))}>Show in folder</button>
        <button class="btn small" title="Open settings.json in the program set for .json files. Press Reload after saving it there." onclick={() => fileAction(async () => (await ipc.settingsOpen("editor"), null))}>Open in editor</button>
        <button class="btn small" title="Read settings.json again after editing it outside the app" onclick={reloadSettings}>Reload</button>
      </div>
      {#if fileMsg}<span class="hint">{fileMsg}</span>{/if}
    </div>

    <!-- a bar on the dialog's bottom edge: stays in view while the settings scroll -->
    <div class="actions">
      {#if msg}<div class="err">{msg}</div>{/if}
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" disabled={saving} onclick={save}>Save</button>
    </div>
  </div>
</div>

<style>
  .stabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--border-muted);
    margin-bottom: 4px;
  }
  .stab {
    padding: 6px 12px;
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--text-secondary);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .stab.on {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
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
  /* the rule above is for text fields; a checkbox keeps its own size, beside its label */
  .line input[type="checkbox"] {
    flex: none;
    height: auto;
    margin: 0;
  }
  .cacheline label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  /* a number, not a text field: its own width */
  .line input.pct {
    flex: none;
    width: 80px;
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
  .stabline :global(.sel) {
    width: 240px;
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
    position: sticky;
    bottom: -16px;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    margin: 0 -18px -16px;
    padding: 10px 18px;
    border-top: 1px solid var(--border-muted);
    background: var(--bg-panel);
  }
  /* the same size as Encode in the sidebar */
  .actions .btn {
    min-width: 96px;
    height: calc(var(--ctl-h) + 6px);
    padding: 0 18px;
    font-size: 13px;
  }
  .actions .err {
    flex: 1;
    min-width: 0;
  }
</style>
