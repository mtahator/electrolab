// The container owns the width (design language §5.4).
import assert from "node:assert/strict";
import { test } from "node:test";

import { findWidths } from "../scripts/widths.mjs";

test("the width that spilled out of Settings is refused", () => {
  assert.deepEqual(findWidths(".settings {\n  min-width: min(560px, 80vw);\n}"), ["2: .settings { min-width: min(560px, 80vw) }"]);
});

test("viewport widths and large fixed widths on content are refused", () => {
  assert.equal(findWidths(".a {\n  width: 50vw;\n}\n.b {\n  width: 400px;\n}").length, 2);
});

test("surfaces, small controls, max-width and comments are fine", () => {
  assert.deepEqual(findWidths(".palette {\n  width: min(560px, calc(100vw - 32px));\n}\n.x {\n  width: 32px;\n  min-width: 0;\n  max-width: 900px;\n  /* width: 900px; */\n}"), []);
});
