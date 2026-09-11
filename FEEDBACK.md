# Developer Feedback — ETHOnline 2026

Honest notes from building BalamOS's crypto capabilities. Grounded in what we actually integrated; where we didn't run something live, we say so.

## The Graph

We built two things against The Graph: `packages/balamos-graph` (a zero-dependency read-only Subgraph CLI) and `packages/balamos-graph-mcp` (an MCP server exposing a `query_subgraph` tool).

**What worked well**
- The gateway URL pattern `gateway.thegraph.com/api/<key>/subgraphs/id/<id>` is clean — we integrated it with **zero dependencies** (just `fetch` + POST GraphQL). That's a low barrier for agent tooling.
- Accepting either a **subgraph id or a full query URL** kept the tool flexible without extra config.
- Wrapping Subgraph queries as an **MCP tool** was straightforward over stdio JSON-RPC.

**Friction / suggestions**
- Requiring an **API key even for reads** means agents must carry a secret; a keyless dev/hackathon read tier (rate-limited) would let an agent demo "what's in my wallet" with no setup.
- The hard part for an *agent* isn't the query transport — it's **discovering the right Subgraph** for a natural-language question. A canonical balances/portfolio Subgraph, or exposing the **Token API as a first-party MCP tool**, would be far higher-leverage for portfolio agents than raw GraphQL against arbitrary schemas.
- A **first-party Subgraph MCP reference server** (with the tool schema) would save every team re-implementing one.
- Note: we unit-tested the client and MCP server (5/5 and 7/7) but did **not** run against a live Subgraph in this window (no API key on hand), so we can't report real-data latency/quirks.

## x402 (Hedera / general)

We built an x402 client (`packages/balamos-x402`) and a demo x402-gated service.

- The **v1↔v2 field drift** (`maxAmountRequired` vs `amount`, plain network strings vs CAIP-2) was the main friction — a client can't tell which a server speaks without trying. A single canonical `PaymentRequirements` schema, or a required `x402Version` the client keys off, would remove guesswork.
- For **Hedera specifically**, a prominent link from the x402 overview docs straight to the `@x402/hedera` reference (exact payload field names for the partially-signed transfer) would have saved a lot of digging — the overview explains *what* x402 is but not the byte-level *how*.
- The **facilitator-pays-gas** model on Hedera is a nice fit for agents (the paying agent doesn't need gas tokens separately) — worth foregrounding in the docs.
