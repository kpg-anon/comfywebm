// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { fit, stepBox, percentBox, exactBox, withLimit, quality } from "../../src/lib/resolution.ts";

test("fit keeps the aspect ratio inside the box and gives even sizes", () => {
  assert.deepEqual(fit(1920, 1080, { w: null, h: 720 }), { w: 1280, h: 720 });
  assert.deepEqual(fit(1920, 1080, { w: 1280, h: 1280 }), { w: 1280, h: 720 });
  assert.deepEqual(fit(1080, 1920, { w: 2048, h: 1024 }), { w: 576, h: 1024 });
  assert.deepEqual(fit(1919, 1079, { w: null, h: null }), { w: 1918, h: 1078 });
});

test("fit never upscales", () => {
  assert.deepEqual(fit(640, 360, { w: 1920, h: 1080 }), { w: 640, h: 360 });
});

// plan.rs `even_down` rounds to the nearest pixel before dropping to even;
// fit() floors, so 865.78 px becomes 864 here and 866 in the engine.
test("fit matches the engine's even_down rounding", () => {
  assert.deepEqual(fit(1920, 1080, { w: null, h: 487 }), { w: 866, h: 486 });
});

test("mode boxes", () => {
  assert.deepEqual(stepBox("height", 720), { w: null, h: 720 });
  assert.deepEqual(stepBox("width", 1280), { w: 1280, h: null });
  assert.deepEqual(percentBox(1920, 1080, 50), { w: 960, h: 540 });
  assert.deepEqual(percentBox(10, 10, 1), { w: 2, h: 2 });
  assert.deepEqual(exactBox("h", 480), { w: null, h: 480 });
});

test("withLimit takes the smaller edge and treats null as no limit", () => {
  assert.deepEqual(withLimit({ w: 4096, h: null }, { w: 2048, h: 2048 }), { w: 2048, h: 2048 });
  assert.deepEqual(withLimit({ w: 1280, h: null }, { w: null, h: null }), { w: 1280, h: null });
  assert.deepEqual(withLimit({ w: null, h: 720 }, { w: 2048, h: 2048 }), { w: 2048, h: 720 });
});

test("quality thresholds at the floor and 1.5x the floor", () => {
  // 1280x720 at 30 fps = 27.648 Mpx/s; vp9 floor 0.03 bpp -> 829.44 kbps
  assert.equal(quality(830, 1280, 720, 30, "vp9"), "fair");
  assert.equal(quality(829, 1280, 720, 30, "vp9"), "poor");
  assert.equal(quality(1245, 1280, 720, 30, "vp9"), "good");
  assert.equal(quality(1243, 1280, 720, 30, "vp9"), "fair");
  assert.equal(quality(830, 1280, 720, 30, "h264"), "poor");
});
