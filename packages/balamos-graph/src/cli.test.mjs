import test from "node:test";
import assert from "node:assert/strict";
import { parseVars, resolveEndpoint } from "./cli.mjs";

test("resolves subgraph IDs through the decentralized gateway", () => {
  assert.equal(resolveEndpoint("QmSubgraphId123", "KEY"), "https://gateway.thegraph.com/api/KEY/subgraphs/id/QmSubgraphId123");
});

test("preserves an explicit query URL", () => {
  assert.equal(resolveEndpoint("https://example.com/x", "KEY"), "https://example.com/x");
});

test("rejects implausible subgraph IDs", () => {
  assert.throws(() => resolveEndpoint("bad id!!", "KEY"), /invalid subgraph id/);
});

test("requires an API key for subgraph IDs", () => {
  assert.throws(() => resolveEndpoint("QmId", ""), /GRAPH_API_KEY is required to query by subgraph id/);
});

test("parses JSON variable values and preserves strings", () => {
  assert.deepEqual(parseVars(["--var", "a=1", "--var", "b=hello"]), { a: 1, b: "hello" });
});
