import { readFile } from "node:fs/promises";
import { access } from "node:fs/promises";

async function capability() {
  if (process.env.BALAMOS_HEDERA_AGENT_URL && process.env.BALAMOS_HEDERA_AGENT_TOKEN) return { url: process.env.BALAMOS_HEDERA_AGENT_URL, token: process.env.BALAMOS_HEDERA_AGENT_TOKEN };
  const candidates = ["/workspace/.balamos/hedera-agent.json", "/config/.balamos/hedera-agent.json"];
  let value;
  for (const path of candidates) { try { await access(path); value = JSON.parse(await readFile(path, "utf8")); break; } catch {} }
  if (!value) throw new Error("agent gateway capability is not configured");
  if (!value.url || !value.token) throw new Error("agent gateway capability is not configured");
  return value;
}
const call = async (path, options = {}) => { const { url, token } = await capability(); const response = await fetch(`${url}${path}`, { ...options, signal: AbortSignal.timeout(10000), headers: { authorization: `Bearer ${token}`, ...(options.headers ?? {}) } }); if (!response.ok) throw new Error(`agent gateway returned HTTP ${response.status}`); return response.json(); };
export function hbarToTinybars(value) { if (typeof value !== "string" || !/^\d+(\.\d{1,8})?$/.test(value)) throw new Error("amount_hbar must be a non-negative decimal with at most 8 fractional digits"); const [whole, fraction = ""] = value.split("."); const result = BigInt(whole) * 100000000n + BigInt(fraction.padEnd(8, "0")); if (result <= 0n || result > 9223372036854775807n) throw new Error("amount must be greater than zero and fit int64"); return result.toString(); }
export async function run(args) { const [command, ...rest] = args; if (command === "propose") { const [recipient, amount, idempotencyKey] = rest; if (!recipient || !idempotencyKey) throw new Error("RECIPIENT and ID are required"); return call("/propose", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ recipient_account_id: recipient, amount_tinybars: hbarToTinybars(amount), idempotency_key: idempotencyKey }) }); } if (command === "list") return call("/list"); if (command === "status") { if (!rest[0]) throw new Error("INTENT_ID is required"); return call(`/status?intent_id=${encodeURIComponent(rest[0])}`); } throw new Error("command must be propose, list, or status"); }
if (import.meta.url === `file://${process.argv[1]}`) run(process.argv.slice(2)).then((result) => console.log(JSON.stringify(result))).catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
