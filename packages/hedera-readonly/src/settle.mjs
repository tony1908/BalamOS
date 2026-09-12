import { Client, PrivateKey, AccountId, TransferTransaction, Hbar } from "@hiero-ledger/sdk";

// Direct operator-signed HBAR transfer settlement (testnet by default).
// Used by the x402 pay flow: the agent wallet settles a payment to payTo.
// The operator key is never logged or returned.
export async function settleHbarTransfer({ operatorId, operatorKey, payTo, amountTinybars, network = "testnet", memo, client } = {}) {
  if (!/^0\.0\.[1-9][0-9]{0,9}$/.test(String(operatorId ?? ""))) throw new Error("operatorId must be a numeric account id");
  if (!/^0\.0\.[1-9][0-9]{0,9}$/.test(String(payTo ?? ""))) throw new Error("payTo must be a numeric account id");
  if (String(operatorId) === String(payTo)) throw new Error("operator and payTo must differ");
  const amount = BigInt(amountTinybars);
  if (!(amount > 0n)) throw new Error("amountTinybars must be positive");
  if (typeof memo === "string" && Buffer.byteLength(memo, "utf8") > 100) throw new Error("memo too long");
  const owned = !client;
  client ??= (network === "mainnet" ? Client.forMainnet() : Client.forTestnet())
    .setOperator(AccountId.fromString(operatorId), PrivateKey.fromStringECDSA(operatorKey));
  try {
    let tx = new TransferTransaction()
      .addHbarTransfer(AccountId.fromString(operatorId), Hbar.fromTinybars((-amount).toString()))
      .addHbarTransfer(AccountId.fromString(payTo), Hbar.fromTinybars(amount.toString()));
    if (memo) tx = tx.setTransactionMemo(memo);
    const response = await tx.execute(client);
    const receipt = await response.getReceipt(client);
    const status = receipt.status?.toString?.() ?? String(receipt.status);
    const transactionId = response.transactionId.toString();
    return {
      ok: status === "SUCCESS",
      status,
      transactionId,
      payer: operatorId,
      payTo,
      amountTinybars: amount.toString(),
      network,
      hashscanUrl: `https://hashscan.io/${network}/transaction/${transactionId}`,
    };
  } finally { if (owned) client.close(); }
}
