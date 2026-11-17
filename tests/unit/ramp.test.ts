// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { clean, outLength, speedAt, speedRange, speedToY, yToSpeed, niceSpeed } from "../../src/lib/ramp.ts";

const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `${a} vs ${b}`);

// the same numbers as ramp_lengths in crates/engine/src/ramp.rs
test("outLength matches the engine's ramp lengths", () => {
  near(outLength([{ t: 4, speed: 2 }, { t: 0, speed: 0.5 }], [{ start: 0, end: 6 }]), Math.log(4) / 0.375 + 1);
  const two = outLength([{ t: 3, speed: 1 }, { t: 11, speed: 3 }], [{ start: 10, end: 12 }, { start: 2, end: 4 }]);
  near(two, Math.log(3 / 2.75) / 0.25 + 1 / 3 + 1 + Math.log(1.25) / 0.25);
  near(outLength([{ t: 5, speed: 2 }], [{ start: 0, end: 4 }]), 2);
  near(outLength([], [{ start: 0, end: 4 }]), 4);
  near(outLength([{ t: 0, speed: 0.01 }], [{ start: 0, end: 1 }]), 4);
});

test("speedAt holds the ends and is linear between points", () => {
  const p = clean([{ t: 4, speed: 2 }, { t: 0, speed: 0.5 }]);
  assert.equal(speedAt(p, -1), 0.5);
  assert.equal(speedAt(p, 2), 1.25);
  assert.equal(speedAt(p, 9), 2);
  assert.deepEqual(speedRange(p, [{ start: 1, end: 3 }]), [0.875, 1.625]);
});

test("the lane's log scale and rounding", () => {
  assert.equal(speedToY(0.25), 0);
  assert.equal(speedToY(4), 1);
  near(speedToY(1), 0.5);
  near(yToSpeed(0.5), 1);
  assert.equal(niceSpeed(1.03), 1);
  assert.equal(niceSpeed(1.234), 1.23);
});
