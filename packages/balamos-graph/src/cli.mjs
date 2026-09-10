const allowed = new Set(["ready", "query"]);
const fail = (message) => { process.stderr.write(`${message}\n`); process.exitCode = 1; };

export function resolveEndpoint(target, apiKey) {
  if (target.startsWith("http://") || target.startsWith("https://")) return target;
  if (!apiKey) throw new Error("GRAPH_API_KEY is required to query by subgraph id");
  if (!/^[A-Za-z0-9]+$/.test(target)) throw new Error("invalid subgraph id");
  return `https://gateway.thegraph.com/api/${apiKey}/subgraphs/id/${target}`;
}

export function parseVars(args) {
  const variables = {};
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] !== "--var") continue;
    const assignment = args[++index];
    if (assignment === undefined) throw new Error("--var requires key=value");
    const separator = assignment.indexOf("=");
    if (separator < 1) throw new Error("--var requires key=value");
    const key = assignment.slice(0, separator);
    const rawValue = assignment.slice(separator + 1);
    try { variables[key] = JSON.parse(rawValue); } catch { variables[key] = rawValue; }
  }
  return variables;
}

async function readStdin() {
  return new Promise((resolve, reject) => {
    let data = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => { data += chunk; if (data.length > 1_000_000) reject(new Error("stdin payload too large")); });
    process.stdin.on("end", () => resolve(data));
    process.stdin.on("error", reject);
  });
}

async function main() {
  const args = process.argv.slice(2);
  const method = args[0];
  if (!allowed.has(method)) throw new Error("method is not allowed");
  if (method === "ready") {
    const hasApiKey = typeof process.env.GRAPH_API_KEY === "string" && process.env.GRAPH_API_KEY.length > 0;
    console.log(JSON.stringify({ ready: hasApiKey, network: "the-graph", has_api_key: hasApiKey, helper_version: "0.1.0", message: hasApiKey ? "The Graph query helper ready" : "GRAPH_API_KEY is not set" }));
    return;
  }
  const target = args[1];
  if (!target) throw new Error("query target is required");
  const hasQueryArgument = args[2] !== undefined && args[2] !== "--var";
  const query = hasQueryArgument ? args[2] : (await readStdin()).trim();
  const variables = parseVars(args.slice(hasQueryArgument ? 3 : 2));
  const endpoint = resolveEndpoint(target, process.env.GRAPH_API_KEY ?? "");
  const response = await fetch(endpoint, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ query, variables }), signal: AbortSignal.timeout(15000) });
  if (!response.ok) throw new Error(`graph gateway returned HTTP ${response.status}`);
  console.log(JSON.stringify(await response.json()));
}

if (import.meta.url === `file://${process.argv[1]}`) main().then(() => process.exit(0)).catch((error) => fail(error instanceof Error ? error.message.slice(0, 500) : String(error).slice(0, 500)));
