# ETHOnline 2026 — Submission Guide (per sponsor)

BalamOS: the Grok-bot-of-crypto. This maps each target track to **what to look at**, **how it qualifies**, and its **honest status**. Run the demos from the [README](../README.md) quickstart.

**Verified now:** `pnpm test:packages` (21 tests) · `bash scripts/demo-x402.sh` · `bash scripts/demo-recipe.sh` · desktop suite 166/166.

---

## Hedera — AI & Agentic Payments ($6k)
- **Ask:** an agent discovers and pays an x402-gated service on Hedera, no API keys.
- **Look at:** `packages/balamos-x402/` (client), `packages/x402-demo-service/` (a Hedera-scheme x402 endpoint), `scripts/demo-x402.sh`.
- **Status:** ✅ **discovery live-verified** — the agent inspects the endpoint and reads real payment terms (amount, HBAR/HTS asset, network, payTo) with no key. ⛔ **on-chain settlement designed, not built** — see `docs/plans/2026-09-10-x402-settlement-design.md`; needs a funded Hedera wallet. The demo's paid path is an explicit mock.

## Hedera — Improve the Hedera Harness ($2k)
- **Ask:** extend the Hedera harness / improve DX / add service coverage.
- **Look at:** `packages/hedera-readonly/` (uses `hedera-agent-kit@4.1.0` as the harness), the new sibling CLIs `packages/balamos-x402`, `packages/balamos-graph`, and the container wiring in `images/orbit-webtop/`.
- **How it qualifies:** BalamOS *is* an agent harness layer — it packages Hedera tools as container CLIs any coding agent (Codex/opencode/Claude) invokes, and extends the surface with x402 discovery + a documented x402-plugin integration path.
- **Status:** ✅ harness use + CLI extensions shipped; x402-plugin integration designed.

## The Graph — Best AI Tooling ($5k)
- **Ask:** AI agents querying Subgraphs via MCP; portfolio/trading agents.
- **Look at:** `packages/balamos-graph/` (CLI), `packages/balamos-graph-mcp/` (**MCP server**, `query_subgraph` tool).
- **Status:** ✅ built + unit-tested (CLI 5/5, MCP 7/7, live stdio handshake verified). ⚠️ **not yet run against a live Subgraph** — set `GRAPH_API_KEY` to verify real-data queries.

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
Funded Hedera wallet (settlement) · `GRAPH_API_KEY` (live Graph proof) · Circle creds + Arc deploy (Circle bonus) · Docker + memory (image rebuild, app run, **video demo**).
