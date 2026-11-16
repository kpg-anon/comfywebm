// @ts-nocheck
import { test } from "node:test";
import assert from "node:assert/strict";
import { rangeAt, setIn, setOut, addAt, bounds, setEdge, totalLength } from "../../src/lib/segments.ts";

const L = () => [
  { start: 2, end: 4 },
  { start: 6, end: 8 },
];

test("rangeAt includes both edges and returns -1 in gaps", () => {
  assert.equal(rangeAt(L(), 2), 0);
  assert.equal(rangeAt(L(), 8), 1);
  assert.equal(rangeAt(L(), 5), -1);
});

test("setIn inside a range moves its start, kept minLen before the end", () => {
  assert.deepEqual(setIn(L(), 3, 10, 0.5).list[0], { start: 3, end: 4 });
  assert.deepEqual(setIn(L(), 3.9, 10, 0.5).list[0], { start: 3.5, end: 4 });
});

test("setIn in a gap extends the next range back; past the last range adds one to the end", () => {
  const a = setIn(L(), 5, 10, 0.5);
  assert.deepEqual(a.list, [{ start: 2, end: 4 }, { start: 5, end: 8 }]);
  assert.equal(a.index, 1);
  const b = setIn(L(), 9, 10, 0.5);
  assert.deepEqual(b.list[2], { start: 9, end: 10 });
  assert.equal(b.index, 2);
  // too close to the end for minLen: unchanged
  assert.equal(setIn(L(), 9.8, 10, 0.5).index, -1);
});

test("setOut in a gap extends the previous range; before every range adds one from 0", () => {
  const a = setOut(L(), 5, 0.5);
  assert.deepEqual(a.list[0], { start: 2, end: 5 });
  assert.equal(a.index, 0);
  const b = setOut(L(), 1, 0.5);
  assert.deepEqual(b.list, [{ start: 0, end: 1 }, { start: 2, end: 4 }, { start: 6, end: 8 }]);
  assert.equal(b.index, 0);
  assert.equal(setOut([], 0.2, 0.5).index, -1);
  // inside a range: end kept at least minLen after the start
  assert.deepEqual(setOut(L(), 6.1, 0.5).list[1], { start: 6, end: 6.5 });
});

test("addAt stops at the next range and refuses short or overlapping ranges", () => {
  const a = addAt(L(), 4.5, 10, 5, 0.5);
  assert.deepEqual(a.list[1], { start: 4.5, end: 6 });
  assert.equal(a.index, 1);
  assert.deepEqual(addAt([], 1, 10, 3, 0.5).list, [{ start: 1, end: 4 }]);
  assert.equal(addAt(L(), 3, 10, 5, 0.5), null);
  assert.equal(addAt(L(), 5.8, 10, 5, 0.5), null);
});

test("bounds and setEdge clamp between neighbours and keep minLen", () => {
  assert.deepEqual(bounds(L(), 0, 10), { min: 0, max: 6 });
  assert.deepEqual(bounds(L(), 1, 10), { min: 4, max: 10 });
  assert.deepEqual(setEdge(L(), 1, "start", 1, 10, 0.5)[1], { start: 4, end: 8 });
  assert.deepEqual(setEdge(L(), 0, "end", 9, 10, 0.5)[0], { start: 2, end: 6 });
  assert.deepEqual(setEdge(L(), 0, "start", 3.9, 10, 0.5)[0], { start: 3.5, end: 4 });
});

test("edits return a new sorted list and leave the input alone", () => {
  const src = L();
  const r = setIn(src, 0, 10, 0.5);
  assert.deepEqual(src, L());
  assert.deepEqual(r.list.map((s) => s.start), [0, 6]);
  assert.equal(totalLength(r.list), 6);
  assert.equal(totalLength([]), 0);
});
