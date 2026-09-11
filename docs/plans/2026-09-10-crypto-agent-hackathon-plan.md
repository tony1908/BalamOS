# BalamOS — "The Grok Bot of Crypto" · ETHOnline 2026 Build Plan

**Date:** 2026-09-10 · **Event:** ETHOnline 2026 (Sep 4–16) · **Deadline:** ~Sep 16 (Arc mainnet bonus by Sep 30) · **Time left:** ~6 days

---

## 1. The vision

**BalamOS is the Grok bot of crypto**: a chat-first AI agent — one per workspace, like a Grok companion — that lives in its own sandboxed Linux desktop and can **see, pay for, and act on** crypto services on your behalf, with a **human-in-the-loop governance rail** that keeps autonomy safe.

Grok answers questions and uses tools in real time. BalamOS does the same, but its "tools" are **wallets and on-chain services**, and its differentiator is that it can **pay its own way** — discovering and paying for services autonomously (x402) instead of being handed API keys.

The **Plugins hub** we just built is the product's spine: it's the shelf of crypto capabilities you install onto an agent. Governance + secrets are the trust rail. The container is where the agent actually runs the CLIs.

## 2. The signature demo (the money shot)

> You: *"Pull the latest on-chain analytics for account 0.0.x and summarize the risk."*
> Agent: discovers an **x402-gated data service on Hedera**, sees it costs a few cents, **pays autonomously in HBAR/USDC** (within its governance budget — no API key), retrieves the data, and answers in chat — showing the **payment receipt** and the **governance policy** that authorized it.

One flow, four sponsor stories: agentic payment (Hedera), agent wallet + risk policy (Circle), leak-proof secrets + approval (Ledger), and pay-per-service discovery (Bazantic). This is what judges should see in 3 minutes.

## 3. What BalamOS already has (assets to leverage)

| Asset | Why it matters here |
|---|---|
| Chat-first agent-per-workspace UX | The "Grok bot" shape is already built |
| Sandboxed webtop container w/ CLIs (codex, opencode, circle, claude, gemini) | Where agent tools run; `circle` CLI already installed |
| **Hedera** read-only monitor + user-approved HBAR transfer intents (`hedera-agent-kit`) | A working Hedera harness to *extend*, not build |
| **Circle** skill guidance + `@circle-fin/cli` in image | Half of the Circle wallet story already scaffolded |
| **Governance** (approval / container / scheduled policy, per-workspace) | = the "spending policy / manage risk" story sponsors ask for |
| **Secrets** (redacted, scoped, per-workspace) | = "secrets agents can't leak" (Ledger) |
| **Plugins hub** (grid → detail → install) | The capability marketplace UI, already shipped |
| `orbit-daemon` protocol + gateways | Where new on-chain capabilities plug in |

## 4. Prize-fit matrix (targets)

| Track | $ | Fit | Effort |
|---|---|---|---|
| **Hedera — Improve the Harness** | 2k | ★★★★★ extend existing Hedera harness (+x402 plugin, coverage, docs) | Low |
| **Hedera — AI & Agentic Payments** | 6k | ★★★★★ agent discovers + pays x402 service on Hedera, no keys | Med |
| **Arc (Circle) — Agentic Economy** | 3.5k+2.5k | ★★★★☆ live USDC agent wallet; governance = spending policy; deploy to Arc | Med |
| **Ledger — AI Agents × Ledger** | 3.5k | ★★★★☆ Key Ring scoped secrets + device approval + x402 | Med |
| **The Graph — Best AI Tooling** | 5k | ★★★☆☆ Subgraph MCP → portfolio/market read ("what's in my wallet") | Med |
| **Bazantic — x402 Recipe** | 3k | ★★★☆☆ wrap an API as an x402 recipe the agent consumes | Low |
| World / ENS / Privy | 3.5–5k | ★★☆☆☆ agent identity (AgentBook/ENS), wallet flows | Varies |

**The through-line:** x402 agentic payments qualify for Hedera, Ledger, Bazantic and complement Circle. **Build the payment spine once, win across four sponsors.**

## 5. The capability set (plugin ideas → prize → vision)

Each is a **tile in the Plugins hub**, a **daemon capability**, and a **skill injected into the container's `AGENTS.md`** so the agent knows how to use it.

1. **x402 Wallet — "the agent that pays its own way"** *(spine)*
   The agent hits a 402-gated endpoint, reads terms, pays HBAR/HTS-USDC (Hedera) or USDC (Arc), retries, gets the resource. A small budget + governance cap gate every payment.
   → *Hedera Agentic Payments, Ledger, Bazantic; feeds Circle.* Grok-vision: the agent can buy the data/compute it needs to answer you.

2. **Hedera Harness+ (packaged tool layer)**
   Wrap `hedera-agent-kit v4` (policies, modular packages) + its x402 plugin as a first-class BalamOS Hedera harness exposed to any coding harness (codex/opencode/claude) in the container. Improve DX + service coverage + docs.
   → *Hedera — Improve the Harness.* Lowest effort, near-guaranteed fit.

3. **Agent Treasury (Circle Agent Wallet, real)**
   Promote Circle from guidance to a live wallet: create wallet, fund testnet USDC, spend under a **governance-enforced policy** (daily cap, allowed recipients/corridors). Show the policy in the Governance panel. Deploy demo to **Arc**.
   → *Arc (Circle) — Agentic Economy.* Grok-vision: the agent has its own money, safely bounded.

4. **Portfolio & Market Read (The Graph via MCP)**
   MCP tool so the agent answers "what's in my wallet / what's moving" from Subgraphs (balances, positions, prices).
   → *The Graph — Best AI Tooling.* Grok-vision: real-time on-chain knowledge in chat.

5. **Hardware-gated actions (Ledger)**
   High-value actions require a Ledger device tap; agent API secrets live in **Ledger Key Ring** (scoped, non-exfiltratable). Human-in-loop = your governance approval + a physical confirm.
   → *Ledger — AI Agents × Ledger.* Grok-vision: autonomy you can trust because a human/device gates the risky bits.

6. **Agent Identity (ENS / World)** *(stretch)*
   Each agent gets an ENS name + AgentBook entry so services and other agents can address and pay it.
   → *ENS agent namespaces, World AgentKit.*

## 6. Phased build plan (6 days, with cut lines)

**Phase 0 — x402 spine (Days 1–2).** Install/build an x402 **client** the container agent can invoke (CLI like `balamos-x402 pay <url>`), daemon-mediated so a payment first checks the **governance budget** and records a receipt. Prove it against a public/self-hosted 402 endpoint. *Cut line: if blocked, fall back to Hedera-only x402.*

**Phase 1 — Hedera (Days 2–3, 2 prizes).** (a) Host a tiny **x402 resource server on Hedera** (Hiero SDK facilitator; pay in HBAR or HTS-USDC). (b) Wire `hedera-agent-kit v4` x402 plugin into `orbit-daemon`, document the **harness** extension, expose its tools in-container. *This alone targets $8k.*

**Phase 2 — Circle/Arc (Days 3–4).** Live Circle Agent Wallet: create + fund (testnet USDC), spend under a governance policy, receipts in chat. Deploy the demo to **Arc** (mainnet if feasible for the bonus). *Targets $6k.*

**Phase 3 — Stretch (Days 5–6).** Pick ONE that lands cleanly: **Ledger** Key Ring + device confirm, **or** **The Graph** portfolio MCP. Both are additive to the spine.

**Day 6 — polish + demo.** Script the signature demo, record the video, architecture diagram, GitHub README per each sponsor's submission checklist.

## 7. Architecture (how each plugs in)

- **Container CLIs** (`balamos-x402`, `circle`, `balamos-hedera-*`) are the agent's hands; the agent invokes them from the sandboxed desktop, mirroring the existing `balamos-hedera-readonly` pattern.
- **`orbit-daemon`** mediates anything that spends: a new `x402`/`payments` module + protocol commands, gating on **governance** (budget, approval) and writing an auditable **receipt/intent** (reuse the `HbarTransferIntent` state-machine pattern).
- **Plugins hub** gains tiles (x402 Payments · Circle Wallet · Hedera · The Graph · Ledger); each tile's detail installs the skill + shows setup.
- **Skills → `AGENTS.md`**: each capability ships a skill (like the Circle/Hedera ones) telling the agent exactly which CLI/tool to call and the safety rules.
- **Governance = the spend policy**: caps, allowed recipients, require-approval — the sponsors' "manage risk" requirement is our existing feature.

## 8. Risks & mitigations

- **Time (~6 days).** Sequence by ROI: Hedera Harness (lowest) → x402 spine → Circle. Ledger/Graph are droppable.
- **Live wallet pairing / mainnet untested** (per existing Hedera notes). Keep testnet-first; mainnet only for the Arc bonus if stable.
- **This machine's memory/Docker instability** (recurring OOM). Do the on-chain demo on a lighter setup or record early.
- **x402 maturity.** Real-commerce volume is thin; keep the demo self-contained (our own 402 service) so it always works.
- **Scope creep.** Governance already exists — don't rebuild risk controls; wire to them.

## 9. Open decisions (for the team)

- Lock the core tracks (recommend: **Hedera ×2 + Circle/Arc** ≈ $11.5k, all on existing code).
- Primary settlement asset for the demo: **HBAR**, **HTS-USDC on Hedera**, or **USDC on Arc**?
- Which stretch: **Ledger** (security narrative) or **The Graph** (read/portfolio narrative)?
- Are we deploying to **Arc mainnet** by Sep 30 for the +$2.5k bonus?

## 10. Locked target set + container-first build order (2026-09-10 update)

**Added to the target set** (each strengthens the Grok-bot narrative, all cheap riders):
- **The Graph ($15k pool) — elevated to Tier 2.** MCP/CLI Subgraph reads = the "what's in my wallet / what's moving" layer. Track: *Best AI Tooling (from scratch)*.
- **Bazantic ($3k) — rides the x402 spine.** Wrap a sponsor API as an x402/MCP **recipe**; the "combine 2+ APIs" track = one agent flow that **reads The Graph and pays via Hedera x402**.
- **ENS ($5k) — agent namespaces.** Each agent gets `name.balamos.eth` as its **payable identity** for x402. Identity half of "every agent has a name + a wallet."

**Architecture principle — container-first.** Every capability ships as a **container-side CLI the agent invokes**, following the proven `packages/hedera-readonly` pattern: a zero-/few-dep ESM node CLI (`bin` in package.json, method dispatch, JSON to stdout, bounded `fetch`, a `ready` health check, `.test.mjs` tests), COPY'd into the webtop image + installed via `orbit-install-apps`, and taught to the agent through a **skill injected into `AGENTS.md`**. Anything that *spends* is additionally mediated by `orbit-daemon` + governance; **reads need no daemon**.

**Revised build order (starts now):**
1. **`packages/balamos-graph`** — The Graph read CLI (`ready`, `query <subgraph> <graphql>`, gateway URL via `GRAPH_API_KEY` secret). Read-only, no wallet, unblocked → **first build**. *(Graph)*
2. **`packages/balamos-x402`** — x402 client CLI (`pay <url>`): read 402 terms → pay (HBAR/HTS-USDC/USDC) → retry. Daemon-mediated + governance budget. *(spine: Hedera/Ledger/Bazantic/Circle)*
3. **Hedera x402 resource server** + harness extension (`hedera-agent-kit` v4 x402 plugin). *(Hedera ×2)*
4. **Bazantic recipe** — one agent flow: `balamos-graph` read + `balamos-x402` pay. *(Bazantic)*
5. **`packages/balamos-ens`** — resolve now; register `*.balamos.eth` subnames as the stretch. *(ENS)*
6. **Circle Agent Wallet** (real) + Arc deploy. *(Circle/Arc)*

Each becomes a Plugins-hub tile + an `AGENTS.md` skill so it's usable inside the container.

---

## 11. Build status (living — updated 2026-09-10)

| Component | What it does | Track | Status |
|---|---|---|---|
| `packages/balamos-graph` | Read-only Subgraph query CLI (`ready`, `query`) | The Graph | ✅ built + tested (5/5) |
| `packages/balamos-x402` | x402 protocol: `inspect` a paywall + header codec | Hedera x402 / Bazantic | ✅ built + tested (5/5) |
| `packages/x402-demo-service` | x402-gated demo endpoint (402 terms + mock paid) | Hedera x402 | ✅ built + tested (4/4), live-verified |
| `scripts/demo-x402.sh` | Reproducible discover→(mock)pay walkthrough | demo | 🔨 in progress |
| Plugins hub (grid→detail, premium) | Install/manage capabilities; 4 tiles | UX | ✅ shipped, 166/166 |
| Graph + x402 hub tiles + skills | Installable per-workspace, injected into `AGENTS.md` | all | ✅ shipped |
| Container wiring (Dockerfile + installer) | CLIs usable inside the agent's desktop | all | ✅ code done; verifies on image rebuild |
| **x402 settlement (real pay)** | Sign + settle HBAR/HTS-USDC under governance cap | Hedera / Circle / Ledger | ⛔ blocked: needs funded mainnet wallet + exact `@x402/hedera` scheme |
| Circle Agent Wallet (real) | USDC wallet + governance spend policy, Arc deploy | Circle/Arc | ⛔ pending wallet |
| ENS agent names | `*.balamos.eth` payable identity | ENS | ⛔ registration needs wallet (resolve-read is doable) |

**Executor:** task execution routed through **DeepSeek V4.1 Flash** (`opencode-go/deepseek-v4.1-flash`), Opus 4.8 plans/validates.

**Demo:** `bash scripts/demo-x402.sh` — starts the demo service and shows the agent discovering real x402 terms with no API key.

---

*Sources: [ETHOnline prizes](https://ethglobal.com/events/ethonline2026/prizes) · [x402 on Hedera](https://docs.hedera.com/solutions/ai/x402) · [Hedera x402 scheme](https://hedera.com/blog/hedera-and-the-x402-payment-standard/) · [Hedera Agent Kit V4](https://hedera.com/blog/hedera-agent-kit-v4-policies-modular-packages-and-plugin-updates/) · [Circle Agent Stack](https://www.circle.com/pressroom/circle-launches-ai-infrastructure-to-power-the-agentic-economy) · [The Graph docs](https://thegraph.com/docs/) · [ENS docs](https://docs.ens.domains/)*
