// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { fmtTime, parseTime, fmtEta, fmtBytes, MB } from "../../src/lib/time.ts";

test("fmtTime leaves out a zero hour and rounds to milliseconds", () => {
  assert.equal(fmtTime(0), "0:00.000");
  assert.equal(fmtTime(65.25), "1:05.250");
  assert.equal(fmtTime(3661.5), "1:01:01.500");
  assert.equal(fmtTime(5, true), "0:00:05.000");
  // 59.9996 rounds up to a whole minute, not "0:60.000"
  assert.equal(fmtTime(59.9996), "1:00.000");
  assert.equal(fmtTime(-3), "0:00.000");
  assert.equal(fmtTime(NaN), "0:00.000");
});

test("parseTime reads h:mm:ss.mmm, m:ss, plain seconds and fractions", () => {
  assert.equal(parseTime("1:01:01.5"), 3661.5);
  assert.equal(parseTime(" 1:30 "), 90);
  assert.equal(parseTime("90"), 90);
  assert.equal(parseTime(".5"), 0.5);
  assert.equal(parseTime("5."), 5);
});

test("parseTime rejects text that is not a time", () => {
  for (const s of ["", "  ", "abc", "-1", "1:2:3:4", "1::2", "1.5:30", "1:3e1"]) assert.equal(parseTime(s), null, s);
});

test("fmtTime -> parseTime round-trips to the millisecond", () => {
  for (const t of [0, 0.001, 12.345, 59.999, 600, 3599.999, 7322.042]) {
    assert.equal(parseTime(fmtTime(t)), t, String(t));
    assert.equal(parseTime(fmtTime(t, true)), t, String(t));
  }
});

test("fmtEta", () => {
  assert.equal(fmtEta(42.4), "42s");
  assert.equal(fmtEta(59.6), "1m 00s");
  assert.equal(fmtEta(185), "3m 05s");
  assert.equal(fmtEta(3720), "1h 02m");
  assert.equal(fmtEta(-10), "0s");
});

test("fmtBytes uses binary units", () => {
  assert.equal(fmtBytes(1023), "1023 B");
  assert.equal(fmtBytes(1024), "1.0 KiB");
  assert.equal(fmtBytes(4 * MB - 20000), "3.98 MiB");
  assert.equal(fmtBytes(1024 * MB), "1.00 GiB");
});
