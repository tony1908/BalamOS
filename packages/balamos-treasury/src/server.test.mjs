import test from "node:test";
import assert from "node:assert/strict";
import { accountSnapshot, treasury } from "./server.mjs";

test("accountSnapshot reads balance and recent txns from the mirror node", async () => {
  const fetcher = async (url) => {
    if (url.includes("/accounts/")) return { ok: true, json: async () => ({ balance: { balance: 250000000 } }) };
    if (url.includes("/transactions")) return { ok: true, json: async () => ({ transactions: [{ transaction_id: "0.0.1@1.0", result: "SUCCESS", name: "CRYPTOTRANSFER" }] }) };
    return { ok: false };
  };
  const snap = await accountSnapshot("0.0.10512454", fetcher);
  assert.equal(snap.hbar, 2.5);
  assert.equal(snap.label, "Agent wallet");
  assert.equal(snap.recent.length, 1);
  assert.equal(snap.recent[0].result, "SUCCESS");
  assert.match(snap.hashscan, /hashscan\.io/);
});

test("accountSnapshot stays resilient when the mirror node is unreachable", async () => {
  const fetcher = async () => { throw new Error("network down"); };
  const snap = await accountSnapshot("0.0.99", fetcher);
  assert.equal(snap.hbar, null);
  assert.deepEqual(snap.recent, []);
});

test("treasury aggregates all configured accounts", async () => {
  const fetcher = async (url) => url.includes("/accounts/")
    ? { ok: true, json: async () => ({ balance: { balance: 100000000 } }) }
    : { ok: true, json: async () => ({ transactions: [] }) };
  const data = await treasury(["0.0.1", "0.0.2"], fetcher);
  assert.equal(data.accounts.length, 2);
  assert.equal(data.accounts[0].hbar, 1);
});
