const allowed = new Set(["ready", "inspect"]);
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

async function main() {
  const [method, url] = process.argv.slice(2);
  if (!allowed.has(method)) throw new Error("method is not allowed");
  if (method === "ready") {
    console.log(JSON.stringify({ ready: true, helper_version: "0.1.0", protocol: "x402", message: "x402 protocol helper ready (inspect only; settlement not enabled)" }));
    return;
  }
  const response = await fetch(url, { signal: AbortSignal.timeout(15000) });
  if (response.status !== 402) {
    console.log(JSON.stringify({ status: response.status, payment_required: false, message: "no payment required" }));
    return;
  }
  let body;
  try { body = await response.json(); } catch { throw new Error("402 response body was not valid JSON"); }
  console.log(JSON.stringify({ status: 402, payment_required: true, x402_version: body.x402Version ?? null, accepts: Array.isArray(body.accepts) ? body.accepts : [], error: body.error ?? null }));
}

if (import.meta.url === `file://${process.argv[1]}`) main().then(() => process.exit(0)).catch((error) => fail(error instanceof Error ? error.message.slice(0, 500) : String(error).slice(0, 500)));
