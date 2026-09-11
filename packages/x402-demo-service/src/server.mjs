import { createServer } from "node:http";

const PORT = process.env.PORT || 4021;
const X402_NETWORK = process.env.X402_NETWORK || "hedera-mainnet";
const X402_PAY_TO = process.env.X402_PAY_TO || "0.0.2";
const X402_ASSET = process.env.X402_ASSET || "HBAR";
const X402_AMOUNT = process.env.X402_AMOUNT || "10000000";

export function buildRequirements({ network, payTo, asset, amount, resource }) {
  return {
    x402Version: 1,
    accepts: [
      {
        scheme: "exact",
        network,
        maxAmountRequired: amount,
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

export function route({ method, hasPayment, resource, config }) {
  if (method !== "GET") {
    return { status: 405, body: { error: "method not allowed" } };
  }

  if (!hasPayment) {
    return { status: 402, body: buildRequirements({ ...config, resource }) };
  }

  return {
    status: 200,
    body: {
      paid: true,
      data: { feed: "balamos-demo", price_usd: 1.23, ts: null },
      note: "MOCK SETTLEMENT: the X-PAYMENT header is accepted without on-chain verification; real facilitator verification is added with the settlement layer.",
    },
  };
}

const config = {
  network: X402_NETWORK,
  payTo: X402_PAY_TO,
  asset: X402_ASSET,
  amount: X402_AMOUNT,
};

const server = createServer((req, res) => {
  const hasPayment = typeof req.headers["x-payment"] === "string" && req.headers["x-payment"].length > 0;
  const resource = `http://localhost:${PORT}${req.url}`;
  const result = route({ method: req.method, hasPayment, resource, config });

  console.error(`${req.method} ${req.url} -> ${result.status}`);
  res.writeHead(result.status, { "content-type": "application/json" });
  res.end(JSON.stringify(result.body));
});

if (import.meta.url === `file://${process.argv[1]}`) {
  server.listen(PORT, () => {
    console.log(`x402 demo service listening on http://localhost:${PORT}`);
  });
}
