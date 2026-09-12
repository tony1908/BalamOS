# x402 Settlement Layer — Design

**Status (2026-09-12): BUILT & live-verified on Hedera testnet.** `balamos-x402 pay <url>` settles real HBAR to `payTo` and the demo service verifies it on the mirror node before serving the resource (example: [`0.0.10512454@1789257195…`](https://hashscan.io/testnet/transaction/0.0.10512454@1789257195.006019834)). Implemented as the lazy slice below:
- `packages/hedera-readonly/src/settle.mjs` — direct operator-signed testnet/mainnet HBAR transfer (`settleHbarTransfer`).
- `packages/balamos-x402/src/cli.mjs` — `pay`: inspect → select `exact` requirement → **governance spend cap** (`X402_MAX_TINYBARS`, default 1 HBAR) → settle → re-request with `X-PAYMENT` proof.
- `packages/x402-demo-service/` — `verify.mjs` verifies the transfer to `payTo` on the public mirror node (no key), with retry for indexing lag; the mock paid path is gone.
- `scripts/demo-x402.sh` — discovery always; real settlement when `HEDERA_KEY_FILE` is set.

**Deferred polish (not required for the working demo):** move settlement into the `orbit-daemon` state machine, surface the spend cap in the in-app Governance panel, and add HTS-USDC alongside HBAR. The rest of this doc is the original design for that fuller build.

---

**Original design.** Everything else reuses code we already have.

## Goal

Give `balamos-x402` a `pay <url>` command that actually settles an x402 payment on Hedera — **governance-gated, autonomous-within-cap** — so the agent can discover *and pay for* a service with no API key. Today `inspect` surfaces the terms; this adds real settlement.

## The Hedera "exact" scheme (how payment works)

Unlike EVM (EIP-3009 `transferWithAuthorization` + EIP-712 signature), Hedera's exact scheme uses a **partially-signed native transaction**: the client builds and signs a transfer (HBAR or an HTS token like USDC) to `payTo`, and a **facilitator submits it and pays the gas**. The client's signature authorizes the transfer; it does not broadcast.

## What we reuse (most of it already exists)

- **`packages/hedera-readonly/transactions.mjs`** — already does `buildTransaction` / `validateSignedTransaction` / `submitTransaction` / `reconcile` for HBAR transfers (the existing intent flow). The settlement payload = a signed transfer, so this is ~80% of the crypto.
- **Daemon `HbarTransferIntent` state machine** (`orbit-protocol`) — freeze → approve → submit → confirm, with digests. The x402 payment is the same shape: freeze the transfer, gate it, sign, settle.
- **Governance** (`GovernancePolicy`) — the "manage risk" requirement. Add a **spend cap** (per-payment + daily max) as the gate.
- **Secrets** — the agent wallet's operator key lives here (redacted, never logged, never in repo).

## Flow

```
agent: balamos-x402 pay <url>
  1. inspect <url>                     -> accepts[]              (already built)
  2. selectRequirement(accepts, {network:"hedera-*", scheme:"exact"})   (already built)
  3. daemon governance gate: amount ≤ per-payment cap, under daily cap,
     require_approval? -> pause for user approval                (reuse governance)
  4. build transfer tx: from agentWallet -> payTo, amount, asset (HBAR or HTS)
     (reuse transactions.mjs buildTransaction)
  5. sign with agent wallet key (from BalamOS secret)            (reuse sign path)
  6. encode PaymentPayload -> base64 -> X-PAYMENT header         (encodePaymentHeader exists)
  7. re-request <url> with header -> 200 + resource
     (server's facilitator verifies+settles; or we POST /verify then /settle)
```

## Wire format (x402 v2)

- **PaymentRequirements** (accepts[]): `scheme`, `network` (CAIP-2, e.g. a Hedera id), `amount` (atomic units), `asset`, `payTo`, `maxTimeoutSeconds`, `extra`.
- **PaymentPayload** (X-PAYMENT, base64): `{ x402Version, resource, accepted, payload, extensions }`. The Hedera-specific `payload` carries the **partially-signed transaction** (confirm exact field name against `@x402/hedera` — EVM uses `{signature, authorization{from,to,value,validAfter,validBefore,nonce}}`; Hedera carries the signed transfer bytes).
- **Facilitator**: `POST /verify` then `POST /settle`, each `{x402Version, paymentPayload, paymentRequirements}`; settle returns `{success, payer, transaction, network}`.
- **Note — v1↔v2 drift:** our current `inspect`/demo used v1 shapes (`maxAmountRequired`, non-CAIP networks). `inspect` is scheme-agnostic so it's fine; the `pay` builder must match whatever version the target facilitator speaks. Align to the Hedera facilitator at build time.

## Custody & safety (decided)

- **Autonomous-within-governance**: the agent pays from its own wallet without a per-tx human tap, but every payment must pass the governance cap; over-cap or policy-flagged payments require explicit approval.
- **Dust-funded**: the wallet holds only a few dollars → bounded worst case.
- **Key custody stays with the user**: they fund the wallet and store the operator key as a BalamOS secret. The daemon signs; the key never enters prompts, logs, or the repo. Claude never handles the key value.

## Build steps (when unblocked)

1. Add a `payments` module in `orbit-daemon`: governance-gated `PrepareX402Payment` / `SubmitX402Payment` mirroring the HBAR intent commands.
2. Extend `balamos-x402` with `pay <url>` (steps above), reusing `transactions.mjs`.
3. Add a **spend-cap** field to governance (per-payment + daily) + surface in the Governance panel.
4. Point the `x402-demo-service` at real verification (drop the mock) OR integrate the Hedera facilitator.
5. Tests: unit-test PaymentPayload construction with a mock signer (host); integration on **Hedera testnet** with a dust wallet before mainnet.

## Needed from the user

- A **funded Hedera wallet** (operator account id + private key) — stored as a BalamOS secret.
- Choice of settlement asset: **HBAR** or **HTS-USDC**.
- The target: our own `x402-demo-service` upgraded to verify, or an external Hedera x402 facilitator/service.
