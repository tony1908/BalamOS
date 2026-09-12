#!/usr/bin/env bash
set -euo pipefail

# demo-x402.sh
#
# End-to-end demonstration of the BalamOS x402 flow on Hedera.
#
# STEP 1 (always, no secrets): the agent discovers an x402-gated endpoint and
#   reads its real payment terms (amount, asset, network, payTo) with no API key.
#
# STEP 2 (real on-chain settlement) runs when HEDERA_KEY_FILE points at a funded
#   testnet operator wallet JSON ({ "accountId": "0.0.x", "privateKeyRaw": "..." }):
#   the agent settles the HBAR payment on Hedera testnet, then re-requests with an
#   X-PAYMENT proof. The demo service verifies the transfer against the public
#   mirror node (no key) before returning the paid resource. If HEDERA_KEY_FILE is
#   unset, STEP 2 is skipped and the command to enable it is printed.
#
# Safe to run repeatedly.

cd "$(dirname "$0")/.."

PORT="${PORT:-4021}"
X402_NETWORK="${X402_NETWORK:-hedera-testnet}"
X402_PAY_TO="${X402_PAY_TO:-0.0.10512599}"   # the demo service's testnet wallet
X402_ASSET="${X402_ASSET:-HBAR}"
X402_AMOUNT="${X402_AMOUNT:-1000000}"          # 0.01 HBAR in tinybars

CLI="packages/balamos-x402/src/cli.mjs"
BASE_URL="http://localhost:${PORT}"
SERVER_PID=""
PROBE_FILE="$(mktemp -t balamos-x402-probe.XXXXXX)"

cleanup() {
  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -f "$PROBE_FILE"
}
trap cleanup EXIT INT TERM

pretty() {
  if command -v python3 >/dev/null 2>&1; then python3 -m json.tool; else cat; fi
}

echo "Starting x402 demo service (${X402_NETWORK}, pay-to ${X402_PAY_TO}, ${X402_AMOUNT} tinybars)..."
PORT="$PORT" \
X402_PAY_TO="$X402_PAY_TO" \
X402_ASSET="$X402_ASSET" \
X402_AMOUNT="$X402_AMOUNT" \
X402_NETWORK="$X402_NETWORK" \
node packages/x402-demo-service/src/server.mjs &
SERVER_PID=$!

echo "Waiting for service on ${BASE_URL} ..."
SERVICE_UP=0
for _ in $(seq 1 20); do
  if node "$CLI" inspect "${BASE_URL}/feed" >"$PROBE_FILE" 2>/dev/null; then
    SERVICE_UP=1
    break
  fi
  sleep 0.3
done
if [ "$SERVICE_UP" -ne 1 ]; then
  echo "ERROR: x402 demo service did not become ready at ${BASE_URL}" >&2
  exit 1
fi

echo
echo "STEP 1 — Agent inspects the paywalled endpoint (no API key):"
node "$CLI" inspect "${BASE_URL}/feed" | pretty

echo
if [ -n "${HEDERA_KEY_FILE:-}" ]; then
  echo "STEP 2 — Agent settles the payment on ${X402_NETWORK} and retrieves the resource (REAL on-chain):"
  HEDERA_KEY_FILE="$HEDERA_KEY_FILE" node "$CLI" pay "${BASE_URL}/feed" | pretty
  echo
  echo "The service verified the HBAR transfer to ${X402_PAY_TO} against the public mirror node — no API key."
else
  echo "STEP 2 — skipped (no wallet). To run real on-chain settlement, point HEDERA_KEY_FILE at a"
  echo "funded Hedera testnet operator wallet and re-run:"
  echo "  HEDERA_KEY_FILE=/path/to/wallet.json bash scripts/demo-x402.sh"
fi
