import test from "node:test";
import assert from "node:assert/strict";
import { buildRequirements, route } from "./server.mjs";

test("buildRequirements returns the configured exact payment requirement", () => {
  const requirements = buildRequirements({
    network: "custom-network",
    payTo: "custom-payee",
    asset: "TOKEN",
    amount: "42",
    resource: "http://example.test/paid",
  });

  assert.equal(requirements.accepts[0].maxAmountRequired, "42");
  assert.equal(requirements.accepts[0].payTo, "custom-payee");
  assert.equal(requirements.accepts[0].asset, "TOKEN");
  assert.equal(requirements.accepts[0].network, "custom-network");
  assert.equal(requirements.accepts[0].scheme, "exact");
});

test("route returns payment requirements for an unpaid GET", () => {
  const result = route({
    method: "GET",
    hasPayment: false,
    resource: "http://x/",
    config: { network: "hedera-mainnet", payTo: "0.0.2", asset: "HBAR", amount: "10000000" },
  });

  assert.equal(result.status, 402);
  assert.equal(result.body.accepts[0].payTo, "0.0.2");
});

test("route returns premium data for a paid GET", () => {
  const result = route({
    method: "GET",
    hasPayment: true,
    resource: "http://x/",
    config: { network: "hedera-mainnet", payTo: "0.0.2", asset: "HBAR", amount: "10000000" },
  });

  assert.equal(result.status, 200);
  assert.equal(result.body.paid, true);
});

test("route rejects non-GET requests", () => {
  const result = route({
    method: "POST",
    hasPayment: false,
    resource: "http://x/",
    config: { network: "hedera-mainnet", payTo: "0.0.2", asset: "HBAR", amount: "10000000" },
  });

  assert.equal(result.status, 405);
});
