// Pure annotation helpers (P1.M3.UI.2, UI.5, UI.8). Run with `pnpm --filter @electropaper/reader-ui test`
// (Node 24 runs the TypeScript module directly).
import assert from "node:assert/strict";
import { test } from "node:test";

import { charIndex, citation, citeAuthors, hits, quadsFor, simplify, textFor } from "../src/annotations/geometry.ts";

// "Hello wor-\nld again" on two lines; each character is 5 pt wide and 10 pt tall.
const text = "Hello wor­\nld again";
const chars = Array.from(text);
const boxes = chars.map((c, i) => {
  const line = i < 11 ? 0 : 1;
  const col = line === 0 ? i : i - 11;
  return c === "\n" ? null : [10 + col * 5, 20 + line * 12, 15 + col * 5, 30 + line * 12];
});
const page = {
  page: 3,
  text,
  boxes,
  words: [],
  lines: [
    { start: 0, end: 11, rect: { page: 3, x0: 10, y0: 20, x1: 65, y1: 30 } },
    { start: 11, end: chars.length, rect: { page: 3, x0: 10, y0: 32, x1: 50, y1: 42 } },
  ],
  unknown: 0,
};

test("quads follow the engine's character boxes, one per line", () => {
  const q = quadsFor(page, 6, 16); // "wor­\nld ag"
  assert.equal(q.length, 2);
  assert.deepEqual(q[0].p[0], { x: 40, y: 20 }); // "w" starts at column 6
  assert.equal(q[0].p[1].x, 55); // "r" ends at column 9 (the soft hyphen has no ink)
  assert.equal(q[1].p[0].x, 10);
  assert.equal(q[1].p[3].y, 42);
  assert.equal(q[0].page, 3);
});

test("marked text joins hyphenated line breaks", () => {
  assert.equal(textFor(page, 6, 14), "world");
  assert.equal(textFor(page, 0, chars.length), "Hello world again");
});

test("DOM offsets map to character indexes (soft hyphen hidden, surrogates counted)", () => {
  assert.equal(charIndex("wor­ld", 3), 4);
  assert.equal(charIndex("abc", 1), 1);
  assert.equal(charIndex("\u{1d436}x", 2), 1); // a math letter is two UTF-16 units
});

test("ink strokes are simplified without losing their shape", () => {
  const line = Array.from({ length: 50 }, (_, i) => ({ x: i, y: 0.01 * (i % 2) }));
  assert.equal(simplify(line).length, 2);
  const corner = [...Array.from({ length: 20 }, (_, i) => ({ x: i, y: 0 })), ...Array.from({ length: 20 }, (_, i) => ({ x: 19, y: i + 1 }))];
  const s = simplify(corner);
  assert.equal(s.length, 3);
  assert.deepEqual(s[1], { x: 19, y: 0 });
});

test("hit testing", () => {
  const base = { id: "a", page: 1, color: "#000000", created_ms: 0, modified_ms: 0, width: 1, rect: { page: 1, x0: 10, y0: 10, x1: 50, y1: 30 } };
  assert.ok(hits({ ...base, kind: "rectangle" }, 10, 20));
  assert.ok(!hits({ ...base, kind: "rectangle" }, 30, 20), "inside an empty rectangle is not on it");
  assert.ok(hits({ ...base, kind: "note" }, 30, 20));
  assert.ok(hits({ ...base, kind: "line", line: [{ x: 0, y: 0 }, { x: 100, y: 100 }] }, 50, 51));
  assert.ok(!hits({ ...base, kind: "line", line: [{ x: 0, y: 0 }, { x: 100, y: 100 }] }, 50, 70));
});

test("citations (snapshot)", () => {
  assert.equal(citeAuthors(["Ada Lovelace"]), "Lovelace");
  assert.equal(citeAuthors(["Lovelace, Ada", "Charles Babbage"]), "Lovelace and Babbage");
  assert.equal(citeAuthors(["A B", "C D", "E F"]), "B et al.");
  assert.equal(
    citation("the  analytical\nengine", { title: "Notes", authors: ["Ada Lovelace", "C. Babbage", "L. Menabrea"], original_name: "x.pdf" }, 7),
    "“the  analytical engine” (Lovelace et al., p. 7)",
  );
  assert.equal(citation("text", { title: null, authors: [], original_name: "paper-2024.pdf" }, 1), "“text” (paper-2024, p. 1)");
});
