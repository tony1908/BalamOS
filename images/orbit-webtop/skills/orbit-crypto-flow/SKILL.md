# Orbit Crypto Flow — research, pay, and settle

Use this skill when the user asks you to research a token/market on-chain and, if a
paid signal or service is cheap enough, pay for it and act — e.g. *"check $TOKEN
on-chain and if the premium signal is under my cap, buy it."* You are the "Grok bot
of crypto": you research with The Graph, pay autonomously over x402 on Hedera within
a governance cap, use a Circle USDC wallet as treasury, and hand the desktop back to
the human for anything you must not automate.

Run the steps in order. Report each step's result in chat before moving on. Never
invent numbers — only report values a tool actually returned.

## 0. Preconditions

- `command -v balamos-graph balamos-x402 circle` — confirm the tools exist.
- The Hedera agent wallet arrives as env vars `HEDERA_OPERATOR_ID` and
  `HEDERA_OPERATOR_KEY` (assigned as BalamOS secrets). If either is missing, stop and
  tell the user to assign the wallet secret to this agent in the Secrets panel. Never
  print the key; never write it to a file.
- The Graph needs `GRAPH_API_KEY` (also a secret). If missing, say the research step
  will be skipped.
- `cast`/`anvil` (Foundry) are available for EVM/Arc reads and local-fork simulation.
- Start the treasury dashboard so the user can watch balances move, then open it in the
  browser: `TREASURY_ACCOUNTS="$HEDERA_OPERATOR_ID,<payTo>" balamos-treasury &` and open
  `http://localhost:4030`.

## 1. Research — The Graph

Query on-chain context with the `query_subgraph` MCP tool (preferred) or
`balamos-graph query <subgraph-id> '<graphql>'`. Fetch what the request needs (price,
liquidity, volume, recent activity). Summarize the real numbers you got back.

## 2. Discover — x402

`balamos-x402 inspect <url>` against the paid endpoint. Read and report the price
(`maxAmountRequired` in tinybars), `asset`, `network`, and `payTo`. No key is needed
to discover terms.

## 3. Decide

Compare the cost to the governance cap (`X402_MAX_TINYBARS`, tinybars; 1 HBAR =
100000000) and the user's stated budget. State a clear **buy** or **skip** with the
one-line reason. If skip, stop here and report why.

## 4. Pay — x402 over Hedera (only if "buy")

**Simulate before you spend.** For a Hedera HBAR payment, first confirm the wallet
balance covers the amount plus fees (via the treasury dashboard or
`balamos-hedera-readonly`). For any EVM/Arc action, dry-run it on a local fork first:
`anvil --fork-url <rpc>`, send against the fork with `cast`, inspect the result — only
then touch the real network.

`balamos-x402 pay <url>`. This signs and submits a real HBAR transfer from the agent
wallet to `payTo`, gated by the cap, then re-requests with proof and returns the
unlocked resource. Report `paid`, the `transactionId`, and the HashScan URL. If the
cap blocks it, report that and stop — do not try to raise the cap yourself.

## 5. Treasury — Circle USDC wallet (ARC-TESTNET)

Check the agent's Circle wallet:
`circle wallet list --chain ARC-TESTNET --type agent --output json`, then
`circle wallet balance --address <ADDR> --chain ARC-TESTNET --output json`.

If the CLI reports no session / not logged in, **you must NOT automate the login or
the OTP.** Instead:

1. Post in chat: *"I need you to log into Circle — I can't handle the OTP. Take
   control of the desktop (the 'Take control' button), open a terminal, run
   `circle wallet login email --testnet`, complete the code from your email, then hand
   control back and reply 'continue'."*
2. Stop and wait. The user will pause you, log in, and resume you with a message.
3. On resume, re-run the balance check — it now succeeds — and report the balance.

For any transfer the user explicitly authorized, estimate first with `--estimate`;
omit `--estimate` only after they authorize that exact transfer. Never spend
otherwise. See the Circle guidance in AGENTS.md.

## 6. Log

Write a short markdown summary to `/workspace` (research numbers, buy/skip, the
settlement tx + HashScan link, the Circle balance). Keep secrets out of it.

## Desktop control / handoff

The handoff in step 5 is user-driven: they click "Take control" in the app, act, then
resume you. To drive the GUI yourself when appropriate, read
`/opt/orbit/skills/orbit-desktop-control/SKILL.md` (scrot + xdotool). Never take back
control while the user holds it; wait for their resume message.
