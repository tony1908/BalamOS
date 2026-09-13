import {
  AccountId,
  Client,
  Hbar,
  PublicKey,
  Transaction,
  TransactionId,
  TransferTransaction,
} from "@hiero-ledger/sdk";
import { proto } from "@hiero-ledger/proto";
import { createHash } from "node:crypto";

const NODE = "0.0.3";
const MAX_INT64 = 9223372036854775807n;
const MIRROR = "https://mainnet.mirrornode.hedera.com/api/v1";

function account(value, label) {
  if (typeof value !== "string" || !/^0\.0\.[1-9][0-9]{0,9}$/.test(value)) throw new Error(`${label} must be a numeric mainnet account ID`);
  return AccountId.fromString(value);
}

function integer(value, label, positive = false) {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]*)$/.test(value)) throw new Error(`${label} must be an exact integer tinybar amount`);
  const result = BigInt(value);
  if (result > MAX_INT64 || (positive && result === 0n)) throw new Error(`${label} is outside the supported range`);
  return result;
}

function decode(value, label) {
  if (typeof value !== "string" || value.length > 65536 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) throw new Error(`${label} must be canonical base64`);
  const bytes = Buffer.from(value, "base64");
  if (bytes.length > 49152 || bytes.toString("base64") !== value) throw new Error(`${label} must be canonical base64`);
  return bytes;
}

function bodyAndSignatures(bytes, label) {
  const list = proto.TransactionList.decode(bytes).transactionList;
  if (list.length !== 1) throw new Error(`${label} must contain exactly one transaction`);
  const entry = list[0];
  if (entry.bodyBytes?.length && entry.signedTransactionBytes?.length) throw new Error(`${label} has ambiguous transaction fields`);
  const signed = entry.signedTransactionBytes?.length ? proto.SignedTransaction.decode(entry.signedTransactionBytes) : entry;
  if (!signed.bodyBytes?.length) throw new Error(`${label} is missing a transaction body`);
  return signed;
}

export function buildTransaction(input) {
  const allowed = new Set(["source_account_id", "recipient_account_id", "amount_tinybars", "max_fee_tinybars", "node_account_id", "transaction_id", "memo"]);
  for (const key of Object.keys(input ?? {})) if (!allowed.has(key)) throw new Error(`${key} is not allowed`);
  const source = account(input?.source_account_id, "source_account_id");
  const recipient = account(input?.recipient_account_id, "recipient_account_id");
  if (source.toString() === recipient.toString()) throw new Error("source and recipient must differ");
  const amount = integer(input?.amount_tinybars, "amount_tinybars", true);
  const fee = integer(input?.max_fee_tinybars, "max_fee_tinybars");
  const node = account(input?.node_account_id ?? NODE, "node_account_id");
  if (node.toString() !== NODE) throw new Error("node_account_id must be the mainnet node 0.0.3");
  if (typeof input?.transaction_id !== "string") throw new Error("transaction_id is required");
  const transactionId = TransactionId.fromString(input.transaction_id);
  if (transactionId.accountId.toString() !== source.toString()) throw new Error("transaction_id payer must equal source_account_id");
  if (fee === 0n) throw new Error("max_fee_tinybars must be positive");
  if (typeof input?.memo !== "undefined" && (typeof input.memo !== "string" || Buffer.byteLength(input.memo, "utf8") > 100)) throw new Error("memo is invalid");
  const transaction = new TransferTransaction()
    .addHbarTransfer(source, Hbar.fromTinybars(`-${amount}`))
    .addHbarTransfer(recipient, Hbar.fromTinybars(amount.toString()))
    .setTransactionId(transactionId)
     .setNodeAccountIds([node])
     .setMaxTransactionFee(Hbar.fromTinybars(fee.toString()))
     .setTransactionValidDuration(120);
  transaction.setRegenerateTransactionId(false);
  transaction.setMaxAttempts(1);
  if (input.memo) transaction.setTransactionMemo(input.memo);
  transaction.freeze();
  const bytes = transaction.toBytes();
  const seconds = BigInt(transactionId.validStart.seconds.toString());
  const nanos = BigInt(transactionId.validStart.nanos?.toString?.() ?? 0);
  return {
    unsigned_bytes: Buffer.from(bytes).toString("base64"),
    sha256: createHash("sha256").update(bytes).digest("hex"),
    transaction_id: transactionId.toString(),
    expiry_unix_ms: Number(seconds * 1000n + nanos / 1_000_000n + 120000n),
    transaction,
  };
}

export function validateSignedTransaction(input) {
  const unsigned = decode(input?.unsigned_bytes, "unsigned_bytes");
  const signedBytes = decode(input?.signed_bytes, "signed_bytes");
  const original = bodyAndSignatures(unsigned, "unsigned_bytes");
  const signed = bodyAndSignatures(signedBytes, "signed_bytes");
  if (original.sigMap?.sigPair?.length) throw new Error("unsigned transaction must not contain signatures");
  if (!Buffer.from(original.bodyBytes).equals(Buffer.from(signed.bodyBytes))) throw new Error("signed transaction body differs from the frozen transaction");
  if (!signed.sigMap?.sigPair?.length) throw new Error("signed transaction has no signatures");
  const body = proto.TransactionBody.decode(signed.bodyBytes);
  if (Buffer.from(original.bodyBytes).toString("base64") !== Buffer.from(signed.bodyBytes).toString("base64")) throw new Error("signed transaction body differs from the frozen transaction");
  const long = (value) => BigInt(value?.toString?.() ?? value ?? 0);
  if (!body.transactionID || body.transactionID.nonce !== 0 || long(body.nodeAccountID?.shardNum) !== 0n || long(body.nodeAccountID?.realmNum) !== 0n || long(body.nodeAccountID?.accountNum) !== 3n) throw new Error("invalid transaction identity");
  if (body.scheduled || body.transactionID.scheduled || body.transactionID.nonce) throw new Error("scheduled and nonce transactions are unsupported");
  if (long(body.transactionFee) <= 0n || long(body.transactionValidDuration?.seconds) !== 120n) throw new Error("invalid fee or duration");
  const now = input?.now_unix_ms ?? Date.now();
  const seconds = long(body.transactionID.transactionValidStart?.seconds ?? body.transactionID.validStart?.seconds);
  const nanos = long(body.transactionID.transactionValidStart?.nanos);
  if (seconds * 1000n + nanos / 1_000_000n + 120000n <= BigInt(now)) throw new Error("transaction has expired");
  if (Buffer.byteLength(body.memo ?? "", "utf8") > 100) throw new Error("invalid memo");
  if (!body.cryptoTransfer || body.batchKey || body.highVolume || body.cryptoTransfer.tokenTransfers?.length || body.cryptoTransfer.allowanceAdjustments?.length || body.cryptoTransfer.isApproval || body.cryptoTransfer.hookCall) throw new Error("unsupported transaction operation");
  const transfers = body.cryptoTransfer.transfers?.accountAmounts ?? [];
  if (transfers.length !== 2) throw new Error("transaction must contain exactly two transfers");
  if (transfers.some((transfer) => transfer.isApproval || transfer.hookCall)) throw new Error("unsupported transfer flags");
  const payer = AccountId._fromProtobuf(body.transactionID.accountID).toString();
  // Payer-key lookup is intentionally offline; mainnet consensus validates it and the UI checks the signer account.
  const debit = transfers.find((x) => AccountId._fromProtobuf(x.accountID).toString() === payer);
  const credit = transfers.find((x) => AccountId._fromProtobuf(x.accountID).toString() !== payer);
  const debitAmount = long(debit?.amount);
  const creditAmount = long(credit?.amount);
  if (!debit || !credit || debitAmount >= 0n || creditAmount <= 0n || debitAmount !== -creditAmount || creditAmount > MAX_INT64) throw new Error("invalid HBAR transfers");
  for (const pair of signed.sigMap.sigPair) {
    const isEd = pair.ed25519?.length > 0;
    const isEcdsa = pair.ECDSASecp256k1?.length > 0;
    if (isEd === isEcdsa || !pair.pubKeyPrefix?.length) throw new Error("signature must include one typed public key and signature");
    const signature = isEd ? pair.ed25519 : pair.ECDSASecp256k1;
    try {
      const key = isEd ? PublicKey.fromBytesED25519(pair.pubKeyPrefix) : PublicKey.fromBytesECDSA(pair.pubKeyPrefix);
      if (!key.verify(signed.bodyBytes, signature)) throw new Error("invalid signature");
    } catch (error) { throw new Error(`invalid signature: ${error.message}`); }
  }
  return { valid: true, transaction_id: TransactionId._fromProtobuf(body.transactionID).toString(), sha256: createHash("sha256").update(signedBytes).digest("hex") };
}

export async function submitTransaction(input, client, now = Date.now, transport) {
  const owned = !client;
  const validated = validateSignedTransaction(input);
  client ??= Client.forMainnet();
  const timeout = (promise) => new Promise((resolve, reject) => { const timer = setTimeout(() => reject(new Error("timeout")), transport?.timeoutMs ?? 8000); promise.then(resolve, reject).finally(() => clearTimeout(timer)); });
  try {
    const transaction = Transaction.fromBytes(decode(input.signed_bytes, "signed_bytes"));
    for (const [method, field, value] of [["setRegenerateTransactionId", "_regenerateTransactionId", false], ["setMaxAttempts", "_maxAttempts", 1]]) {
      try { transaction[method](value); } catch (error) {
        if (!String(error?.message).includes("immutable")) throw error;
        transaction[field] = value;
      }
    }
    const execute = transport?.execute ?? ((tx, c) => tx.execute(c));
    const receipt = transport?.receipt ?? ((response, c) => response.getReceipt(c));
    const response = await timeout(execute(transaction, client));
    const id = validated.transaction_id;
    try {
      const result = await timeout(receipt(response, client));
      const status = result.status?.toString?.() ?? String(result.status ?? "UNKNOWN");
      const definitive = new Set(["INVALID_ACCOUNT_ID", "INVALID_NODE_ACCOUNT", "INVALID_TRANSACTION", "INVALID_SIGNATURE", "INSUFFICIENT_PAYER_BALANCE", "DUPLICATE_TRANSACTION", "TRANSACTION_EXPIRED", "MEMO_TOO_LONG"]);
      return { transaction_id: id, status: status === "SUCCESS" ? "confirmed" : definitive.has(status) ? "failed" : "unknown", result: status };
    }
    catch { return { transaction_id: id, status: "unknown" }; }
  } catch { return { transaction_id: validated?.transaction_id ?? input.transaction_id, status: "unknown" }; }
  finally { if (owned) client.close(); }
}

export async function reconcileTransaction(transactionId, fetcher = fetch) {
  const match = /^(0\.0\.[0-9]+)@([0-9]+)\.([0-9]{1,9})$/.exec(transactionId);
  if (!match) return { transaction_id: transactionId, status: "unknown" };
  const mirrorId = `${match[1]}-${match[2]}-${match[3].padEnd(9, "0")}`;
  try {
    const response = await fetcher(`${MIRROR}/transactions/${encodeURIComponent(mirrorId)}`, { signal: AbortSignal.timeout(8000) });
    if (response.status === 404 || !response.ok) return { transaction_id: transactionId, status: "unknown" };
    const data = await response.json();
    const rows = Array.isArray(data?.transactions) ? data.transactions : [];
    const matching = rows.filter((row) => row?.transaction_id === mirrorId);
    const result = matching.find((x) => x?.result === "SUCCESS")?.result ?? matching.find((x) => typeof x?.result === "string" && x.result)?.result;
    return { transaction_id: transactionId, status: result === "SUCCESS" ? "confirmed" : "unknown", result: result ?? null };
  } catch { return { transaction_id: transactionId, status: "unknown" }; }
}
