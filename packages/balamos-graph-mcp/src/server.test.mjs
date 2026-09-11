import assert from "node:assert/strict";
import test from "node:test";
import { handleMessage, resolveEndpoint } from "./server.mjs";

test("initialize returns server info and protocol version", async () => {
  const response = await handleMessage({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18" } });
  assert.equal(response.result.serverInfo.name, "balamos-graph-mcp");
  assert.equal(response.result.protocolVersion, "2025-06-18");
});

test("notification returns null", async () => {
  const response = await handleMessage({ jsonrpc: "2.0", method: "notifications/initialized" });
  assert.equal(response, null);
});

test("tools/list exposes query_subgraph", async () => {
  const response = await handleMessage({ jsonrpc: "2.0", id: 2, method: "tools/list" });
  assert.equal(response.result.tools[0].name, "query_subgraph");
});

test("tools/call happy path returns query result", async () => {
  process.env.GRAPH_API_KEY = "KEY";
  const fetchImpl = async () => ({ ok: true, json: async () => ({ data: { ok: true } }) });
  const response = await handleMessage(
    { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "query_subgraph", arguments: { subgraph: "Qm123", query: "{ x }" } } },
    { fetchImpl }
  );
  assert.ok(response.result.content[0].text.includes("ok"));
});

test("unknown tool returns -32602", async () => {
  const response = await handleMessage({ jsonrpc: "2.0", id: 4, method: "tools/call", params: { name: "nope", arguments: {} } });
  assert.equal(response.error.code, -32602);
});

test("unknown method returns -32601", async () => {
  const response = await handleMessage({ jsonrpc: "2.0", id: 5, method: "bogus" });
  assert.equal(response.error.code, -32601);
});

test("resolveEndpoint builds gateway url and passes through http urls", () => {
  assert.equal(resolveEndpoint("Qm1", "KEY"), "https://gateway.thegraph.com/api/KEY/subgraphs/id/Qm1");
  assert.equal(resolveEndpoint("https://x", "KEY"), "https://x");
});
