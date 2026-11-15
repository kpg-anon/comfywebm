// The platform presets as a fallback list, used until config_get answers (and
// in a plain browser). The real list is the TOML files in the config folder,
// written from the same values by crates/engine/src/config.rs.
import type { Codec, Container, Preset } from "./types";

const fourchan = (name: string, maxBytes: number, audio: boolean, maxDuration: number): Preset => ({
  name,
  container: "webm",
  codec: "vp9",
  rate: { mode: "size", maxBytes },
  autoScale: true,
  maxWidth: 2048,
  maxHeight: 2048,
  fps: null,
  audioBitrateK: audio ? 96 : null,
  effort: "balanced",
  twoPass: true,
  maxDuration,
});

const discord = (name: string, maxBytes: number): Preset => ({
  name,
  container: "mp4",
  codec: "h264",
  rate: { mode: "size", maxBytes },
  autoScale: true,
  maxWidth: null,
  maxHeight: null,
  fps: null,
  audioBitrateK: 128,
  effort: "balanced",
  twoPass: true,
  maxDuration: null,
});

export const BUILTIN_PRESETS: Preset[] = [
  fourchan("4chan (4 MiB, no sound)", 4194304, false, 120),
  fourchan("4chan /b/ /bant/ (2 MiB, no sound)", 2097152, false, 120),
  fourchan("4chan /gif/ (4 MiB, sound)", 4194304, true, 300),
  fourchan("4chan /r/ /wsr/ (4 MiB, sound)", 4194304, true, 120),
  fourchan("4chan /wsg/ (6 MiB, sound)", 6291456, true, 400),
  discord("Discord (20 MB)", 20000000),
  discord("Discord Nitro Basic (50 MB)", 50000000),
  {
    name: "X / Twitter",
    container: "mp4",
    codec: "h264",
    rate: { mode: "crf", crf: 20, maxBitrateK: null },
    autoScale: false,
    maxWidth: 1280,
    maxHeight: 1280,
    fps: 60,
    audioBitrateK: 128,
    effort: "balanced",
    twoPass: false,
    maxDuration: 1200,
  },
  {
    name: "Quality WebM",
    container: "webm",
    codec: "vp9",
    rate: { mode: "crf", crf: 30, maxBitrateK: null },
    autoScale: false,
    maxWidth: null,
    maxHeight: null,
    fps: null,
    audioBitrateK: 128,
    effort: "balanced",
    twoPass: false,
    maxDuration: null,
  },
  {
    name: "Quality MP4",
    container: "mp4",
    codec: "h264",
    rate: { mode: "crf", crf: 20, maxBitrateK: null },
    autoScale: false,
    maxWidth: null,
    maxHeight: null,
    fps: null,
    audioBitrateK: 160,
    effort: "balanced",
    twoPass: false,
    maxDuration: null,
  },
  {
    name: "GIF (4 MiB)",
    container: "gif",
    codec: "gif",
    rate: { mode: "size", maxBytes: 4194304 },
    autoScale: true,
    maxWidth: 640,
    maxHeight: 640,
    fps: 15,
    audioBitrateK: null,
    effort: "balanced",
    twoPass: false,
    maxDuration: null,
  },
];

/** Codecs each container can hold, in menu order. */
export const CODECS_BY_CONTAINER: Record<Container, Codec[]> = {
  webm: ["vp9", "vp8", "av1"],
  mp4: ["h264", "hevc", "av1"],
  gif: ["gif"],
};

export const CODEC_LABEL: Record<Codec, string> = { vp9: "VP9", vp8: "VP8", av1: "AV1", h264: "H.264", hevc: "HEVC", gif: "GIF" };

/** CRF scale per codec: [min, max, default]. */
export const CRF_RANGE: Record<Codec, [number, number, number]> = {
  vp9: [0, 63, 30],
  vp8: [4, 63, 10],
  av1: [0, 63, 32],
  h264: [0, 51, 20],
  hevc: [0, 51, 24],
  // GIF has no quality scale; the value is not used
  gif: [0, 63, 30],
};

/** The audio encoder ffmpeg uses for a container and video codec. */
export function audioCodecFor(container: Container, codec: Codec): "opus" | "vorbis" | "aac" {
  if (container === "mp4") return "aac";
  return codec === "vp8" ? "vorbis" : "opus";
}

export const AUDIO_LABEL = { opus: "Opus", vorbis: "Vorbis", aac: "AAC" } as const;
