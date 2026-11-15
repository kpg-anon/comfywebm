<script lang="ts">
  // Sidebar > File info: what ffprobe reported about the open file.
  import { app } from "$lib/state.svelte";
  import { fmtBytes, fmtTime } from "$lib/time";
  import Icon from "./Icon.svelte";

  let copied = $state(false);

  const m = $derived(app.media);

  function gcd(a: number, b: number): number {
    return b ? gcd(b, a % b) : a;
  }
  const aspect = (w: number, h: number) => {
    const g = gcd(w, h) || 1;
    const r = w / h;
    return `${w / g}:${h / g}${w / g > 50 ? ` (${r.toFixed(3)})` : ""}`;
  };
  const kbps = (k: number | null) => (k ? `${k.toLocaleString("en-US")} kbps` : "unknown");

  type Row = [string, string];
  const sections = $derived.by((): { title: string; rows: Row[] }[] => {
    if (!m) return [];
    const v = m.video;
    const parts = m.path.split(/[\\/]/);
    const out: { title: string; rows: Row[] }[] = [
      {
        title: "File",
        rows: [
          ["Name", m.fileName],
          ["Folder", parts.slice(0, -1).join("\\")],
          ["Size", `${fmtBytes(m.sizeBytes)} (${m.sizeBytes.toLocaleString("en-US")} bytes)`],
          ["Container", m.format || "unknown"],
          ["Duration", fmtTime(m.duration)],
          ["Bitrate", kbps(m.bitrateK)],
          ...(m.created ? ([["Created", m.created]] as Row[]) : []),
          ...(m.encoder ? ([["Written by", m.encoder]] as Row[]) : []),
        ],
      },
      {
        title: "Video",
        rows: [
          ["Codec", [v.codec, v.profile].filter(Boolean).join(", ")],
          ["Size", `${v.width}x${v.height} (${aspect(v.width, v.height)})`],
          ["Frame rate", `${Math.round(v.fps * 1000) / 1000} fps${v.vfr ? ", variable" : ""}`],
          ...(v.frames ? ([["Frames", v.frames.toLocaleString("en-US")]] as Row[]) : []),
          ["Bitrate", kbps(v.bitrateK)],
          ["Pixel format", v.pixFmt ?? "unknown"],
          ["Color", v.color ?? "not tagged"],
          ["Scan", v.interlaced ? "interlaced" : "progressive"],
          ["Range", v.hdr ? "HDR" : "SDR"],
        ],
      },
    ];
    m.audio.forEach((a) =>
      out.push({
        title: `Audio track ${a.track + 1}`,
        rows: [
          ["Codec", a.codec],
          ["Channels", String(a.channels)],
          ...(a.sampleRate ? ([["Sample rate", `${a.sampleRate.toLocaleString("en-US")} Hz`]] as Row[]) : []),
          ["Bitrate", kbps(a.bitrateK)],
          ...(a.lang ? ([["Language", a.lang]] as Row[]) : []),
          ...(a.title ? ([["Title", a.title]] as Row[]) : []),
        ],
      }),
    );
    m.subs.forEach((s) =>
      out.push({
        title: `Subtitle track ${s.track + 1}`,
        rows: [
          ["Codec", `${s.codec}${s.textBased ? "" : " (image-based)"}`],
          ...(s.lang ? ([["Language", s.lang]] as Row[]) : []),
          ...(s.title ? ([["Title", s.title]] as Row[]) : []),
        ],
      }),
    );
    if (!m.audio.length) out.push({ title: "Audio", rows: [["Tracks", "none"]] });
    return out;
  });

  async function copy() {
    const text = sections.map((s) => [s.title, ...s.rows.map(([k, v]) => `  ${k}: ${v}`)].join("\n")).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1200);
    } catch {
      copied = false;
    }
  }
</script>

{#if !m}
  <div class="empty muted">Open a video to see what is in it.</div>
{:else}
  <div class="info">
    {#each sections as s (s.title)}
      <section>
        <h3 class="head-text">{s.title}</h3>
        {#each s.rows as [k, v] (k)}
          <div class="kv">
            <span class="k">{k}</span>
            <span class="v" class:mono={k !== "Name" && k !== "Folder" && k !== "Title"}>{v}</span>
          </div>
        {/each}
      </section>
    {/each}
    <button class="btn small copy" onclick={copy}><Icon name={copied ? "check" : "copy"} size={11} />{copied ? "Copied" : "Copy as text"}</button>
  </div>
{/if}

<style>
  .empty {
    padding: 14px 0;
    font-size: 12px;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 10px 0 14px;
  }
  h3 {
    margin: 0 0 4px;
  }
  .kv {
    display: grid;
    grid-template-columns: 86px 1fr;
    gap: 8px;
    padding: 2px 0;
    font-size: 12px;
  }
  .k {
    color: var(--text-muted);
  }
  .v {
    min-width: 0;
    word-break: break-word;
    user-select: text;
    color: var(--text-primary);
  }
  .v.mono {
    font-size: 11.5px;
  }
  .copy {
    align-self: flex-start;
  }
</style>
