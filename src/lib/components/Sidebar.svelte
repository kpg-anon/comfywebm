<script lang="ts">
  // Right panel. Preset and Rate stay at the top, the summary and Encode
  // buttons at the bottom. In between, two tabs: Basic (output, resolution,
  // motion, audio, file) and Advanced (encoder, filters, audio level,
  // subtitles, metadata, the ffmpeg command). The Advanced tab shows how many
  // of its settings are changed, so nothing set there goes unnoticed.
  import { app, isNeutral } from "$lib/state.svelte";
  import { batch } from "$lib/batch.svelte";
  import { pickFolder, pickImage, pickSubtitle } from "$lib/dialogs";
  import { CODECS_BY_CONTAINER, CODEC_LABEL } from "$lib/presets";
  import { fmtBytes, fmtTime } from "$lib/time";
  import type { Rotate } from "$lib/types";
  import AdjustPanel from "./AdjustPanel.svelte";
  import FileInfo from "./FileInfo.svelte";
  import Group from "./Group.svelte";
  import Icon from "./Icon.svelte";
  import MotionControl from "./MotionControl.svelte";
  import PresetPicker from "./PresetPicker.svelte";
  import ResolutionPicker from "./ResolutionPicker.svelte";
  import Seg from "./Seg.svelte";
  import Select from "./Select.svelte";
  import SizeField from "./SizeField.svelte";


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

  const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

  async function chooseFolder() {
    const d = await pickFolder("Output folder");
    if (d) await app.saveSettings({ ...app.settings, outputDir: d, outputMode: "folder" });
  }

  const outMode = $derived(app.settings.outputMode ?? (app.settings.outputDir ? "folder" : "desktop"));
  const outOptions = $derived([
    { value: "desktop", label: "Desktop" },
    { value: "beside", label: "Beside the source file" },
    { value: "folder", label: app.settings.outputDir ? `Other: ${baseName(app.settings.outputDir)}` : "Other folder..." },
  ]);
  function setOutMode(v: string) {
    if (v === "folder" && !app.settings.outputDir) void chooseFolder();
    else void app.saveSettings({ ...app.settings, outputMode: v as "desktop" | "beside" | "folder" });
  }

  async function chooseSubs() {
    const f = await pickSubtitle();
    if (f) {
      app.subsFile = f;
      app.subsKind = "file";
    }
  }

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
      app.stabilize && "stabilize",
      app.smoothSlowmo && "smooth slow motion",
      app.tonemap !== hdrAuto && "tone map",
      app.rotate !== 0 && `${app.rotate}°`,
      app.mirror && "flip horizontal",
      app.vflip && "flip vertical",
      !isNeutral(app.adjust) && "adjustments",
      app.playback !== "forward" && app.playback,
      (app.fadeIn > 0 || app.fadeOut > 0) && "fade",
      app.audioOn && app.volumeDb !== 0 && `${app.volumeDb > 0 ? "+" : ""}${app.volumeDb} dB`,
      app.audioOn && app.normalize && "normalize",
      app.subsKind !== "off" && "subtitles",
      app.title.trim() && "title",
      app.captionText.trim() && "caption",
      app.logoPath && "logo",
      app.extraArgs.trim() && "extra args",
      app.effort !== "balanced" && `${app.effort} effort`,
      app.rateMode === "crf" && app.maxBitrateK && "bitrate cap",
    ].filter((x): x is string => !!x),
  );

  const filterNote = $derived(
    [
      app.deinterlace && "deinterlace",
      app.denoise && "denoise",
      app.stabilize && "stabilize",
      app.smoothSlowmo && app.speed < 1 && "smooth slow-mo",
      app.tonemap && "tone map",
      app.rotate && `${app.rotate}°`,
      app.mirror && "flip H",
      app.vflip && "flip V",
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
    <label class="copy" title="Copy the video and audio as they are, without encoding: fast and lossless, but the cut starts at a key frame (up to a few seconds early) and crop, resolution, speed and filters do nothing.">
      <input type="checkbox" checked={app.streamCopy} onchange={(e) => app.setStreamCopy((e.currentTarget as HTMLInputElement).checked)} />
      Cut only (no re-encode)
    </label>
    {#if app.streamCopy}
      <div class="muted small">
        {#if app.media && !app.copyContainer()}
          This file's {app.media.video.codec} video cannot be copied into WebM or MP4. Turn this off to encode it.
        {:else}
          The streams are copied into {app.container.toUpperCase()}; the file size is what the cut holds.
        {/if}
      </div>
    {:else}
    <Seg
      label="Rate mode"
      value={app.rateMode}
      onchange={(v) => (app.rateMode = v)}
      options={[{ value: "size", label: "Size limit" }, { value: "crf", label: `Quality (CRF)` }]}
    />
    {#if app.rateMode === "size"}
      <div class="row">
        <span title="The output file is made to fit under this size, in MiB (1 MiB = 1,048,576 bytes). The video bitrate is worked out from it, the length and the audio.">Size limit</span>
        <span class="unit"><SizeField bind:bytes={app.maxBytes} /><span class="muted">MiB</span></span>
      </div>
      {#if app.codec === "gif" || app.codec === "webp"}
        <div class="muted small">{app.maxBytes.toLocaleString("en-US")} bytes · {app.codec === "gif" ? "GIF" : "WebP"} has no bitrate setting, so the picture size is chosen to fit (Auto resolution){app.codec === "webp" ? ", at quality 75" : ""}</div>
      {:else}
        <div class="muted small">{app.maxBytes.toLocaleString("en-US")} bytes · two passes · aims for 99% of the limit</div>
      {/if}
    {:else if app.container === "audio"}
      <div class="muted small">Quality mode uses the bitrate set in the Audio group.</div>
    {:else if app.codec === "gif"}
      <div class="muted small">GIF has no quality setting. Its size comes from the picture size, the frame rate and the length.</div>

    {:else}
      <div class="row">
        <span title="Constant quality: every frame gets the bits it needs for this quality, so the file size depends on the content. Lower = better and larger.">CRF</span>
        <input type="range" min={app.crfRange[0]} max={app.crfRange[1]} step="1" bind:value={app.crf} aria-label="CRF" />
        <span class="mono crf">{app.crf}</span>
      </div>
      <div class="ends muted small"><span>better, larger ({app.crfRange[0]})</span><span>worse, smaller ({app.crfRange[1]})</span></div>
    {/if}
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
    <button class="tab" class:on={app.tab === "info"} role="tab" aria-selected={app.tab === "info"} title="What is in the open file: container, codecs, sizes, frame rate, color, tracks" onclick={() => app.setTab("info")}>File info</button>
  </div>

  <div class="scroll">
    {#if app.tab === "info"}
      <FileInfo />
    {:else if app.tab === "basic"}
      <Group id="output" title="Video" note="{app.container.toUpperCase()} · {CODEC_LABEL[app.codec]}">
        <div class="row">
          <span title="WebM plays on 4chan and in browsers; MP4 plays nearly everywhere, including phones and Discord.">Format</span>
          <Seg label="Container" value={app.container} onchange={(v) => app.setContainer(v)} options={[{ value: "webm", label: "WebM" }, { value: "mp4", label: "MP4" }, { value: "gif", label: "GIF" }, { value: "webp", label: "WebP" }, { value: "audio", label: "Audio" }]} />
        </div>
        <div class="row">
          <span title="VP9 and AV1 give the best picture per MB (AV1 encodes slowest). VP8 and H.264 are for old players. HEVC is H.265, smaller than H.264 but not supported everywhere. Grayed out: your ffmpeg lacks it.">Codec</span>
          <Seg label="Codec" value={app.codec} onchange={(v) => app.setCodec(v)} options={codecs} />
        </div>
        {#if app.container === "audio"}
          <div class="muted small">Audio only: the sound of the ranges, with speed, direction, fades and the audio level. Resolution, crop and the picture filters do nothing.</div>
        {:else}
        <ResolutionPicker />
        <div class="row">
          <span title="Cut away the edges of the picture. Draw the rectangle on the video (C).">Crop</span>
          {#if app.crop}
            <span class="mono small">{app.crop.w}x{app.crop.h} at {app.crop.x},{app.crop.y}</span>
            <button class="btn small" onclick={() => app.setCrop(null)}>Clear</button>
          {:else}
            <span class="muted small grow">none</span>
            <button class="btn small" disabled={!app.media || app.detectingCrop || app.ffmpegMissing} title="Find black bars (letterbox or pillarbox) and crop them" onclick={() => app.detectBars()}>{app.detectingCrop ? "Checking..." : "Find bars"}</button>
            <button class="btn small" disabled={!app.media} onclick={() => (app.cropMode = true)}>Draw <kbd>C</kbd></button>
          {/if}
        </div>
        {/if}
      </Group>

      <Group id="motion" title="Motion" note="{app.speed !== 1 ? `${app.speed}x · ` : ''}{app.media ? `${Math.round(app.outFps * 100) / 100} fps` : ''}">
        <MotionControl />
      </Group>

      <Group id="audio" title="Audio" note={app.audioOn ? `${app.audioCodecName} ${app.audioBitrateK}k` : "off"}>
        {#if app.codec === "gif" || app.codec === "webp"}
          <div class="muted small">Animated {app.codec === "gif" ? "GIF" : "WebP"} has no sound.</div>
        {:else if app.media && app.media.audio.length === 0}
          <div class="muted small">The source has no audio.</div>
        {:else}
          <div class="row">
            <span title="Which audio track of the source goes into the output, or Off for a silent file.">Track</span>
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
              <span title="Bits per second for the sound. In size limit mode this comes out of the video's share. 96 kbps Opus is near transparent for most clips.">Bitrate ({app.audioCodecName})</span>
              <Select label="Audio bitrate" bind:value={app.audioBitrateK} options={app.audioRates.map((r) => ({ value: r, label: `${r} kbps` }))} />
            </div>
            {#if app.playback === "boomerang"}<div class="muted small">Boomerang output has no audio.</div>{/if}
          {/if}
        {/if}
      </Group>

      <Group id="file" title="Output" note={app.outputName || ""}>
        <label class="col">
          <span title={"The name of the encoded file, without the extension. Empty uses the name template from Settings (for example {name}_{start}-{end}). An existing file is never overwritten; a number is added."}>Output filename</span>
          <input class="field" type="text" spellcheck="false" placeholder={app.settings.nameTemplate} bind:value={app.outputName} />
        </label>
        <div class="row">
          <span title="Where encoded files and saved frames go.">Output folder</span>
          <Select label="Output folder" value={outMode} options={outOptions} onchange={setOutMode} />
          <button class="btn small icon" aria-label="Choose another folder" onclick={chooseFolder}><Icon name="open" size={12} /></button>
        </div>
        <div class="path small muted">{app.outputPath ?? "Beside each source file"}</div>
      </Group>
    {:else}
      <Group id="encoder" title="Encoder" note={app.effort}>
        <div class="row">
          <span title="How hard the encoder works. Best gives a slightly better picture at the same size and takes several times longer; Fast is for quick tests.">Effort</span>
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
        {#if app.rateMode === "crf"}
          <label class="row">
            <span title="Quality mode: a ceiling on the bitrate, so complex scenes cannot make the file very large. Empty = no ceiling.">Max bitrate</span>
            <span class="unit">
              <input class="field mono" type="number" min="0" step="50" placeholder="none" bind:value={app.maxBitrateK} />
              <span class="muted">kbps</span>
            </span>
          </label>
          <label class="check" title="Analyze the video first, then encode. Better bit distribution, twice the time. Size limit mode always uses two passes."><input type="checkbox" bind:checked={app.twoPass} /> Two passes</label>
        {/if}
      </Group>

      <Group id="filters" title="Filters" note={filterNote}>
        <div class="checks">
          <label class="check" title="Removes the comb lines of interlaced video (TV recordings, old camcorders). Turned on by itself when the source is interlaced."><input type="checkbox" bind:checked={app.deinterlace} /> Deinterlace</label>
          <label class="check" title="Smooths out grain and noise (hqdn3d). Noise costs bits, so this can help at small sizes; it also softens fine detail."><input type="checkbox" bind:checked={app.denoise} /> Denoise</label>
          <label class="check" title="Steadies a shaky hand-held picture (ffmpeg deshake). The edges move, so crop a little."><input type="checkbox" bind:checked={app.stabilize} /> Stabilize</label>
          <label class="check" title="At speeds below 1x, makes new in-between frames by motion estimation (minterpolate) instead of showing each frame several times. Slow to encode; fast motion can show warping."><input type="checkbox" bind:checked={app.smoothSlowmo} /> Smooth slow motion</label>
          <label class="check" class:off={!zscale} title="Converts HDR video to normal (SDR) colors, so it does not look gray and washed out. Turned on by itself for HDR sources.">
            <input type="checkbox" bind:checked={app.tonemap} disabled={!zscale} /> Tone map HDR
          </label>
        </div>
        {#if app.media?.video.interlaced}<div class="muted small">The source is interlaced.</div>{/if}
        {#if app.media?.video.hdr}<div class="muted small">The source is HDR.</div>{/if}
        {#if !zscale}<div class="muted small">Tone mapping needs an ffmpeg build with the zscale filter.</div>{/if}
        <div class="row">
          <span title="Turns the picture clockwise. The crop is drawn on the unturned picture.">Rotate</span>
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
          <span title="Flip the picture: horizontal mirrors left and right, vertical turns it upside down. Applied after rotating.">Flip</span>
          <div class="flips">
            <button class="btn small" class:on={app.mirror} aria-pressed={app.mirror} onclick={() => (app.mirror = !app.mirror)}><Icon name="flipH" size={13} />Horizontal</button>
            <button class="btn small" class:on={app.vflip} aria-pressed={app.vflip} onclick={() => (app.vflip = !app.vflip)}><Icon name="flipV" size={13} />Vertical</button>
          </div>
        </div>
        <div class="row">
          <span title="Reverse plays the clip backwards. Boomerang plays it forward and then backwards (and has no sound). Both hold every frame in memory, so keep them short.">Direction</span>
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
          <span title="Fade from black at the start and to black at the end, in seconds; the sound fades with it. 0 = no fade.">Fade</span>
          <span class="unit">
            <span class="muted small">in</span>
            <input class="field mono s" type="number" min="0" step="0.25" aria-label="Fade in seconds" value={app.fadeIn} onchange={(e) => (app.fadeIn = Math.max(0, num(e)))} />
            <span class="muted small">out</span>
            <input class="field mono s" type="number" min="0" step="0.25" aria-label="Fade out seconds" value={app.fadeOut} onchange={(e) => (app.fadeOut = Math.max(0, num(e)))} />
            <span class="muted small">s</span>
          </span>
        </div>
      </Group>

      <Group id="adjust" title="Adjustments" note={isNeutral(app.adjust) ? "" : "changed"}>
        <AdjustPanel />
      </Group>

      <Group id="audiolevel" title="Audio level" note={app.audioOn ? `${app.volumeDb} dB${app.normalize ? ", normalized" : ""}` : "audio off"}>
        {#if !app.audioOn}
          <div class="muted small">Audio is off (Basic tab).</div>
        {:else}
          <label class="row">
            <span title="Makes the sound louder or quieter, in decibels. +6 dB is about twice as loud; 0 = unchanged.">Volume</span>
            <span class="unit">
              <input class="field mono s" type="number" min="-30" max="30" step="0.5" value={app.volumeDb} onchange={(e) => (app.volumeDb = Math.min(30, Math.max(-30, num(e))))} />
              <span class="muted small">dB</span>
            </span>
          </label>
          <label class="check" title="Brings the sound to a standard loudness (EBU R128, -16 LUFS), so quiet clips get louder and loud ones quieter."><input type="checkbox" bind:checked={app.normalize} /> Normalize loudness</label>
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
        {#if app.subsKind !== "off"}
          <div class="row">
            <span title="Seconds the subtitles appear later; negative makes them earlier. For a subtitle file that does not match this cut of the video.">Delay (s)</span>
            <input class="field mono num" type="number" step="0.1" bind:value={app.subsDelay} />
          </div>
          <div class="row">
            <span title="Font size of the subtitles. The file's own keeps the size set in the file (18 for SRT).">Size</span>
            <Select
              label="Subtitle size"
              value={app.subsFontSize ?? 0}
              onchange={(v) => (app.subsFontSize = v || null)}
              options={[{ value: 0, label: "File's own" }, { value: 14, label: "Small (14)" }, { value: 22, label: "Large (22)" }, { value: 28, label: "Larger (28)" }, { value: 36, label: "Huge (36)" }]}
            />
          </div>
        {/if}
      </Group>

      <Group id="caption" title="Caption" note={app.captionText.trim() ? (app.captionTop ? "top" : "bottom") : "none"}>
        <label class="col">
          <span title="Text drawn on the picture, centered, white with a black outline. Several lines are allowed. The player shows where it goes; Sample shows the real result.">Text</span>
          <textarea class="field" rows="2" spellcheck="true" placeholder="none" bind:value={app.captionText}></textarea>
        </label>
        <div class="row">
          <span>Position</span>
          <Seg label="Caption position" value={app.captionTop ? "top" : "bottom"} onchange={(v) => (app.captionTop = v === "top")} options={[{ value: "top", label: "Top" }, { value: "bottom", label: "Bottom" }]} />
        </div>
        <div class="row">
          <span title="Text height as a share of the output picture's height.">Size</span>
          <Select label="Caption size" bind:value={app.captionSize} options={[4, 5, 7, 9, 12, 15].map((v) => ({ value: v, label: `${v}%` }))} />
        </div>
      </Group>

      <Group id="logo" title="Logo" note={app.logoPath ? baseName(app.logoPath) : "none"}>
        <div class="row">
          <span class="path small" class:muted={!app.logoPath} title={app.logoPath ?? ""}>{app.logoPath ? baseName(app.logoPath) : "No image"}</span>
          <button class="btn small" onclick={async () => { const f = await pickImage(); if (f) app.logoPath = f; }}>Choose</button>
          {#if app.logoPath}<button class="btn small" onclick={() => (app.logoPath = null)}>Remove</button>{/if}
        </div>
        {#if app.logoPath}
          <div class="row">
            <span>Corner</span>
            <Seg label="Logo corner" value={app.logoCorner} onchange={(v) => (app.logoCorner = v)} options={[{ value: "tl", label: "↖" }, { value: "tr", label: "↗" }, { value: "bl", label: "↙" }, { value: "br", label: "↘" }]} />
          </div>
          <div class="row">
            <span title="Width of the image as a share of the output picture's width.">Size</span>
            <Select label="Logo size" bind:value={app.logoSize} options={[5, 8, 10, 15, 20, 25, 33, 50].map((v) => ({ value: v, label: `${v}%` }))} />
          </div>
          <div class="row">
            <span>Opacity</span>
            <input type="range" min="0.1" max="1" step="0.05" bind:value={app.logoOpacity} aria-label="Logo opacity" />
            <span class="mono small">{Math.round(app.logoOpacity * 100)}%</span>
          </div>
        {/if}
      </Group>

      <Group id="meta" title="Metadata and arguments">
        <label class="col">
          <span title="The title stored inside the file; players show it in the window title. Other metadata from the source is always dropped.">Title metadata</span>
          <input class="field" type="text" spellcheck="false" placeholder="none" bind:value={app.title} />
        </label>
        <label class="col">
          <span title="Added to the ffmpeg command just before the output file, for options the app does not offer. Quote values with spaces.">Extra ffmpeg arguments</span>
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
    {#if app.view === "batch"}
      <div class="muted small">These settings apply to every file in the batch{batch.included.length ? ` (${batch.included.length})` : ""}. Each file's check is on the left.</div>
      <div class="actions">
        <button class="btn primary big" disabled={!batch.included.length || app.ffmpegMissing} onclick={() => batch.enqueue(true)}>Encode {batch.included.length} files</button>
        <button class="btn big" disabled={!batch.included.length || app.ffmpegMissing} onclick={() => batch.enqueue(false)}>Add to queue</button>
      </div>
    {:else}
      {#if app.plan}
        <div class="summary">
          <span class="mono small">
            {#if app.container === "audio"}audio only{:else}{app.plan.outWidth}x{app.plan.outHeight}{app.plan.autoScaled ? " (auto)" : ""}{/if} · {fmtTime(app.plan.durationOut)}
          </span>
          {#if app.plan.quality}
            <span class="badge {app.plan.quality}">{app.plan.quality}</span>
          {/if}
        </div>
        <div class="mono small muted">
          {#if app.container === "audio"}
            {app.audioCodecName} {Math.round(app.plan.videoKbps ?? app.plan.audioKbps)} kbps
          {:else}
            video {app.plan.videoKbps === null ? "CRF" : `${Math.round(app.plan.videoKbps)} kbps`}
            · audio {app.plan.audioKbps > 0 ? `${Math.round(app.plan.audioKbps)} kbps` : "off"}
          {/if}
          {#if app.plan.bitsPerPixel !== null}· {app.plan.bitsPerPixel.toFixed(3)} bpp{/if}
          {#if app.rateMode === "size"}· at most {fmtBytes(app.maxBytes)}{/if}
          {#if app.separate && app.segments.length > 1}· {app.segments.length} files{/if}
        </div>
        {#if app.plan.output}
          <div class="mono small muted outname" title={app.plan.output}>→ {baseName(app.plan.output)}{app.separate && app.segments.length > 1 ? " (first of them)" : ""}</div>
        {/if}
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
    {/if}
  </div>
</aside>

<style>
  input.num {
    width: 80px;
  }
  .sidebar {
    width: var(--side-w, 330px);
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
  .flips {
    display: flex;
    gap: 4px;
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
  .copy {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .unit {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .unit input {
    width: 90px;
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
  .outname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
