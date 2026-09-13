import { Client, AccountId } from "@hiero-ledger/sdk";
import { HederaAgentAPI } from "@hashgraph/hedera-agent-kit";
import { getHbarBalanceQuery } from "@hashgraph/hedera-agent-kit/plugins";
import { buildTransaction, validateSignedTransaction, submitTransaction, reconcileTransaction } from "./transactions.mjs";

const MIRROR = "https://mainnet.mirrornode.hedera.com/api/v1";
const allowed = new Set(["ready", "snapshot", "build", "validate", "submit", "reconcile"]);
const fail = (message) => { process.stderr.write(`${message}\n`); process.exitCode = 1; };
function account(value) { if (!/^0\.0\.[1-9][0-9]{0,9}$/.test(value)) throw new Error("account_id must be a numeric mainnet account ID"); return AccountId.fromString(value).toString(); }
export function hbar(value) { const negative = value < 0n; const digits = (negative ? -value : value).toString().padStart(9, "0"); const whole = digits.slice(0, -8) || "0"; const fraction = digits.slice(-8).replace(/0+$/, ""); return `${negative ? "-" : ""}${whole}${fraction ? `.${fraction}` : ""}`; }
export function tinybars(value) { const [whole, fraction = ""] = String(value).split("."); return BigInt(whole) * 100000000n + BigInt(fraction.padEnd(8, "0").slice(0, 8)); }
async function json(path) { const response = await fetch(`${MIRROR}${path}`, { signal: AbortSignal.timeout(8000) }); if (!response.ok) throw new Error(`mirror node returned HTTP ${response.status}`); return response.json(); }
async function getBalance(id) { const client = Client.forMainnet(); try { const api = new HederaAgentAPI(client, {}, [getHbarBalanceQuery({})]); const result = JSON.parse(await api.run("get_hbar_balance_query_tool", { accountId: id })); const tiny = tinybars(result.raw.hbarBalance); return { tinybars: tiny.toString(), hbar: hbar(tiny) }; } finally { client.close(); } }
async function main() {
  const args = process.argv.slice(2); const method = args[0] === "transactions" ? args[1] : args[0]; const rawId = args[0] === "transactions" ? args[2] : args[1]; const rawLimit = args[0] === "transactions" ? args[3] : args[2]; if (!allowed.has(method)) throw new Error("method is not allowed");
  if (["build", "validate", "submit", "reconcile"].includes(method)) {
    const input = JSON.parse(await new Promise((resolve, reject) => { let data = ""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => { data += chunk; if (data.length > 1_000_000) reject(new Error("stdin payload too large")); }); process.stdin.on("end", () => resolve(data.trim())); process.stdin.on("error", reject); }));
    const payload = input;
    if (method === "build") { const result = buildTransaction(payload); delete result.transaction; console.log(JSON.stringify(result)); return; }
    if (method === "validate") { console.log(JSON.stringify(validateSignedTransaction(payload))); return; }
    if (method === "submit") { const result = await submitTransaction(payload); console.log(JSON.stringify(result)); return; }
    console.log(JSON.stringify(await reconcileTransaction(payload.transaction_id))); return;
  }
  if (method === "ready") { new HederaAgentAPI(Client.forMainnet(), {}, [getHbarBalanceQuery({})]); console.log(JSON.stringify({ ready: true, network: "mainnet", helper_version: "0.1.0", message: "Agent Kit query runtime ready" })); return; }
  const id = account(rawId); const limit = Math.min(Math.max(Number(rawLimit) || 10, 1), 25); const [balance, txns] = await Promise.all([getBalance(id), json(`/transactions?account.id=${encodeURIComponent(id)}&limit=${limit}&order=desc`)]);
  console.log(JSON.stringify({ account_id: id, network: "mainnet", balance_tinybars: balance.tinybars, balance_hbar: balance.hbar, transactions: (txns.transactions ?? []).slice(0, limit).map((tx) => ({ id: tx.transaction_id ?? "unknown", consensus_timestamp: tx.consensus_timestamp ?? null, name: tx.name ?? "unknown", result: tx.result ?? "unknown", charged_hbar_tinybars: String(tx.charged_tx_fee ?? 0) })), fetched_at_unix_ms: Date.now() }));
}
if (import.meta.url === `file://${process.argv[1]}`) main().then(() => process.exit(0)).catch((error) => fail(error instanceof Error ? error.message.slice(0, 500) : String(error).slice(0, 500)));
