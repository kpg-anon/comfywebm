// The output line beside Encode: picture size, frame rate, sound, length and
// the expected file size, in the unit the target site counts in.
import type { PlanInfo, Rate } from "./types";

export type SizeUnit = "MiB" | "MB";

/**
 * The unit the size limit was set in: a whole number of decimal megabytes
 * (Discord's 20,000,000 bytes) is MB, anything else MiB (4chan's 4,194,304
 * bytes). Quality mode has no limit: MiB, the app's own unit.
 */
export function sizeUnit(rate: Rate): SizeUnit {
  if (rate.mode !== "size") return "MiB";
  const b = Math.round(rate.maxBytes);
  return b > 0 && b % 1_000_000 === 0 && b % 1_048_576 !== 0 ? "MB" : "MiB";
}

export function fmtSize(bytes: number, unit: SizeUnit): string {
  return `${(bytes / (unit === "MB" ? 1e6 : 1_048_576)).toFixed(2)} ${unit}`;
}

/** The expected size in bytes; `atMost`: only an upper bound is known. Null: not known before encoding. */
export function expectedSize(plan: PlanInfo, rate: Rate): { bytes: number; atMost: boolean } | null {
  const secs = plan.durationOut;
  if (!(secs > 0)) return null;
  const bytes = (kbps: number) => (kbps * 1000 * secs) / 8;
  if (rate.mode === "size") {
    // GIF and WebP have no bitrate: the picture size is chosen to fit
    if (plan.videoKbps === null) return { bytes: rate.maxBytes, atMost: true };
    // The bitrate is worked out from the limit, but it is a ceiling: a simple
    // picture with bits to spare comes out smaller (a 2 s test pattern at 1.6
    // bits per pixel made 0.57 of 4 MiB, even after the retry that fills the limit).
    return { bytes: Math.min(rate.maxBytes, bytes(plan.videoKbps + plan.audioKbps)), atMost: true };
  }
  // quality mode: sound only has a fixed bitrate; a video's size depends on the picture, below its cap if one is set
  if (plan.outWidth === 0) return { bytes: bytes(plan.audioKbps), atMost: false };
  if (plan.videoKbps !== null) return { bytes: bytes(plan.videoKbps + plan.audioKbps), atMost: true };
  return null;
}

export function fmtFps(fps: number): string {
  return `${Math.round(fps * 100) / 100} fps`;
}

/**
 * ["1280x720", "30 fps", "audio", "12.0 s", "up to 3.96 MiB"], the length written by `fmtLength` (time.ts fmtLen).
 * `files` > 1: each range is its own file, each under the limit.
 */
export function outputParts(plan: PlanInfo, rate: Rate, fmtLength: (secs: number) => string, files = 1): string[] {
  const unit = sizeUnit(rate);
  const parts: string[] = [];
  if (plan.outWidth > 0) {
    parts.push(`${plan.outWidth}x${plan.outHeight}${plan.autoScaled ? " (auto)" : ""}`, fmtFps(plan.outFps), plan.audioKbps > 0 ? "audio" : "no audio");
  } else parts.push("sound only");
  if (files > 1) {
    parts.push(rate.mode === "size" ? `${files} files, each up to ${fmtSize(rate.maxBytes, unit)}` : `${files} files`);
    return parts;
  }
  parts.push(fmtLength(plan.durationOut));
  const e = expectedSize(plan, rate);
  parts.push(e ? `${e.atMost ? "up to" : "about"} ${fmtSize(e.bytes, unit)}` : "size known after encoding");
  return parts;
}
