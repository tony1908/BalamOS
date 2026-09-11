export const THE_GRAPH_SKILL_NAME = "The Graph Subgraph Reader";
export const THE_GRAPH_SKILL_INSTRUCTION = `Use the \`balamos-graph\` CLI for read-only on-chain data from The Graph's Subgraphs. It never signs, transfers, spends, or fabricates data.

- Check readiness first: \`balamos-graph ready\`. Continue only when it reports \`ready\`: true (this requires the GRAPH_API_KEY environment secret to be set).
- Query a Subgraph: \`balamos-graph query <SUBGRAPH_ID or full query URL> '<graphql query>'\`. Pass GraphQL variables with repeated \`--var key=value\` flags. The command prints the raw JSON response.
- Use it to answer portfolio, balance, position, and price questions ("what's in my wallet", recent activity, token holdings). Choose a Subgraph appropriate to the chain/protocol being asked about.
- Summarize the returned JSON plainly and never invent fields or values that are not present.
- This capability is strictly read-only: it cannot sign transactions, move funds, or make payments.`;
