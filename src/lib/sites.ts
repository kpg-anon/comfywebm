// What the site a preset is made for accepts. The site is read from the
// preset's name ("4chan ...", "X / Twitter"), so presets written to the config
// folder by earlier versions, and your own presets named after the site, get
// the same rules. Formats and codecs a site refuses are grayed out in the
// sidebar. Sources (2026-11): 4chan takes WebM with VP8 or VP9 (not AV1) and
// MP4 with H.264, plus GIF; X takes MP4 with H.264, plus GIF.
import type { Codec, Container } from "./types";

export type Site = { name: string; containers: Container[]; codecs: Codec[] };

const SITES: [RegExp, Site][] = [
  [/^4chan\b/i, { name: "4chan", containers: ["webm", "mp4", "gif"], codecs: ["vp9", "vp8", "h264", "gif"] }],
  [/^x\b|twitter/i, { name: "X", containers: ["mp4", "gif"], codecs: ["h264", "gif"] }],
];

const CONTAINER_LABEL: Record<Container, string> = { webm: "WebM", mp4: "MP4", gif: "GIF", webp: "WebP", audio: "sound-only files" };
const CODEC_NAME: Record<Codec, string> = {
  vp9: "VP9",
  vp8: "VP8",
  av1: "AV1",
  h264: "H.264",
  hevc: "HEVC",
  gif: "GIF",
  webp: "WebP",
  opus: "Opus",
  mp3: "MP3",
  aac: "AAC",
};

/** The site a preset is for, or null (no limits known). */
export function siteOf(presetName: string | null | undefined): Site | null {
  const n = (presetName ?? "").trim();
  return SITES.find(([re]) => re.test(n))?.[1] ?? null;
}

/** Why the site does not take this format (or this codec in it), or null when it does. */
export function refusal(site: Site | null, container: Container, codec?: Codec): string | null {
  if (!site) return null;
  if (!site.containers.includes(container)) return `${site.name} does not accept ${CONTAINER_LABEL[container]}.`;
  if (codec && !site.codecs.includes(codec)) return `${site.name} does not accept ${CODEC_NAME[codec]} in ${CONTAINER_LABEL[container]}.`;
  return null;
}
