import { createServer } from "node:http";
import { verifyHederaPayment } from "./verify.mjs";

const PORT = process.env.PORT || 4021;
const X402_NETWORK = process.env.X402_NETWORK || "hedera-testnet";
const X402_PAY_TO = process.env.X402_PAY_TO || "0.0.10512599";
const X402_ASSET = process.env.X402_ASSET || "HBAR";
const X402_AMOUNT = process.env.X402_AMOUNT || "1000000"; // 0.01 HBAR in tinybars

export function buildRequirements({ network, payTo, asset, amount, resource }) {
  return {
    x402Version: 1,
    accepts: [
      {
        scheme: "exact",
        network,
        maxAmountRequired: amount,
        amount,
        resource,
        description: "BalamOS demo paid data feed",
        mimeType: "application/json",
        payTo,
        asset,
        maxTimeoutSeconds: 60,
        extra: {},
      },
    ],
    error: "payment required",
  };
}

function decodePaymentHeader(b64) {
  try { return JSON.parse(Buffer.from(b64, "base64").toString("utf8")); } catch { return null; }
}

function hashscan(network, txId) {
  return `https://hashscan.io/${String(network).includes("mainnet") ? "mainnet" : "testnet"}/transaction/${txId}`;
}

// verify is injectable for tests; defaults to real mirror-node verification.
export async function route({ method, paymentHeader, resource, config, verify = verifyHederaPayment }) {
  if (method !== "GET") return { status: 405, body: { error: "method not allowed" } };
  if (!paymentHeader) return { status: 402, body: buildRequirements({ ...config, resource }) };
  const payment = decodePaymentHeader(paymentHeader);
  if (!payment || typeof payment.txId !== "string") {
    return { status: 402, body: { ...buildRequirements({ ...config, resource }), error: "invalid X-PAYMENT header" } };
  }
  const result = await verify({ txId: payment.txId, payTo: config.payTo, minTinybars: config.amount, network: config.network });
  if (!result.ok) {
    return { status: 402, body: { ...buildRequirements({ ...config, resource }), error: `payment not verified: ${result.reason}` } };
  }
  return {
    status: 200,
    body: {
      paid: true,
      data: { feed: "balamos-demo", price_usd: 1.23, ts: Date.now() },
      settlement: {
        txId: payment.txId,
        network: config.network,
        payTo: config.payTo,
        creditedTinybars: result.creditedTinybars ?? null,
        hashscanUrl: hashscan(config.network, payment.txId),
      },
    },
  };
}

const config = { network: X402_NETWORK, payTo: X402_PAY_TO, asset: X402_ASSET, amount: X402_AMOUNT };

const server = createServer((req, res) => {
  const paymentHeader = typeof req.headers["x-payment"] === "string" ? req.headers["x-payment"] : "";
  const resource = `http://localhost:${PORT}${req.url}`;
  Promise.resolve(route({ method: req.method, paymentHeader, resource, config }))
    .then((result) => {
      console.error(`${req.method} ${req.url} -> ${result.status}`);
      res.writeHead(result.status, { "content-type": "application/json" });
      res.end(JSON.stringify(result.body));
    })
    .catch((err) => {
      res.writeHead(500, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: String(err?.message ?? err).slice(0, 300) }));
    });
});

if (import.meta.url === `file://${process.argv[1]}`) {
  server.listen(PORT, () => {
    console.log(`x402 demo service listening on http://localhost:${PORT}`);
  });
}
