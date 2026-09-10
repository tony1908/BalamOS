# The Graph Agent CLI

`balamos-graph` lets an agent make read-only GraphQL queries against The Graph subgraphs. Set `GRAPH_API_KEY` before querying by Subgraph ID.

Commands:

```sh
balamos-graph ready
balamos-graph query QmSubgraphId123 'query { ... }'
```

Queries may also be read from stdin, and repeated `--var key=value` flags provide GraphQL variables. This helper never signs transactions or spends funds; it only reads subgraph data.
