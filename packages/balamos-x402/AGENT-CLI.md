# BalamOS x402 CLI

`balamos-x402 inspect` requests an x402-protected URL and reports the required amount, asset, network, and `payTo` terms returned by the endpoint.

This is the protocol layer only. It does not sign or move funds; settlement is added later behind daemon/governance.

```sh
balamos-x402 ready
balamos-x402 inspect https://example.com/paid-resource
```
