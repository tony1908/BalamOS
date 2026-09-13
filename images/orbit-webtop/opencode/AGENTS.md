# Container Agent Guidance

## Environment

- You run as user `abc`.
- The home directory is `/config`.
- The user's project is mounted at `/workspace`; do project work there.
- Verify the home directory with `printenv HOME`.

## Available CLIs

The preinstalled CLIs are `codex`, `opencode`, `circle`, `claude`, and `gemini`.

- Check availability with `command -v <tool>`.
- Check a tool version with `<tool> --version` before using it.

## Desktop Control

The screenshot and input skill is at `/opt/orbit/skills/orbit-desktop-control/SKILL.md`.
Read it before driving the GUI. It documents `scrot` screenshots and `xdotool` input.

## Hedera & x402 payments

- `balamos-hedera-readonly` provides public read-only Hedera Mainnet snapshots. It
  never signs or transfers.
- `balamos-x402` handles agentic payments over the x402 protocol:
  - `balamos-x402 inspect <url>` reads a paid endpoint's terms (no key needed).
  - `balamos-x402 pay <url>` settles the payment: it signs and submits a real HBAR
    transfer from the agent wallet to `payTo`, gated by a governance spend cap
    (`X402_MAX_TINYBARS`), then re-requests with proof. The wallet arrives as env vars
    `HEDERA_OPERATOR_ID` / `HEDERA_OPERATOR_KEY` (BalamOS secrets). Only pay within the
    cap and only what the user asked for; if the cap blocks a payment, report it — do
    not raise the cap.

## The Graph

Query Subgraphs with the `query_subgraph` MCP tool or `balamos-graph query <id>
'<graphql>'` for on-chain context (price, liquidity, volume). Needs `GRAPH_API_KEY`.

## EVM / Arc (Foundry)

`cast` and `anvil` (Foundry) read/call/send on any EVM chain, including Circle's Arc.
Before any real EVM spend, simulate on a local fork: `anvil --fork-url <rpc>`, then
`cast` against the fork. Only send to the real network once the fork run looks right.

## Wallets in the browser

Chromium ships with MetaMask and HashPack force-installed. They need a one-time
seed/import you must NOT automate — ask the user to take manual desktop control to set
them up (see the handoff in the crypto-flow skill).

## Treasury dashboard

`balamos-treasury` serves a local balances dashboard at http://localhost:4030 (set
`TREASURY_ACCOUNTS` to the accounts to watch). Start it and open it so the user can see
funds move as you transact.

## Crypto research + pay flow

For "research a token/market and pay for a signal if it's cheap enough" requests,
follow `/opt/orbit/skills/orbit-crypto-flow/SKILL.md`.

## Circle Agent Wallets

- The `circle` CLI is ARC-TESTNET-first.
- Never automate login or OTP flows.
- Never spend without explicit user authorization.
- Keep credentials out of prompts, files, and logs.

## Safety

- Never store secrets, private keys, or tokens in files, prompts, or logs.
- Do only what the user explicitly authorized.
