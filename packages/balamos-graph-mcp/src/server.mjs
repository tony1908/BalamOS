import readline from "node:readline";

const TOOL = {
  name: "query_subgraph",
  description: "Run a read-only GraphQL query against a subgraph on The Graph. Provide a subgraph id (or full query URL) and a GraphQL query. Never signs or spends.",
  inputSchema: {
    type: "object",
    properties: {
      subgraph: { type: "string", description: "Subgraph ID or full query URL" },
      query: { type: "string", description: "GraphQL query string" },
      variables: { type: "object", description: "Optional GraphQL variables" }
    },
    required: ["subgraph", "query"]
  }
};

export function resolveEndpoint(subgraph, apiKey) {
  if (subgraph.startsWith("http://") || subgraph.startsWith("https://")) return subgraph;
  if (!apiKey) throw new Error("GRAPH_API_KEY is required to query by subgraph id");
  if (!/^[A-Za-z0-9]+$/.test(subgraph)) throw new Error("invalid subgraph id");
  return `https://gateway.thegraph.com/api/${apiKey}/subgraphs/id/${subgraph}`;
}

export async function runQuery({ subgraph, query, variables }, fetchImpl) {
  const endpoint = resolveEndpoint(subgraph, process.env.GRAPH_API_KEY ?? "");
  const response = await fetchImpl(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query, variables: variables ?? {} }),
    signal: AbortSignal.timeout(15000)
  });
  if (!response.ok) throw new Error(`graph gateway returned HTTP ${response.status}`);
  return JSON.stringify(await response.json());
}

function errorResponse(id, code, message) {
  return { jsonrpc: "2.0", id, error: { code, message } };
}

export async function handleMessage(msg, opts = {}) {
  if (msg.method === "initialize") {
    return {
      jsonrpc: "2.0",
      id: msg.id,
      result: {
        protocolVersion: (msg.params && msg.params.protocolVersion) || "2025-06-18",
        capabilities: { tools: {} },
        serverInfo: { name: "balamos-graph-mcp", version: "0.1.0" }
      }
    };
  }
  if (typeof msg.method === "string" && msg.method.startsWith("notifications/")) return null;
  if (msg.method === "tools/list") {
    return { jsonrpc: "2.0", id: msg.id, result: { tools: [TOOL] } };
  }
  if (msg.method === "tools/call") {
    const name = msg.params && msg.params.name;
    const args = (msg.params && msg.params.arguments) || {};
    if (name !== "query_subgraph") return errorResponse(msg.id, -32602, "unknown tool");
    try {
      const text = await runQuery(args, opts.fetchImpl || fetch);
      return { jsonrpc: "2.0", id: msg.id, result: { content: [{ type: "text", text }] } };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return { jsonrpc: "2.0", id: msg.id, result: { content: [{ type: "text", text: "error: " + message }], isError: true } };
    }
  }
  return errorResponse(msg.id, -32601, "method not found");
}

async function main() {
  const rl = readline.createInterface({ input: process.stdin });
  rl.on("line", async (line) => {
    if (!line.trim()) return;
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      return;
    }
    const response = await handleMessage(msg);
    if (response !== null) process.stdout.write(JSON.stringify(response) + "\n");
  });
}

if (import.meta.url === `file://${process.argv[1]}`) main();
