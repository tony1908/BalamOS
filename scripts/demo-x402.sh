#!/usr/bin/env bash
set -euo pipefail

# demo-x402.sh
#
# End-to-end demonstration of the BalamOS x402 flow.
#
# This demo starts the local x402-gated demo service and shows the balamos-x402
# client discovering its payment terms (no API key required), followed by the
# mock paid retrieval of the resource. Real on-chain settlement is intentionally
# not performed here; it is added later behind governance (funded agent wallet
# plus facilitator verification).
#
# Safe to run repeatedly. No secrets required.

cd "$(dirname "$0")/.."

PORT="${PORT:-4021}"
X402_PAY_TO="${X402_PAY_TO:-0.0.4567}"
X402_ASSET="${X402_ASSET:-HBAR}"
X402_AMOUNT="${X402_AMOUNT:-10000000}"
X402_NETWORK="${X402_NETWORK:-hedera-mainnet}"

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

# Pretty-print stdin as JSON when python3 is available, otherwise pass through.
pretty() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -m json.tool
  else
    cat
  fi
}

echo "Starting x402 demo service..."
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
  if node "$CLI" inspect "${BASE_URL}/" >"$PROBE_FILE" 2>/dev/null; then
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
node "$CLI" inspect "${BASE_URL}/" | pretty

echo
echo "STEP 2 — Agent retrieves the resource after paying (MOCK settlement):"
echo "NOTE: the X-PAYMENT value below is a placeholder token; settlement is mocked, not verified on-chain."
curl -s -H "X-PAYMENT: ZGVtbw==" "${BASE_URL}/data" | pretty

echo
echo "Discovery is real; on-chain settlement is the next layer (governance-gated, funded agent wallet)."
