# Hedera Agent CLI

After transfer approvals are configured, start a new agent session. The daemon injects `BALAMOS_HEDERA_AGENT_URL` and `BALAMOS_HEDERA_AGENT_TOKEN` into that agent container. No new capability files are written; tokens are never shown to the desktop.

Commands:

```sh
balamos-hedera-transfer propose 0.0.11 1.00000001 request-123
balamos-hedera-transfer list
balamos-hedera-transfer status INTENT_ID
```

`propose` sends an exact integer tinybar amount to the authenticated gateway. It does not expose signed bytes or approve, configure, submit, or reconcile operations. Natural-language requests or `balamos-hedera-transfer propose RECIPIENT AMOUNT_HBAR UNIQUE_ID` may propose; the agent may only propose, list, and inspect status. The user must Approve and sign in the configured external wallet, then explicitly Submit approved transfer. Transactions expire after approximately 120 seconds; unknown and submitting intents reconcile without auto-resend. No autonomous funds, arbitrary signing/HCS/tokens, or private key storage is allowed.
