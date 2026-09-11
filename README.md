# BalamOS — the Grok bot of crypto

BalamOS is a **chat-first agent OS**: each workspace is an AI agent you message, running inside its own sandboxed Linux desktop. It can **see, pay for, and act on** on-chain services on your behalf — with a **human-in-the-loop governance rail** that keeps autonomy safe.

Its differentiator: the agent can **pay its own way** — discovering and paying for services via **x402** (no API key), instead of being handed credentials.

> ETHOnline 2026 submission. Targeting Hedera (Agentic Payments + Harness), Circle/Arc (Agentic Economy), The Graph (AI Tooling), with Bazantic/Ledger/ENS as riders. See [`docs/plans/2026-09-10-crypto-agent-hackathon-plan.md`](docs/plans/2026-09-10-crypto-agent-hackathon-plan.md).

## What's built

Every capability is a **container-side CLI** the agent invokes, surfaced as an installable tile in the **Plugins hub**, and taught to the agent via a skill injected into its `AGENTS.md`.

| Capability | Package / surface | State |
|---|---|---|
| **The Graph reader** | `packages/balamos-graph` — read-only Subgraph queries ("what's in my wallet") | ✅ tested |
| **x402 client** | `packages/balamos-x402` — `inspect` a paywall + payment-header codec | ✅ tested |
| **x402 demo service** | `packages/x402-demo-service` — an x402-gated endpoint to demo against | ✅ tested |
| **Hedera** | mainnet read-only monitor + user-approved HBAR transfer intents | ✅ (existing) |
| **Circle Agent Wallets** | ARC-TESTNET-first CLI guidance skill | ✅ guidance |
| **Plugins hub** | grid → detail flow, premium UI, per-workspace install | ✅ shipped |

**Not yet:** real x402 **settlement** (signing/paying on-chain) — it's designed to run behind the governance spending cap with a funded, dust-limited agent wallet, and is the next layer. Today the paid path in the demo is an explicit **mock**.

## Quickstart

```bash
# 1. Run all crypto-CLI unit tests (21 tests)
pnpm test:packages

# 2. See the x402 discover → (mock) pay flow end to end
bash scripts/demo-x402.sh

# 2b. See the 2-API recipe (x402 cost + Graph context -> buy/skip decision)
bash scripts/demo-recipe.sh

# 3. Launch the full desktop app (needs Docker running)
scripts/dev-app.sh
```

The x402 demo needs no keys or funds — it shows the agent discovering a paywalled endpoint's real payment terms (amount, asset, network, payTo) with no API key.

## Architecture

```
React desktop (Tauri)  ──▶  orbit-daemon (Rust)  ──▶  webtop container (sandboxed Linux desktop)
   Plugins hub / chat        governance · secrets        agent + capability CLIs:
                             workspace lifecycle          balamos-graph · balamos-x402 · balamos-hedera-* · circle
```

- **`apps/desktop`** — Tauri + React UI (the chat, the Plugins hub).
- **`crates/orbit-*`** — the Rust daemon, protocol, store; mediates anything that spends and enforces governance.
- **`packages/*`** — the zero-/few-dependency capability CLIs the container agent runs.
- **`images/orbit-webtop`** — the container image the CLIs are installed into.

## Safety

Reads never sign or spend. Anything that moves funds is mediated by the daemon and gated by **governance** (spending caps, approval). Secrets are redacted and never written to prompts, logs, or the repo. Real payments will use a small, capped, dust-funded agent wallet so worst-case loss is bounded.
