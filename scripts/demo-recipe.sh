#!/usr/bin/env bash
set -euo pipefail

# demo-recipe.sh
#
# BalamOS "recipe": combine TWO capabilities into ONE agent decision that
# neither tool makes alone.
#
#   * balamos-x402 client  -> discover the price of a paid data feed (x402)
#   * balamos-graph reader -> fetch on-chain context (The Graph)
#
# The composed question: "Given the cost of a paid data feed (x402) and the
# on-chain context (The Graph), should the agent buy it?"
#
# This is the Bazantic "combine 2+ APIs" track. The x402 step uses the local
# demo service and always runs. The Graph step runs live only if GRAPH_API_KEY
# is set; otherwise it is skipped without failing.
#
# Safe to run repeatedly. Requires no secrets (the Graph step is optional).

cd "$(dirname "$0")/.."

PORT="${PORT:-4021}"
X402_ASSET="${X402_ASSET:-HBAR}"
X402_AMOUNT="${X402_AMOUNT:-10000000}"
X402_PAY_TO="${X402_PAY_TO:-0.0.4567}"
X402_NETWORK="${X402_NETWORK:-hedera-mainnet}"

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

echo "Starting x402 demo service..."
PORT="$PORT" \
X402_ASSET="$X402_ASSET" \
X402_AMOUNT="$X402_AMOUNT" \
X402_PAY_TO="$X402_PAY_TO" \
X402_NETWORK="$X402_NETWORK" \
node packages/x402-demo-service/src/server.mjs &
SERVER_PID=$!

SERVICE_UP=0
for _ in $(seq 1 20); do
  if node "$X402_CLI" inspect "${BASE_URL}/" >/dev/null 2>&1; then
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
echo "STEP 1 — x402: discovering the price of the paid feed..."
X402_JSON="$(node "$X402_CLI" inspect "${BASE_URL}/")"

X402_FIELDS="$(printf '%s' "$X402_JSON" | python3 -c '
import json, sys
data = json.load(sys.stdin)
accepts = data.get("accepts") or []
req = accepts[0] if accepts else {}
for key in ("maxAmountRequired", "asset", "payTo", "network"):
    print(req.get(key, "unknown"))
' 2>/dev/null || true)"

if [ -z "$X402_FIELDS" ]; then
  echo "ERROR: could not parse x402 payment requirements" >&2
  exit 1
fi

AMOUNT="$(printf '%s\n' "$X402_FIELDS" | sed -n '1p')"
ASSET="$(printf '%s\n' "$X402_FIELDS" | sed -n '2p')"
PAY_TO="$(printf '%s\n' "$X402_FIELDS" | sed -n '3p')"
NETWORK="$(printf '%s\n' "$X402_FIELDS" | sed -n '4p')"

echo "STEP 1 — x402: the paid feed costs ${AMOUNT} ${ASSET} (payTo ${PAY_TO}, network ${NETWORK})."

echo
echo "STEP 2 — The Graph: fetching on-chain context..."
if [ -n "$GRAPH_API_KEY" ]; then
  if GRAPH_READY="$(node "$GRAPH_CLI" ready 2>/dev/null)"; then
    READY_FLAG="$(printf '%s' "$GRAPH_READY" | python3 -c 'import json,sys; print("ready" if json.load(sys.stdin).get("ready") else "not ready")' 2>/dev/null || echo "unknown")"
    echo "  Graph helper: ${READY_FLAG}."
  else
    echo "  Graph helper: not ready."
  fi
  if [ -n "$GRAPH_SUBGRAPH" ] && [ -n "$GRAPH_QUERY" ]; then
    echo "  Sample query (best-effort, live):"
    if GRAPH_RESULT="$(node "$GRAPH_CLI" query "$GRAPH_SUBGRAPH" "$GRAPH_QUERY" 2>/dev/null)"; then
      echo "    received on-chain context: $(printf '%s' "$GRAPH_RESULT" | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin))[:160])' 2>/dev/null || echo "ok")"
    else
      echo "    sample query did not return data — continuing without it."
    fi
  fi
else
  echo "  GRAPH_API_KEY not set — skipping live Graph read; in production the agent fetches portfolio/price context here."
fi

echo
echo "STEP 3 — Recipe decision:"
echo "  The feed costs ${AMOUNT} ${ASSET}. With on-chain context, the agent decides buy/skip against its governance budget — a call neither the price API nor the data API makes alone."

echo
echo "Settlement is the next layer: a governance-gated wallet signs and settles the x402 payment only after the buy decision."
