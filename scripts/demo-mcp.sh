#!/usr/bin/env bash
set -euo pipefail

# demo-mcp.sh
#
# Demonstrates the BalamOS Graph MCP server (packages/balamos-graph-mcp).
#
# This script drives the server over stdio using newline-delimited JSON-RPC 2.0
# and prints each response. The MCP handshake (initialize) and tools/list need
# no credentials. The tools/call for query_subgraph runs live only when
# GRAPH_API_KEY is set (used when resolving a bare subgraph id); here we pass a
# full https URL as the subgraph, so resolveEndpoint passes it through with no
# API key. The request fails to reach a real gateway and returns a graceful
# error result — that is expected and shows the tool-call path end to end.
#
# Safe to run repeatedly. No secrets required.

cd "$(dirname "$0")/.."

SERVER="packages/balamos-graph-mcp/src/server.mjs"

# Pretty-print stdin as JSON when python3 is available, otherwise pass through.
pretty() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -m json.tool 2>/dev/null || cat
  else
    cat
  fi
}

# Pull the nth (1-indexed) line from stdin, or empty if missing.
nth_line() {
  sed -n "${1}p"
}

# Print a JSON-RPC response line, pretty-printed when possible. Warns but never
# crashes if the line is missing.
print_response() {
  local line
  line="$(nth_line "$1")"
  if [ -z "$line" ]; then
    echo "  (no response line for request $1)" >&2
    return 0
  fi
  printf '%s\n' "$line" | pretty
}

REQUESTS="$(
  cat <<'JSON'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_subgraph","arguments":{"subgraph":"https://example.com/nonexistent","query":"{ _meta { block { number } } }"}}}
JSON
)"

# Drive the server: feed the three requests, capture all stdout.
RESPONSES="$(printf '%s\n' "$REQUESTS" | node "$SERVER" 2>/dev/null || true)"

echo "Driving balamos-graph-mcp over stdio (JSON-RPC 2.0)..."
echo

echo "STEP 1 — initialize:"
printf '%s\n' "$RESPONSES" | print_response 1

echo
echo "STEP 2 — tools/list:"
printf '%s\n' "$RESPONSES" | print_response 2

echo
echo "STEP 3 — tools/call (query_subgraph):"
printf '%s\n' "$RESPONSES" | print_response 3
echo "NOTE: with a real GRAPH_API_KEY and a real subgraph id this returns live"
echo "      on-chain data. Here it demonstrates the tool-call path with a"
echo "      graceful error (the example.com URL is not a reachable gateway)."

echo
echo "Handshake and tool enumeration required no secrets; the tool call is the same path a funded agent would use."
