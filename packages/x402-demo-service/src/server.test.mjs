import test from "node:test";
import assert from "node:assert/strict";
import { buildRequirements, route } from "./server.mjs";
import { verifyHederaPayment, toMirrorId } from "./verify.mjs";

const config = { network: "hedera-testnet", payTo: "0.0.10512599", asset: "HBAR", amount: "1000000" };
const b64 = (o) => Buffer.from(JSON.stringify(o), "utf8").toString("base64");

test("buildRequirements returns the configured exact requirement", () => {
  const r = buildRequirements({ ...config, resource: "http://x/" });
  assert.equal(r.accepts[0].scheme, "exact");
  assert.equal(r.accepts[0].payTo, "0.0.10512599");
  assert.equal(r.accepts[0].maxAmountRequired, "1000000");
  assert.equal(r.accepts[0].amount, "1000000");
});

test("route returns 402 for an unpaid GET", async () => {
  const r = await route({ method: "GET", paymentHeader: "", resource: "http://x/", config });
  assert.equal(r.status, 402);
  assert.equal(r.body.accepts[0].payTo, "0.0.10512599");
});

test("route rejects non-GET", async () => {
  const r = await route({ method: "POST", paymentHeader: "", resource: "http://x/", config });
  assert.equal(r.status, 405);
});

test("route 402s an invalid X-PAYMENT header", async () => {
  const r = await route({ method: "GET", paymentHeader: "@@notbase64@@", resource: "http://x/", config });
  assert.equal(r.status, 402);
  assert.match(r.body.error, /invalid X-PAYMENT/);
});

test("route settles 200 when verification passes", async () => {
  const verify = async () => ({ ok: true, creditedTinybars: "1000000" });
  const r = await route({ method: "GET", paymentHeader: b64({ txId: "0.0.1@1.0" }), resource: "http://x/", config, verify });
  assert.equal(r.status, 200);
  assert.equal(r.body.paid, true);
  assert.equal(r.body.settlement.creditedTinybars, "1000000");
});

test("route 402s when verification fails", async () => {
  const verify = async () => ({ ok: false, reason: "payment not found" });
  const r = await route({ method: "GET", paymentHeader: b64({ txId: "0.0.1@1.0" }), resource: "http://x/", config, verify });
  assert.equal(r.status, 402);
  assert.match(r.body.error, /payment not verified/);
});

test("toMirrorId converts a transaction id", () => {
  assert.equal(toMirrorId("0.0.10512454@1789256708.070890425"), "0.0.10512454-1789256708-070890425");
  assert.equal(toMirrorId("garbage"), null);
});

test("verifyHederaPayment credits payTo and compares to the required amount", async () => {
  const row = { result: "SUCCESS", transfers: [ { account: "0.0.10512454", amount: -1000000 }, { account: "0.0.10512599", amount: 1000000 } ] };
  const fetcher = async () => ({ ok: true, json: async () => ({ transactions: [row] }) });
  const paid = await verifyHederaPayment({ txId: "0.0.10512454@1.0", payTo: "0.0.10512599", minTinybars: "1000000", fetcher });
  assert.equal(paid.ok, true);
  assert.equal(paid.creditedTinybars, "1000000");
  const short = await verifyHederaPayment({ txId: "0.0.10512454@1.0", payTo: "0.0.10512599", minTinybars: "2000000", fetcher });
  assert.equal(short.ok, false);
});

test("verifyHederaPayment rejects a failed consensus result", async () => {
  const fetcher = async () => ({ ok: true, json: async () => ({ transactions: [{ result: "INSUFFICIENT_ACCOUNT_BALANCE", transfers: [] }] }) });
  const r = await verifyHederaPayment({ txId: "0.0.1@1.0", payTo: "0.0.10512599", minTinybars: "1", fetcher });
  assert.equal(r.ok, false);
});

test("verifyHederaPayment gives up when the tx never indexes", async () => {
  const fetcher = async () => ({ ok: false });
  const r = await verifyHederaPayment({ txId: "0.0.1@1.0", payTo: "0.0.10512599", minTinybars: "1", fetcher, attempts: 2, delayMs: 0, sleep: async () => {} });
  assert.equal(r.ok, false);
  assert.match(r.reason, /not found/);
});
