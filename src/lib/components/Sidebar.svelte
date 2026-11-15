<script lang="ts">
  // Right panel. Preset and Rate stay at the top, the summary and Encode
  // buttons at the bottom. In between, two tabs: Basic (output, resolution,
  // motion, audio, file) and Advanced (encoder, filters, audio level,
  // subtitles, metadata, the ffmpeg command). The Advanced tab shows how many
  // of its settings are changed, so nothing set there goes unnoticed.
  import { app } from "$lib/state.svelte";
  import { pickFolder, pickSubtitle } from "$lib/dialogs";
  import { CODECS_BY_CONTAINER, CODEC_LABEL } from "$lib/presets";
  import { fmtBytes, fmtTime } from "$lib/time";
  import type { Rotate } from "$lib/types";
  import Group from "./Group.svelte";
  import Icon from "./Icon.svelte";
  import MotionControl from "./MotionControl.svelte";
  import PresetPicker from "./PresetPicker.svelte";
  import ResolutionPicker from "./ResolutionPicker.svelte";
  import Seg from "./Seg.svelte";
  import Select from "./Select.svelte";
  import SizeField from "./SizeField.svelte";

  const AUDIO_RATES = [32, 48, 64, 80, 96, 112, 128, 160, 192, 256];

  const codecs = $derived(
    CODECS_BY_CONTAINER[app.container].map((c) => ({ value: c, label: CODEC_LABEL[c], disabled: !app.codecAvailable(c) })),
  );
  const zscale = $derived(!app.tools || app.tools.filters.zscale);

  let copied = $state(false);

  const warnings = $derived([...app.uiWarnings, ...(app.plan?.warnings ?? [])]);

  function trackLabel(t: { track: number; codec: string; lang: string | null; title: string | null }, extra = "") {
    return [`#${t.track + 1}`, t.codec, extra, t.lang, t.title].filter(Boolean).join(" · ");
  }

  async function copyCmd() {
    if (!app.plan) return;
    try {
      await navigator.clipboard.writeText(app.plan.command);
      copied = true;
      setTimeout(() => (copied = false), 1200);
    } catch {
      copied = false;
    }
  }

  async function chooseFolder() {
    const d = await pickFolder("Output folder");
    if (d) await app.saveSettings({ ...app.settings, outputDir: d });
  }

  async function chooseSubs() {
    const f = await pickSubtitle();
    if (f) {
      app.subsFile = f;
      app.subsKind = "file";
    }
  }

  const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value) || 0;

  const audioOptions = $derived([
    { value: "off", label: "Off" },
    ...(app.media ? app.media.audio.map((a) => ({ value: String(a.track), label: trackLabel(a, `${a.channels} ch`) })) : [{ value: "0", label: "Track 1" }]),
  ]);

  const subsOptions = $derived([
    { value: "off", label: "Off" },
    ...(app.media?.subs ?? []).map((s) => ({
      value: `t${s.track}`,
      label: trackLabel(s),
      hint: s.textBased ? "" : "image-based, not supported",
      disabled: !s.textBased,
    })),
    { value: "file", label: "From file..." },
  ]);

  function setSubs(v: string) {
    if (v === "off") app.subsKind = "off";
    else if (v === "file") {
      app.subsKind = "file";
      if (!app.subsFile) void chooseSubs();
    } else {
      app.subsKind = "embedded";
      app.subsTrack = Number(v.slice(1));
    }
  }

  const hdrAuto = $derived(!!app.media?.video.hdr && zscale);
  /** Advanced settings that differ from what a fresh file gets. */
  const changed = $derived(
    [
      app.deinterlace !== !!app.media?.video.interlaced && "deinterlace",
      app.denoise && "denoise",
      app.tonemap !== hdrAuto && "tone map",
      app.rotate !== 0 && `${app.rotate}°`,
      app.mirror && "mirror",
      app.playback !== "forward" && app.playback,
      (app.fadeIn > 0 || app.fadeOut > 0) && "fade",
      app.audioOn && app.volumeDb !== 0 && `${app.volumeDb > 0 ? "+" : ""}${app.volumeDb} dB`,
      app.audioOn && app.normalize && "normalize",
      app.subsKind !== "off" && "subtitles",
      app.title.trim() && "title",
      app.extraArgs.trim() && "extra args",
      app.effort !== "balanced" && `${app.effort} effort`,
      app.rateMode === "crf" && app.maxBitrateK && "bitrate cap",
    ].filter((x): x is string => !!x),
  );

  const filterNote = $derived(
    [
      app.deinterlace && "deinterlace",
      app.denoise && "denoise",
      app.tonemap && "tone map",
      app.rotate && `${app.rotate}°`,
      app.mirror && "mirror",
      app.playback !== "forward" && app.playback,
      (app.fadeIn > 0 || app.fadeOut > 0) && "fade",
    ]
      .filter(Boolean)
      .join(", "),
  );
</script>

<aside class="sidebar">
  <div class="fixed top">
    <PresetPicker />
    <Seg
      label="Rate mode"
      value={app.rateMode}
      onchange={(v) => (app.rateMode = v)}
      options={[{ value: "size", label: "Size limit" }, { value: "crf", label: `Quality (CRF)` }]}
    />
    {#if app.rateMode === "size"}
      <div class="row">
        <span>Size limit</span>
        <span class="unit"><SizeField bind:bytes={app.maxBytes} /><span class="muted">MiB</span></span>
      </div>
      <div class="muted small">{app.maxBytes.toLocaleString("en-US")} bytes · two passes · aims for 99% of the limit</div>
    {:else}
      <div class="row">
        <span>CRF</span>
        <input type="range" min={app.crfRange[0]} max={app.crfRange[1]} step="1" bind:value={app.crf} aria-label="CRF" />
        <span class="mono crf">{app.crf}</span>
      </div>
      <div class="ends muted small"><span>better, larger ({app.crfRange[0]})</span><span>worse, smaller ({app.crfRange[1]})</span></div>
    {/if}
  </div>

  <div class="tabs" role="tablist">
    <button class="tab" class:on={app.tab === "basic"} role="tab" aria-selected={app.tab === "basic"} onclick={() => app.setTab("basic")}>Basic</button>
    <button
      class="tab"
      class:on={app.tab === "advanced"}
      role="tab"
      aria-selected={app.tab === "advanced"}
      title={changed.length ? `Changed: ${changed.join(", ")}` : "Nothing changed here"}
      onclick={() => app.setTab("advanced")}
    >
      Advanced{#if changed.length}<span class="count">{changed.length}</span>{/if}
    </button>
  </div>

  <div class="scroll">
    {#if app.tab === "basic"}
      <Group id="output" title="Video" note="{app.container.toUpperCase()} · {CODEC_LABEL[app.codec]}">
        <div class="row">
          <span>Format</span>
          <Seg label="Container" value={app.container} onchange={(v) => app.setContainer(v)} options={[{ value: "webm", label: "WebM" }, { value: "mp4", label: "MP4" }]} />
        </div>
        <div class="row">
          <span>Codec</span>
          <Seg label="Codec" value={app.codec} onchange={(v) => app.setCodec(v)} options={codecs} />
        </div>
        <ResolutionPicker />
        <div class="row">
          <span>Crop</span>
          {#if app.crop}
            <span class="mono small">{app.crop.w}x{app.crop.h} at {app.crop.x},{app.crop.y}</span>
            <button class="btn small" onclick={() => app.setCrop(null)}>Clear</button>
          {:else}
            <span class="muted small grow">none</span>
            <button class="btn small" disabled={!app.media} onclick={() => (app.cropMode = true)}>Draw <kbd>C</kbd></button>
          {/if}
        </div>
      </Group>

      <Group id="motion" title="Motion" note="{app.speed !== 1 ? `${app.speed}x · ` : ''}{app.media ? `${Math.round(app.outFps * 100) / 100} fps` : ''}">
        <MotionControl />
      </Group>

      <Group id="audio" title="Audio" note={app.audioOn ? `${app.audioCodecName} ${app.audioBitrateK}k` : "off"}>
        {#if app.media && app.media.audio.length === 0}
          <div class="muted small">The source has no audio.</div>
        {:else}
          <div class="row">
            <span>Track</span>
            <Select
              label="Audio track"
              value={app.audioOn ? String(app.audioTrack) : "off"}
              options={audioOptions}
              onchange={(v) => {
                if (v === "off") app.audioOn = false;
                else {
                  app.audioOn = true;
                  app.audioTrack = Number(v);
                }
              }}
            />
          </div>
          {#if app.audioOn}
            <div class="row">
              <span>Bitrate ({app.audioCodecName})</span>
              <Select label="Audio bitrate" bind:value={app.audioBitrateK} options={AUDIO_RATES.map((r) => ({ value: r, label: `${r} kbps` }))} />
            </div>
            {#if app.playback === "boomerang"}<div class="muted small">Boomerang output has no audio.</div>{/if}
          {/if}
        {/if}
      </Group>

      <Group id="file" title="File" note={app.outputName || ""}>
        <label class="col">
          <span>File name</span>
          <input class="field" type="text" spellcheck="false" placeholder={app.settings.nameTemplate} bind:value={app.outputName} />
        </label>
        <div class="row">
          <span>Folder</span>
          <span class="path small" class:muted={!app.settings.outputDir}>{app.settings.outputDir ?? "Beside the source file"}</span>
        </div>
        <div class="btns">
          <button class="btn small" onclick={chooseFolder}><Icon name="open" size={12} />Change</button>
          {#if app.settings.outputDir}
            <button class="btn small" onclick={() => app.saveSettings({ ...app.settings, outputDir: null })}>Beside source</button>
          {/if}
        </div>
      </Group>
    {:else}
      <Group id="encoder" title="Encoder" note={app.effort}>
        <div class="row">
          <span>Effort</span>
          <Seg
            label="Effort"
            bind:value={app.effort}
            options={[
              { value: "fast", label: "Fast" },
              { value: "balanced", label: "Balanced" },
              { value: "best", label: "Best" },
            ]}
          />
        </div>
        <label class="row">
          <span>Picture limit</span>
          <span class="unit">
            <input class="field mono w" type="number" min="0" step="2" placeholder="width" aria-label="Picture limit width" bind:value={app.maxWidth} />
            <span class="muted">x</span>
            <input class="field mono w" type="number" min="0" step="2" placeholder="height" aria-label="Picture limit height" bind:value={app.maxHeight} />
          </span>
        </label>
        <div class="muted small">The largest picture the destination accepts (set by the preset). Resolution choices stay inside it.</div>
        {#if app.rateMode === "crf"}
          <label class="row">
            <span>Max bitrate</span>
            <span class="unit">
              <input class="field mono" type="number" min="0" step="50" placeholder="none" bind:value={app.maxBitrateK} />
              <span class="muted">kbps</span>
            </span>
          </label>
          <label class="check"><input type="checkbox" bind:checked={app.twoPass} /> Two passes</label>
        {/if}
      </Group>

      <Group id="filters" title="Filters" note={filterNote}>
        <div class="checks">
          <label class="check"><input type="checkbox" bind:checked={app.deinterlace} /> Deinterlace</label>
          <label class="check"><input type="checkbox" bind:checked={app.denoise} /> Denoise</label>
          <label class="check" class:off={!zscale}>
            <input type="checkbox" bind:checked={app.tonemap} disabled={!zscale} /> Tone map HDR
          </label>
          <label class="check"><input type="checkbox" bind:checked={app.mirror} /> Mirror</label>
        </div>
        {#if app.media?.video.interlaced}<div class="muted small">The source is interlaced.</div>{/if}
        {#if app.media?.video.hdr}<div class="muted small">The source is HDR.</div>{/if}
        {#if !zscale}<div class="muted small">Tone mapping needs an ffmpeg build with the zscale filter.</div>{/if}
        <div class="row">
          <span>Rotate</span>
          <Seg
            label="Rotate"
            value={app.rotate}
            onchange={(v) => (app.rotate = v as Rotate)}
            options={[
              { value: 0, label: "0°" },
              { value: 90, label: "90°" },
              { value: 180, label: "180°" },
              { value: 270, label: "270°" },
            ]}
          />
        </div>
        <div class="row">
          <span>Direction</span>
          <Seg
            label="Playback direction"
            bind:value={app.playback}
            options={[
              { value: "forward", label: "Forward" },
              { value: "reverse", label: "Reverse" },
              { value: "boomerang", label: "Boomerang" },
            ]}
          />
        </div>
        <div class="row">
          <span>Fade</span>
          <span class="unit">
            <span class="muted small">in</span>
            <input class="field mono s" type="number" min="0" step="0.25" aria-label="Fade in seconds" value={app.fadeIn} onchange={(e) => (app.fadeIn = Math.max(0, num(e)))} />
            <span class="muted small">out</span>
            <input class="field mono s" type="number" min="0" step="0.25" aria-label="Fade out seconds" value={app.fadeOut} onchange={(e) => (app.fadeOut = Math.max(0, num(e)))} />
            <span class="muted small">s</span>
          </span>
        </div>
      </Group>

      <Group id="audiolevel" title="Audio level" note={app.audioOn ? `${app.volumeDb} dB${app.normalize ? ", normalized" : ""}` : "audio off"}>
        {#if !app.audioOn}
          <div class="muted small">Audio is off (Basic tab).</div>
        {:else}
          <label class="row">
            <span>Volume</span>
            <span class="unit">
              <input class="field mono s" type="number" min="-30" max="30" step="0.5" value={app.volumeDb} onchange={(e) => (app.volumeDb = Math.min(30, Math.max(-30, num(e))))} />
              <span class="muted small">dB</span>
            </span>
          </label>
          <label class="check"><input type="checkbox" bind:checked={app.normalize} /> Normalize loudness</label>
        {/if}
      </Group>

      <Group id="subs" title="Subtitles" note={app.subsKind === "off" ? "off" : "burned in"}>
        <Select label="Subtitles" value={app.subsKind === "embedded" ? `t${app.subsTrack}` : app.subsKind} options={subsOptions} onchange={setSubs} />
        {#if app.subsKind === "file"}
          <div class="row">
            <span class="path small" class:muted={!app.subsFile}>{app.subsFile ? baseName(app.subsFile) : "No file chosen"}</span>
            <button class="btn small" onclick={chooseSubs}>Choose</button>
          </div>
        {/if}
      </Group>

      <Group id="meta" title="Metadata and arguments">
        <label class="col">
          <span>Title metadata</span>
          <input class="field" type="text" spellcheck="false" placeholder="none" bind:value={app.title} />
        </label>
        <label class="col">
          <span>Extra ffmpeg arguments</span>
          <input class="field mono" type="text" spellcheck="false" placeholder="none" bind:value={app.extraArgs} />
        </label>
      </Group>

      <Group id="command" title="ffmpeg command">
        {#if app.plan}
          <textarea class="cmd mono" readonly rows="6" value={app.plan.command}></textarea>
          <button class="btn small cmdcopy" onclick={copyCmd}><Icon name={copied ? "check" : "copy"} size={12} />{copied ? "Copied" : "Copy"}</button>
        {:else}
          <div class="muted small">Open a video to see the ffmpeg command.</div>
        {/if}
      </Group>
    {/if}
  </div>

  <div class="fixed bottom">
    {#if app.plan}
      <div class="summary">
        <span class="mono small">
          {app.plan.outWidth}x{app.plan.outHeight}{app.plan.autoScaled ? " (auto)" : ""} · {fmtTime(app.plan.durationOut)}
        </span>
        {#if app.plan.quality}
          <span class="badge {app.plan.quality}">{app.plan.quality}</span>
        {/if}
      </div>
      <div class="mono small muted">
        video {app.plan.videoKbps === null ? "CRF" : `${Math.round(app.plan.videoKbps)} kbps`}
        · audio {app.plan.audioKbps > 0 ? `${Math.round(app.plan.audioKbps)} kbps` : "off"}
        {#if app.plan.bitsPerPixel !== null}· {app.plan.bitsPerPixel.toFixed(3)} bpp{/if}
        {#if app.rateMode === "size"}· at most {fmtBytes(app.maxBytes)}{/if}
        {#if app.separate && app.segments.length > 1}· {app.segments.length} files{/if}
      </div>
    {:else if app.planError}
      <div class="err small">{app.planError}</div>
    {:else if !app.media}
      <div class="muted small">Open a video to see the output size.</div>
    {/if}
    {#if warnings.length}
      <div class="warnings">
        {#each warnings as w, i (i)}
          <div class="warn small wline"><Icon name="warn" size={12} /><span>{w}</span></div>
        {/each}
      </div>
    {/if}
    <div class="actions">
      <button class="btn primary big" disabled={!app.canEncode} onclick={() => app.enqueue(true)}>Encode <kbd>Ctrl+Enter</kbd></button>
      <button class="btn big" disabled={!app.canEncode} onclick={() => app.enqueue(false)}>Add to queue</button>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    width: 330px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--border-muted);
    background: var(--sidebar-bg);
  }
  .fixed {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px var(--pad-panel);
  }
  .bottom {
    border-top: 1px solid var(--border);
  }
  .tabs {
    flex: none;
    display: flex;
    gap: 2px;
    padding: 0 var(--pad-panel);
    border-bottom: 1px solid var(--border);
  }
  .tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border: 0;
    background: none;
    color: var(--text-muted);
    font-family: var(--font-head);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: var(--head-track);
    text-transform: var(--head-case);
    cursor: pointer;
  }
  .tab:hover {
    color: var(--text-primary);
  }
  .tab.on {
    color: var(--accent-strong);
  }
  .tab.on::after {
    content: "";
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: -1px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
  }
  .count {
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--text-inverse);
    font-size: 10px;
    line-height: 16px;
    text-align: center;
    letter-spacing: 0;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--pad-panel);
  }
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
  }
  .row :global(.seg) {
    flex: 1;
    max-width: 230px;
  }
  .row input[type="range"] {
    flex: 1;
  }
  .grow {
    flex: 1;
  }
  .crf {
    width: 22px;
    text-align: right;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .unit {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .unit input {
    width: 90px;
  }
  .unit input.w {
    width: 72px;
  }
  .unit input.s {
    width: 58px;
  }
  .checks {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  .check.off {
    opacity: 0.55;
  }
  .ends {
    display: flex;
    justify-content: space-between;
    margin-top: -4px;
  }
  .small {
    font-size: 11px;
  }
  .path {
    min-width: 0;
    word-break: break-all;
    text-align: right;
  }
  .btns {
    display: flex;
    gap: 4px;
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .badge {
    padding: 0 6px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .badge.good {
    background: color-mix(in srgb, var(--green) 20%, transparent);
    color: var(--green);
  }
  .badge.fair {
    background: color-mix(in srgb, var(--yellow) 20%, transparent);
    color: var(--yellow);
  }
  .badge.poor {
    background: color-mix(in srgb, var(--red) 20%, transparent);
    color: var(--red);
  }
  .warnings {
    max-height: 88px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .wline {
    display: flex;
    gap: 5px;
    align-items: flex-start;
  }
  .wline :global(svg) {
    margin-top: 2px;
  }
  .cmd {
    width: 100%;
    resize: vertical;
    padding: 6px;
    border: var(--bw) solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    font-size: 11px;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .cmdcopy {
    align-self: flex-start;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }
  .big {
    flex: 1;
    height: calc(var(--ctl-h) + 6px);
    font-size: 13px;
  }
  .big kbd {
    font-size: 10px;
    background: transparent;
    border-color: color-mix(in srgb, var(--text-inverse) 40%, transparent);
    color: inherit;
  }
  .btn kbd {
    font-size: 10px;
    padding: 0 3px;
  }
</style>
