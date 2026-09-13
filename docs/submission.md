# ETHOnline 2026 — Submission Guide (per sponsor)

BalamOS: the Grok-bot-of-crypto. This maps each target track to **what to look at**, **how it qualifies**, and its **honest status**. Run the demos from the [README](../README.md) quickstart.

**Verified now:** `pnpm test:packages` (31 tests) · `bash scripts/demo-x402.sh` (discovery, no secrets) · `HEDERA_KEY_FILE=… bash scripts/demo-x402.sh` (**real on-chain HBAR settlement on Hedera testnet**) · `bash scripts/demo-recipe.sh` · desktop suite 165/165.

---

## Hedera — AI & Agentic Payments ($6k)
- **Ask:** an agent discovers and pays an x402-gated service on Hedera, no API keys.
- **Look at:** `packages/balamos-x402/` (client `inspect` + `pay`), `packages/hedera-readonly/src/settle.mjs` (HBAR settlement), `packages/x402-demo-service/` (a Hedera x402 endpoint that verifies on-chain), `scripts/demo-x402.sh`.
- **Status:** ✅ **discovery + on-chain settlement live-verified on Hedera testnet.** `balamos-x402 pay <url>` inspects the endpoint (no key), gates the amount against a governance spend cap, signs and submits a real HBAR transfer from the agent wallet to `payTo`, then re-requests with an `X-PAYMENT` proof; the service **verifies the transfer against the public mirror node** (no key) before returning the resource. No mock. Example settlement: [`0.0.10512454@1789257195…`](https://hashscan.io/testnet/transaction/0.0.10512454@1789257195.006019834). Custody stays with the user (operator key funds the wallet, never enters the repo/logs/prompts). Remaining polish: surface the spend cap in the in-app Governance panel and route settlement through the daemon state machine — see `docs/plans/2026-09-10-x402-settlement-design.md`.

## Hedera — Improve the Hedera Harness ($2k)
- **Ask:** extend the Hedera harness / improve DX / add service coverage.
- **Look at:** `packages/hedera-readonly/` (uses `hedera-agent-kit@4.1.0` as the harness), the new sibling CLIs `packages/balamos-x402`, `packages/balamos-graph`, and the container wiring in `images/orbit-webtop/`.
- **How it qualifies:** BalamOS *is* an agent harness layer — it packages Hedera tools as container CLIs any coding agent (Codex/opencode/Claude) invokes, and extends the surface with x402 discovery + a documented x402-plugin integration path.
- **Status:** ✅ harness use + CLI extensions shipped; x402-plugin integration designed.

## The Graph — Best AI Tooling ($5k)
- **Ask:** AI agents querying Subgraphs via MCP; portfolio/trading agents.
- **Look at:** `packages/balamos-graph/` (CLI), `packages/balamos-graph-mcp/` (**MCP server**, `query_subgraph` tool), `scripts/demo-mcp.sh`, and the container auto-wiring (`images/orbit-webtop/opencode/opencode.json`).
- **Status:** ✅ built + unit-tested (CLI 5/5, MCP 7/7). ✅ **MCP server verified connecting to a real opencode install** (`opencode mcp list` → connected) and auto-wired into the container's opencode config, so the agent gets `query_subgraph` natively. ✅ **verified live against a real Subgraph** (returned the current Ethereum block) through both the CLI and the MCP `query_subgraph` tool, using a Subgraph Studio API key supplied as `GRAPH_API_KEY`.

## Bazantic — combine 2+ APIs ($3k)
- **Ask:** one workflow combining 2+ APIs that neither solves alone.
- **Look at:** `scripts/demo-recipe.sh` — composes x402 cost discovery + The Graph context into a buy/skip decision.
- **Status:** ✅ runs end-to-end (Graph step live with a key).

## Arc (Circle) — Agentic Economy ($6k incl. mainnet bonus)
- **Ask:** AI agent holding a USDC wallet, autonomous payments, risk management.
- **Look at:** `apps/desktop/src/skills/circleAgentWallet.ts` (guidance skill), `docs/circle-agent-wallets.md`, and the governance system (the "manage risk" story).
- **Status:** ✅ guidance skill shipped. ⛔ **live USDC agent wallet + Arc deploy not built** — needs Circle creds + the funded-wallet/governance-cap work from the settlement design.

## Riders (designed, not built)
- **Ledger** — scoped secrets + device approval + x402 (maps to BalamOS secrets + governance).
- **ENS** — `*.balamos.eth` agent names as payable identity.
- **World** — human-backed agent identity via AgentBook / Selfie-Check governance gate.

---

## Blocked on external inputs
Circle creds + Arc deploy (Circle bonus) · Docker + memory (image rebuild, app run, **video demo**). *(Resolved: funded Hedera testnet wallet — settlement now live; `GRAPH_API_KEY` — Graph track live-verified.)*
