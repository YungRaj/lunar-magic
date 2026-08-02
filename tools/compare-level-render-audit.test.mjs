import assert from "node:assert/strict";
import test from "node:test";

import { parseIgnoredLiveRects } from "./compare-level-render-audit.mjs";

test("reference overlay rectangles parse without losing boundaries", () => {
  assert.deepEqual(parseIgnoredLiveRects("96,112,64,16;16,277,16,27"), [
    [96, 112, 64, 16],
    [16, 277, 16, 27],
  ]);
  assert.deepEqual(parseIgnoredLiveRects(undefined), []);
});

test("reference overlay rectangles reject unsafe or malformed shapes", () => {
  for (const value of ["1,2,0,4", "1,2,-3,4", "1,2,3", "1,2,3,4.5", "x,2,3,4"]) {
    assert.throws(() => parseIgnoredLiveRects(value), /invalid LM_COMPARE_IGNORE_LIVE_RECTS/);
  }
});
