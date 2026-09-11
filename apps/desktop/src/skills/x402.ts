export const X402_SKILL_NAME = "x402 Payments";
export const X402_SKILL_INSTRUCTION = `Use the \`balamos-x402\` CLI to inspect pay-per-use (HTTP 402 / x402) services before any payment is considered.

- \`balamos-x402 ready\` reports the helper status.
- \`balamos-x402 inspect <url>\` fetches the endpoint; if it requires payment it returns the accepted payment terms — the required amount, asset, network, and payTo destination — without paying.
- This helper is INSPECT-ONLY today. It does NOT sign transactions or move any funds. Never claim a payment was made or a paid resource was retrieved.
- When a paid resource is needed, report the terms (amount, asset, network) to the user and stop; autonomous settlement is added later behind explicit governance approval and a funded, capped agent wallet.`;
