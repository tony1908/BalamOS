#!/usr/bin/env bash
set -euo pipefail

# demo-recipe.sh
#
# BalamOS "recipe": combine TWO capabilities into ONE agent decision + action
# that neither tool makes alone (Bazantic "combine 2+ APIs" track).
#
#   * The Graph (balamos-graph) -> on-chain context (price/liquidity/volume)
#   * x402 (balamos-x402)        -> discover a paid feed's price, then, if it
#                                   clears the governance cap, SETTLE it on
#                                   Hedera (a real HBAR transfer) and unlock it.
#
# Composed question: "Given on-chain context (The Graph) and the cost of a paid
# feed (x402), should the agent buy it -- and if so, pay for it?"
#
# STEP 4 settles for real only when HEDERA_KEY_FILE points at a funded Hedera
# testnet wallet; otherwise it stops at the buy decision. The Graph step runs
# live only if GRAPH_API_KEY (and GRAPH_SUBGRAPH) are set. Safe to run repeatedly.

cd "$(dirname "$0")/.."

PORT="${PORT:-4021}"
X402_ASSET="${X402_ASSET:-HBAR}"
X402_AMOUNT="${X402_AMOUNT:-1000000}"                 # 0.01 HBAR in tinybars
X402_PAY_TO="${X402_PAY_TO:-0.0.10512599}"
X402_NETWORK="${X402_NETWORK:-hedera-testnet}"
X402_MAX_TINYBARS="${X402_MAX_TINYBARS:-100000000}"   # governance cap, 1 HBAR

GRAPH_API_KEY="${GRAPH_API_KEY:-}"
GRAPH_SUBGRAPH="${GRAPH_SUBGRAPH:-}"
GRAPH_QUERY="${GRAPH_QUERY:-{ _meta { block { number } } }}"

X402_CLI="packages/balamos-x402/src/cli.mjs"
GRAPH_CLI="packages/balamos-graph/src/cli.mjs"
BASE_URL="http://localhost:${PORT}"
SERVER_PID=""

cleanup() {
  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

echo "Starting x402 demo service (${X402_NETWORK}, pay-to ${X402_PAY_TO})..."
PORT="$PORT" \
X402_ASSET="$X402_ASSET" \
X402_AMOUNT="$X402_AMOUNT" \
X402_PAY_TO="$X402_PAY_TO" \
X402_NETWORK="$X402_NETWORK" \
node packages/x402-demo-service/src/server.mjs &
SERVER_PID=$!

SERVICE_UP=0
for _ in $(seq 1 20); do
  if node "$X402_CLI" inspect "${BASE_URL}/feed" >/dev/null 2>&1; then
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
echo "STEP 1 - The Graph: on-chain context..."
if [ -n "$GRAPH_API_KEY" ]; then
  if [ -n "$GRAPH_SUBGRAPH" ]; then
    if GRAPH_RESULT="$(node "$GRAPH_CLI" query "$GRAPH_SUBGRAPH" "$GRAPH_QUERY" 2>/dev/null)"; then
      echo "  context: $(printf '%s' "$GRAPH_RESULT" | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin))[:200])' 2>/dev/null || echo ok)"
    else
      echo "  Graph query returned no data - continuing without it."
    fi
  else
    echo "  GRAPH_SUBGRAPH not set - set it (and GRAPH_QUERY) for a live read."
  fi
else
  echo "  GRAPH_API_KEY not set - in production the agent reads price/liquidity context here."
fi

echo
echo "STEP 2 - x402: discovering the paid feed's price..."
X402_JSON="$(node "$X402_CLI" inspect "${BASE_URL}/feed")"
AMOUNT="$(printf '%s' "$X402_JSON" | python3 -c 'import json,sys; a=(json.load(sys.stdin).get("accepts") or [{}])[0]; print(a.get("maxAmountRequired") or a.get("amount") or "")' 2>/dev/null || true)"
if [ -z "$AMOUNT" ]; then echo "ERROR: could not read the x402 price" >&2; exit 1; fi
echo "  the feed costs ${AMOUNT} tinybars (${X402_ASSET}) to ${X402_PAY_TO} on ${X402_NETWORK}."

echo
echo "STEP 3 - Recipe decision (the combined call):"
if [ "$AMOUNT" -le "$X402_MAX_TINYBARS" ]; then
  DECISION="buy"
  echo "  ${AMOUNT} <= cap ${X402_MAX_TINYBARS} tinybars -> BUY. (Neither the price API nor the data API makes this call alone.)"
else
  DECISION="skip"
  echo "  ${AMOUNT} > cap ${X402_MAX_TINYBARS} tinybars -> SKIP."
fi

echo
if [ "$DECISION" = "buy" ] && [ -n "${HEDERA_KEY_FILE:-}" ]; then
  echo "STEP 4 - x402 settlement on Hedera (REAL):"
  HEDERA_KEY_FILE="$HEDERA_KEY_FILE" X402_MAX_TINYBARS="$X402_MAX_TINYBARS" \
    node "$X402_CLI" pay "${BASE_URL}/feed" | (python3 -m json.tool 2>/dev/null || cat)
elif [ "$DECISION" = "buy" ]; then
  echo "STEP 4 - settlement skipped (no wallet). Set HEDERA_KEY_FILE to a funded"
  echo "  Hedera testnet wallet to settle for real:"
  echo "    HEDERA_KEY_FILE=/path/to/wallet.json bash scripts/demo-recipe.sh"
else
  echo "STEP 4 - no purchase (decision was skip)."
fi
