// Verify an x402 Hedera payment against the public mirror node — no API key needed.
const mirrorBase = (network) => `https://${String(network).includes("mainnet") ? "mainnet" : "testnet"}.mirrornode.hedera.com/api/v1`;

export function toMirrorId(txId) {
  const m = /^(0\.0\.[0-9]+)@([0-9]+)\.([0-9]{1,9})$/.exec(String(txId ?? ""));
  return m ? `${m[1]}-${m[2]}-${m[3].padEnd(9, "0")}` : null;
}

export async function verifyHederaPayment({
  txId,
  payTo,
  minTinybars,
  network = "hedera-testnet",
  fetcher = fetch,
  attempts = 6,
  delayMs = 2500,
  sleep = (ms) => new Promise((r) => setTimeout(r, ms)),
}) {
  const mirrorId = toMirrorId(txId);
  if (!mirrorId) return { ok: false, reason: "invalid transaction id" };
  if (!/^0\.0\.[1-9][0-9]{0,9}$/.test(String(payTo ?? ""))) return { ok: false, reason: "invalid payTo" };
  const need = BigInt(minTinybars);
  const url = `${mirrorBase(network)}/transactions/${encodeURIComponent(mirrorId)}`;
  for (let i = 0; i < attempts; i += 1) {
    let data;
    try {
      const res = await fetcher(url, { signal: AbortSignal.timeout(8000) });
      if (res.ok) data = await res.json();
    } catch { /* transient; retry */ }
    const rows = Array.isArray(data?.transactions) ? data.transactions : [];
    const row = rows.find((r) => r?.result === "SUCCESS") ?? rows[0];
    if (row) {
      if (row.result !== "SUCCESS") return { ok: false, reason: `consensus ${row.result}`, tx: row };
      const credited = (row.transfers ?? [])
        .filter((t) => t.account === payTo)
        .reduce((sum, t) => sum + BigInt(t.amount), 0n);
      if (credited >= need) return { ok: true, creditedTinybars: credited.toString(), tx: row };
      return { ok: false, reason: `paid ${credited} tinybars, need ${need}`, tx: row };
    }
    if (i < attempts - 1) await sleep(delayMs);
  }
  return { ok: false, reason: "payment not found on mirror node (not yet indexed?)" };
}
