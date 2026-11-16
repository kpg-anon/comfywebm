// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { evenOn, motion, smooth, smoothSpeeds, nearestSmooth, motionWarnings, fmtFps, fmtSpeed } from "../../src/lib/motion.ts";

test("evenOn: whole multiples and divisors of the refresh rate, with 1.5% slack", () => {
  assert.equal(evenOn(30, 60), true);
  assert.equal(evenOn(29.97, 60), true);
  assert.equal(evenOn(120, 60), true);
  assert.equal(evenOn(45, 60), false);
  assert.equal(evenOn(24, 60), false);
  assert.equal(evenOn(0, 60), false);
});

test("motion: cap, uneven dropping and evenness", () => {
  assert.deepEqual(motion(60, 1, 30, 60), { rate: 60, out: 30, capped: true, uneven: false, even: true });
  assert.deepEqual(motion(45, 1, 30, 60), { rate: 45, out: 30, capped: true, uneven: true, even: true });
  // a cap at or above the rate, or 0, does nothing
  assert.equal(motion(30, 1, 30, 60).capped, false);
  assert.equal(motion(30, 1, 0, 60).capped, false);
  assert.equal(smooth(motion(24, 1.25, null, 60)), true);
  assert.equal(smooth(motion(24, 1, null, 60)), false);
});

test("smoothSpeeds for 24 fps on 60 Hz", () => {
  // targets 60/n >= 10 and 120, 180, 240 fps; 5x and up are past hi = 4
  assert.deepEqual(smoothSpeeds(24, null, 60), [0.417, 0.5, 0.625, 0.833, 1, 1.25, 2.5]);
  assert.deepEqual(smoothSpeeds(0, null, 60), [1]);
  for (const s of smoothSpeeds(30, 25, 60)) if (s !== 1) assert.ok(smooth(motion(30, s, 25, 60)), String(s));
});

test("nearestSmooth", () => {
  const list = [0.5, 1, 1.25, 2.5];
  assert.deepEqual(nearestSmooth(1, list), [0.5, 1.25]);
  assert.deepEqual(nearestSmooth(3, list), [2.5, null]);
  assert.deepEqual(nearestSmooth(0.1, list), [null, 0.5]);
});

test("motionWarnings", () => {
  assert.deepEqual(motionWarnings(24, 1, null, 60), []);
  assert.deepEqual(motionWarnings(24, 1.25, null, 60), []);
  const w = motionWarnings(45, 1, 30, 60);
  assert.equal(w.length, 1);
  assert.match(w[0], /45 fps into 30 fps/);
  const j = motionWarnings(24, 1.1, null, 60);
  assert.equal(j.length, 1);
  assert.match(j[0], /26\.4 fps.*60 Hz/);
  assert.equal(fmtFps(29.97003), "29.97 fps");
  assert.equal(fmtSpeed(0.83333), "0.833x");
});
