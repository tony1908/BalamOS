import test from "node:test";
import assert from "node:assert/strict";
import { decodePaymentHeader, encodePaymentHeader, selectRequirement } from "./cli.mjs";

test("encodes and decodes a payment header as base64 JSON", () => {
  const value = { x402Version: 1, scheme: "exact", network: "hedera-testnet", payload: { a: 1 } };
  assert.deepEqual(decodePaymentHeader(encodePaymentHeader(value)), value);
});

test("selects the first compatible payment requirement", () => {
  const accepts = [
    { scheme: "exact", network: "eip155:1", asset: "USDC" },
    { scheme: "exact", network: "hedera:mainnet", asset: "HBAR" },
  ];
  assert.deepEqual(selectRequirement(accepts, { network: "hedera:mainnet" }), accepts[1]);
});

test("returns null for empty requirements", () => {
  assert.equal(selectRequirement([], {}), null);
});

test("selects the first requirement with no filters", () => {
  const requirement = { scheme: "exact", network: "base" };
  assert.deepEqual(selectRequirement([requirement]), requirement);
});

test("rejects malformed headers", () => {
  assert.throws(() => decodePaymentHeader("not!base64!json"), /invalid X-PAYMENT header/);
});
