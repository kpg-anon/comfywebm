// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { siteOf, refusal } from "../../src/lib/sites.ts";

test("the site comes from the preset's name", () => {
  assert.equal(siteOf("4chan (4 MiB, no sound)")?.name, "4chan");
  assert.equal(siteOf("4chan /wsg/ (6 MiB, sound)")?.name, "4chan");
  assert.equal(siteOf("X / Twitter")?.name, "X");
  assert.equal(siteOf("Discord (20 MB)"), null);
  assert.equal(siteOf("Quality WebM"), null);
  assert.equal(siteOf(""), null);
});

test("4chan: WebM VP8/VP9, MP4 H.264 and GIF; not AV1, HEVC, WebP or sound only", () => {
  const s = siteOf("4chan (4 MiB, no sound)");
  for (const [c, k] of [["webm", "vp9"], ["webm", "vp8"], ["mp4", "h264"], ["gif", "gif"]]) assert.equal(refusal(s, c, k), null, `${c} ${k}`);
  assert.equal(refusal(s, "webm", "av1"), "4chan does not accept AV1 in WebM.");
  assert.equal(refusal(s, "mp4", "hevc"), "4chan does not accept HEVC in MP4.");
  assert.equal(refusal(s, "webp"), "4chan does not accept WebP.");
  assert.equal(refusal(s, "audio", "opus"), "4chan does not accept sound-only files.");
  // no site: no limits
  assert.equal(refusal(null, "webp", "webp"), null);
});
