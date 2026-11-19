// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { sizeUnit, fmtSize, expectedSize, outputParts } from "../../src/lib/outsize.ts";
import { fmtLen } from "../../src/lib/time.ts";

const outputLine = (p, r, files) => outputParts(p, r, fmtLen, files).join(" · ");

test("fmtLen: tenths under a minute, then m:ss and h:mm:ss", () => {
  assert.equal(fmtLen(2.9985), "3.0 s");
  assert.equal(fmtLen(59.94), "59.9 s");
  assert.equal(fmtLen(59.96), "1:00");
  assert.equal(fmtLen(125.4), "2:05");
  assert.equal(fmtLen(3725), "1:02:05");
});

const plan = (over = {}) => ({
  outWidth: 1280,
  outHeight: 720,
  outFps: 30,
  durationOut: 10,
  videoKbps: 3000,
  audioKbps: 96,
  bitsPerPixel: null,
  quality: null,
  autoScaled: false,
  command: "",
  warnings: [],
  ...over,
});

test("the unit follows the size limit: decimal MB for Discord, MiB for 4chan and quality mode", () => {
  assert.equal(sizeUnit({ mode: "size", maxBytes: 20_000_000 }), "MB");
  assert.equal(sizeUnit({ mode: "size", maxBytes: 4_194_304 }), "MiB");
  assert.equal(sizeUnit({ mode: "size", maxBytes: 3_500_000 }), "MiB");
  assert.equal(sizeUnit({ mode: "crf", crf: 30, maxBitrateK: null }), "MiB");
  assert.equal(fmtSize(4_194_304, "MiB"), "4.00 MiB");
  assert.equal(fmtSize(20_000_000, "MB"), "20.00 MB");
});

test("expected size: an upper bound from the bitrates (at most the limit), or the fixed sound bitrate", () => {
  // (3000 + 96) kbps for 10 s = 3,870,000 bytes
  assert.deepEqual(expectedSize(plan(), { mode: "size", maxBytes: 4_194_304 }), { bytes: 3_870_000, atMost: true });
  assert.deepEqual(expectedSize(plan({ videoKbps: 9000 }), { mode: "size", maxBytes: 4_194_304 }), { bytes: 4_194_304, atMost: true });
  // GIF: no bitrate
  assert.deepEqual(expectedSize(plan({ videoKbps: null, audioKbps: 0 }), { mode: "size", maxBytes: 4_194_304 }), { bytes: 4_194_304, atMost: true });
  assert.equal(expectedSize(plan({ videoKbps: null }), { mode: "crf", crf: 30, maxBitrateK: null }), null);
  assert.deepEqual(expectedSize(plan(), { mode: "crf", crf: 30, maxBitrateK: 3000 }), { bytes: 3_870_000, atMost: true });
  // sound only in quality mode: its bitrate is fixed
  assert.deepEqual(expectedSize(plan({ outWidth: 0, outHeight: 0, videoKbps: null, audioKbps: 128 }), { mode: "crf", crf: 30, maxBitrateK: null }), {
    bytes: 160_000,
    atMost: false,
  });
});

test("output line", () => {
  assert.equal(outputLine(plan({ audioKbps: 0, autoScaled: true }), { mode: "size", maxBytes: 4_194_304 }), "1280x720 · 30 fps · no audio · 10.0 s · ~3.58MiB");
  assert.equal(outputLine(plan({ outFps: 29.97 }), { mode: "size", maxBytes: 4_194_304 }), "1280x720 · 29.97 fps · audio · 10.0 s · ~3.69MiB");
  assert.equal(outputLine(plan(), { mode: "size", maxBytes: 20_000_000 }), "1280x720 · 30 fps · audio · 10.0 s · ~3.87MB");
  assert.equal(outputLine(plan({ videoKbps: null }), { mode: "crf", crf: 20, maxBitrateK: null }), "1280x720 · 30 fps · audio · 10.0 s · size unknown");
  assert.equal(outputLine(plan(), { mode: "size", maxBytes: 4_194_304 }, 3), "1280x720 · 30 fps · audio · 3 files, each up to 4.00 MiB");
  assert.equal(outputLine(plan({ outWidth: 0, outHeight: 0, videoKbps: null, audioKbps: 128 }), { mode: "crf", crf: 30, maxBitrateK: null }), "sound only · 10.0 s · ~0.15MiB");
});
