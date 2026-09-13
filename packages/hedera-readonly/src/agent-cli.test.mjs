import test from "node:test";
import assert from "node:assert/strict";
import { hbarToTinybars } from "./agent-cli.mjs";
test("converts HBAR exactly to integer tinybars", () => { assert.equal(hbarToTinybars("1.00000001"), "100000001"); assert.equal(hbarToTinybars("12"), "1200000000"); });
test("rejects fractional precision and malformed amounts", () => { assert.throws(() => hbarToTinybars("1.000000001")); assert.throws(() => hbarToTinybars("1e2")); assert.throws(() => hbarToTinybars("-1")); });
