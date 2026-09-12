import test from "node:test";
import assert from "node:assert/strict";
import { settleHbarTransfer } from "./settle.mjs";

test("rejects a non-numeric operator id", async () => {
  await assert.rejects(() => settleHbarTransfer({ operatorId: "bad", payTo: "0.0.3", amountTinybars: "1", client: {} }), /operatorId/);
});

test("rejects a payTo equal to the operator", async () => {
  await assert.rejects(() => settleHbarTransfer({ operatorId: "0.0.2", payTo: "0.0.2", amountTinybars: "1", client: {} }), /must differ/);
});

test("rejects a non-positive amount", async () => {
  await assert.rejects(() => settleHbarTransfer({ operatorId: "0.0.2", payTo: "0.0.3", amountTinybars: "0", client: {} }), /positive/);
});

test("rejects an over-long memo", async () => {
  await assert.rejects(() => settleHbarTransfer({ operatorId: "0.0.2", payTo: "0.0.3", amountTinybars: "1", memo: "x".repeat(101), client: {} }), /memo too long/);
});
