import { readFileSync } from "node:fs";

const allowed = new Set(["ready", "inspect", "pay"]);
const fail = (message) => { process.stderr.write(`${message}\n`); process.exitCode = 1; };

export function encodePaymentHeader(payment) {
  return Buffer.from(JSON.stringify(payment), "utf8").toString("base64");
}

export function decodePaymentHeader(b64) {
  try {
    return JSON.parse(Buffer.from(b64, "base64").toString("utf8"));
  } catch {
    throw new Error("invalid X-PAYMENT header");
  }
}

export function selectRequirement(accepts, { network, scheme } = {}) {
  if (!Array.isArray(accepts) || accepts.length === 0) return null;
  return accepts.find((requirement) =>
    (network === undefined || requirement.network === network) &&
    (scheme === undefined || requirement.scheme === scheme)
  ) ?? null;
}

function loadOperator() {
  const file = process.env.HEDERA_KEY_FILE;
  if (file) {
    const k = JSON.parse(readFileSync(file, "utf8"));
    if (!k.accountId || !k.privateKeyRaw) throw new Error("HEDERA_KEY_FILE must contain accountId and privateKeyRaw");
    return { operatorId: k.accountId, operatorKey: k.privateKeyRaw };
  }
  const operatorId = process.env.HEDERA_OPERATOR_ID;
  const operatorKey = process.env.HEDERA_OPERATOR_KEY;
  if (!operatorId || !operatorKey) throw new Error("set HEDERA_KEY_FILE, or HEDERA_OPERATOR_ID + HEDERA_OPERATOR_KEY");
  return { operatorId, operatorKey };
}

async function main() {
  const [method, url] = process.argv.slice(2);
  if (!allowed.has(method)) throw new Error("method is not allowed");
  if (method === "ready") {
    console.log(JSON.stringify({ ready: true, helper_version: "0.2.0", protocol: "x402", capabilities: ["inspect", "pay"], settlement: "hedera-hbar", message: "x402 helper ready (inspect + Hedera HBAR settlement)" }));
    return;
  }

  if (method === "inspect") {
    const response = await fetch(url, { signal: AbortSignal.timeout(15000) });
    if (response.status !== 402) {
      console.log(JSON.stringify({ status: response.status, payment_required: false, message: "no payment required" }));
      return;
    }
    let body;
    try { body = await response.json(); } catch { throw new Error("402 response body was not valid JSON"); }
    console.log(JSON.stringify({ status: 402, payment_required: true, x402_version: body.x402Version ?? null, accepts: Array.isArray(body.accepts) ? body.accepts : [], error: body.error ?? null }));
    return;
  }

  // method === "pay"
  const discover = await fetch(url, { signal: AbortSignal.timeout(15000) });
  if (discover.status !== 402) {
    console.log(JSON.stringify({ paid: false, status: discover.status, message: "no payment required" }));
    return;
  }
  let body;
  try { body = await discover.json(); } catch { throw new Error("402 response body was not valid JSON"); }
  const accepts = Array.isArray(body.accepts) ? body.accepts : [];
  const requirement = selectRequirement(accepts, { scheme: "exact" }) ?? accepts[0];
  if (!requirement) throw new Error("no payment requirement offered");
  const asset = requirement.asset ?? "HBAR";
  if (asset !== "HBAR") throw new Error(`unsupported asset ${asset} (this build settles HBAR only)`);
  const amountTinybars = String(requirement.maxAmountRequired ?? requirement.amount ?? "");
  if (!/^[1-9][0-9]*$/.test(amountTinybars)) throw new Error("requirement is missing a valid amount");
  if (!/^0\.0\.[1-9][0-9]{0,9}$/.test(String(requirement.payTo ?? ""))) throw new Error("requirement is missing a valid payTo account");
  const cap = BigInt(process.env.X402_MAX_TINYBARS ?? "100000000"); // governance cap; default 1 HBAR
  if (BigInt(amountTinybars) > cap) throw new Error(`governance: amount ${amountTinybars} tinybars exceeds cap ${cap} tinybars`);
  const network = String(requirement.network ?? "hedera-testnet").includes("mainnet") ? "mainnet" : "testnet";
  const { operatorId, operatorKey } = loadOperator();
  // Lazy import so `ready`/`inspect` never require the Hedera SDK to be resolvable.
  const { settleHbarTransfer } = await import("@balamos/hedera-readonly/src/settle.mjs");
  const settlement = await settleHbarTransfer({ operatorId, operatorKey, payTo: requirement.payTo, amountTinybars, network, memo: "x402" });
  if (!settlement.ok) throw new Error(`settlement failed: ${settlement.status}`);
  const header = encodePaymentHeader({
    x402Version: body.x402Version ?? 1,
    scheme: "exact",
    network: requirement.network,
    txId: settlement.transactionId,
    payer: operatorId,
    payTo: requirement.payTo,
    amount: amountTinybars,
    asset,
  });
  const paid = await fetch(url, { headers: { "x-payment": header }, signal: AbortSignal.timeout(20000) });
  let resource = null;
  try { resource = await paid.json(); } catch { /* non-JSON body */ }
  console.log(JSON.stringify({
    paid: paid.status === 200,
    status: paid.status,
    settlement: { transactionId: settlement.transactionId, hashscanUrl: settlement.hashscanUrl, amountTinybars, payTo: requirement.payTo, network },
    resource,
  }));
}

if (import.meta.url === `file://${process.argv[1]}`) main().then(() => process.exit(0)).catch((error) => fail(error instanceof Error ? error.message.slice(0, 500) : String(error).slice(0, 500)));
