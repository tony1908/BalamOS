# x402 Demo Service

This is a minimal x402-gated endpoint for testing the `balamos-x402` client. It serves demo premium data and returns payment terms when no `X-PAYMENT` header is present.

Run it with:

```sh
node packages/x402-demo-service/src/server.mjs
```

Configure it with `PORT`, `X402_NETWORK`, `X402_PAY_TO`, `X402_ASSET`, and `X402_AMOUNT` environment variables.

For example, after starting the server, `balamos-x402 inspect http://localhost:4021/` returns the payment terms.

The `200` path is a MOCK: the `X-PAYMENT` header is accepted without on-chain verification until real settlement and facilitator verification are added.
