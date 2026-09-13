import test from "node:test";
import assert from "node:assert/strict";
import { hbar, tinybars } from "./cli.mjs";

test("converts tinybars to exact eight-decimal HBAR", () => {
  const tiny = 1663012637744658n;
  assert.equal(hbar(tiny), "16630126.37744658");
  assert.equal(tinybars("16630126.37744658"), tiny);
});

test("preserves zero padding in fractional units", () => {
  assert.equal(hbar(100000001n), "1.00000001");
});
