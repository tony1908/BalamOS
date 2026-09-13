import test from "node:test";
import assert from "node:assert/strict";
import {
  buildTransaction,
  validateSignedTransaction,
  reconcileTransaction,
  submitTransaction,
} from "./transactions.mjs";
import { PrivateKey } from "@hiero-ledger/sdk";
import { proto } from "@hiero-ledger/proto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const request = {
  source_account_id: "0.0.10",
  recipient_account_id: "0.0.11",
  amount_tinybars: "100",
  max_fee_tinybars: "100000000",
  transaction_id: "0.0.10@1990000000.000000000",
  memo: "test",
};

test("builds real frozen SDK bytes and validates an ephemeral signature", async () => {
  const built = buildTransaction(request);
  assert.match(built.unsigned_bytes, /^[A-Za-z0-9+/]+=*$/);
  assert.equal(built.transaction_id, request.transaction_id);
  assert.equal(built.expiry_unix_ms, 1990000000000 + 120000);
  const key = PrivateKey.generateED25519();
  const signed = await built.transaction.sign(key);
  const signedBytes = Buffer.from(signed.toBytes()).toString("base64");
  const result = validateSignedTransaction({
    unsigned_bytes: built.unsigned_bytes,
    signed_bytes: signedBytes,
    now_unix_ms: 1770000000000,
  });
  assert.equal(result.valid, true);
});

test("encodes exact tinybar amounts in protobuf", () => {
  for (const amount of ["1", "100000000", "9007199254740993"]) {
    const built = buildTransaction({ ...request, amount_tinybars: amount });
    const entry = proto.TransactionList.decode(Buffer.from(built.unsigned_bytes, "base64")).transactionList[0];
    const signed = proto.SignedTransaction.decode(entry.signedTransactionBytes);
    const body = proto.TransactionBody.decode(signed.bodyBytes);
    const amounts = body.cryptoTransfer.transfers.accountAmounts.map(({ amount: value }) => value.toString());
    assert.deepEqual(amounts, [`-${amount}`, amount]);
  }
});

test("rejects changed body and unsigned signed bytes", async () => {
  const built = buildTransaction(request);
  const key = PrivateKey.generateED25519();
  const signed = await built.transaction.sign(key);
  const bytes = Buffer.from(signed.toBytes());
  assert.throws(() => validateSignedTransaction({
    unsigned_bytes: built.unsigned_bytes,
    signed_bytes: Buffer.from(bytes.subarray(0, -1)).toString("base64"),
  }));
  assert.throws(() => validateSignedTransaction({
    unsigned_bytes: built.unsigned_bytes,
    signed_bytes: built.unsigned_bytes,
  }));
});

test("validates both ephemeral key types and rejects mutated frozen fields", async () => {
  for (const key of [PrivateKey.generateED25519(), PrivateKey.generateECDSA()]) {
    const built = buildTransaction(request);
    const signed = await built.transaction.sign(key);
    const signedBytes = Buffer.from(signed.toBytes()).toString("base64");
    assert.equal(validateSignedTransaction({ unsigned_bytes: built.unsigned_bytes, signed_bytes: signedBytes, now_unix_ms: 1770000000000 }).valid, true);
    for (const field of ["recipient_account_id", "amount_tinybars", "max_fee_tinybars", "transaction_id", "node_account_id", "memo"]) {
      if (field === "node_account_id") { assert.throws(() => buildTransaction({ ...request, node_account_id: "0.0.4" })); continue; }
      const changed = buildTransaction({ ...request, [field]: field === "recipient_account_id" ? "0.0.12" : field === "amount_tinybars" ? "101" : field === "max_fee_tinybars" ? "100000001" : field === "transaction_id" ? "0.0.10@1990000001.000000000" : "changed" });
      const changedSigned = await changed.transaction.sign(key);
      assert.throws(() => validateSignedTransaction({ unsigned_bytes: built.unsigned_bytes, signed_bytes: Buffer.from(changedSigned.toBytes()).toString("base64"), now_unix_ms: 1770000000000 }), field);
    }
  }
});

test("reconciles only exact mirror IDs and normalizes statuses", async () => {
  const calls = [];
  const fetcher = async (url) => { calls.push(url); return { ok: true, status: 200, json: async () => ({ transactions: [{ transaction_id: "0.0.10-1770000000-123456789", result: "SUCCESS" }] }) }; };
  assert.deepEqual(await reconcileTransaction("0.0.10@1770000000.123456789", fetcher), { transaction_id: "0.0.10@1770000000.123456789", status: "confirmed", result: "SUCCESS" });
  assert.equal(calls[0], "https://mainnet.mirrornode.hedera.com/api/v1/transactions/0.0.10-1770000000-123456789");
  for (const rows of [[{ transaction_id: "0.0.10-1770000000-123456789", result: "DUPLICATE_TRANSACTION" }], [{ result: "SUCCESS" }], [{ transaction_id: "wrong", result: "SUCCESS" }]]) {
    const result = await reconcileTransaction("0.0.10@1770000000.123456789", async () => ({ ok: true, status: 200, json: async () => ({ transactions: rows }) }));
    assert.equal(result.status, "unknown");
  }
  assert.equal((await reconcileTransaction("0.0.10@1770000000.123456789", async () => ({ ok: false, status: 404 }))).status, "unknown");
});

test("submits through injected transport without changing the validated ID", async () => {
  const built = buildTransaction(request);
  const key = PrivateKey.generateED25519();
  const signed = await built.transaction.sign(key);
  const input = { unsigned_bytes: built.unsigned_bytes, signed_bytes: Buffer.from(signed.toBytes()).toString("base64"), now_unix_ms: 1770000000000 };
  let calls = 0;
  const result = await submitTransaction(input, { close() {} }, Date.now, { timeoutMs: 20, execute: async () => { calls++; throw new Error("offline"); } });
  assert.equal(calls, 1);
  assert.deepEqual(result, { transaction_id: request.transaction_id, status: "unknown" });
});

test("rejects malformed signed payloads before executing", async () => {
  let calls = 0;
  await assert.rejects(() => submitTransaction({ unsigned_bytes: request.transaction_id, signed_bytes: request.transaction_id }, { close() {} }, Date.now, { execute: async () => { calls++; } }));
  assert.equal(calls, 0);
});

test("floors expiry milliseconds and preserves nanosecond valid starts", async () => {
  const built = buildTransaction({ ...request, transaction_id: "0.0.10@1990000000.123456789" });
  assert.equal(built.expiry_unix_ms, 1990000000123 + 120000);
  assert.equal(Number.isInteger(built.expiry_unix_ms), true);
  const body = proto.SignedTransaction.decode(proto.TransactionList.decode(Buffer.from(built.unsigned_bytes, "base64")).transactionList[0].signedTransactionBytes);
  assert.equal(body.bodyBytes && proto.TransactionBody.decode(body.bodyBytes).transactionID.transactionValidStart.nanos.toString(), "123456789");
  const signed = await built.transaction.sign(PrivateKey.generateED25519());
  assert.equal(validateSignedTransaction({
    unsigned_bytes: built.unsigned_bytes,
    signed_bytes: Buffer.from(signed.toBytes()).toString("base64"),
    now_unix_ms: built.expiry_unix_ms - 1,
  }).valid, true);
});

test("supports bare and wrapped offline CLI build and validate", async () => {
  const cli = fileURLToPath(new URL("./cli.mjs", import.meta.url));
  const build = spawnSync(process.execPath, [cli, "transactions", "build"], { input: JSON.stringify(request), encoding: "utf8" });
  assert.equal(build.status, 0);
  const built = JSON.parse(build.stdout);
  const validate = spawnSync(process.execPath, [cli, "validate"], { input: JSON.stringify({ unsigned_bytes: built.unsigned_bytes, signed_bytes: built.unsigned_bytes }), encoding: "utf8" });
  assert.notEqual(validate.status, 0);
});
