import { createServer } from "node:http";

const PORT = process.env.PORT || 4030;
const NETWORK = String(process.env.TREASURY_NETWORK || "testnet").includes("main") ? "mainnet" : "testnet";
const ACCOUNTS = (process.env.TREASURY_ACCOUNTS || "0.0.10512454,0.0.10512599")
  .split(",").map((s) => s.trim()).filter(Boolean);
const MIRROR = `https://${NETWORK}.mirrornode.hedera.com/api/v1`;
const LABELS = { "0.0.10512454": "Agent wallet", "0.0.10512599": "Service wallet" };

// One account's live snapshot from the public mirror node (no key needed).
export async function accountSnapshot(id, fetcher = fetch) {
  const snap = { id, label: LABELS[id] || id, hbar: null, recent: [], hashscan: `https://hashscan.io/${NETWORK}/account/${id}` };
  try {
    const res = await fetcher(`${MIRROR}/accounts/${id}`, { signal: AbortSignal.timeout(8000) });
    if (res.ok) { const j = await res.json(); snap.hbar = Number(j?.balance?.balance ?? 0) / 1e8; }
  } catch { /* offline: leave hbar null */ }
  try {
    const res = await fetcher(`${MIRROR}/transactions?account.id=${id}&limit=5&order=desc`, { signal: AbortSignal.timeout(8000) });
    if (res.ok) {
      const j = await res.json();
      snap.recent = (j?.transactions ?? []).map((tx) => ({ id: tx.transaction_id, result: tx.result, name: tx.name }));
    }
  } catch { /* offline: leave recent empty */ }
  return snap;
}

export async function treasury(accounts = ACCOUNTS, fetcher = fetch) {
  return { network: NETWORK, accounts: await Promise.all(accounts.map((id) => accountSnapshot(id, fetcher))) };
}

const PAGE = `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>BalamOS Treasury</title>
<style>
  body{margin:0;font:14px system-ui,sans-serif;background:#0b0e14;color:#e6e6e6}
  header{padding:18px 22px;border-bottom:1px solid #1e2530}
  h1{margin:0;font-size:15px;letter-spacing:.08em;text-transform:uppercase;color:#9aa4b2}
  .net{font-size:12px;color:#6b7684;margin-top:4px}
  .grid{display:grid;gap:16px;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));padding:22px}
  .card{background:#131824;border:1px solid #1e2530;border-radius:14px;padding:18px}
  .label{font-size:12px;color:#9aa4b2}
  .id{font-family:ui-monospace,monospace;font-size:12px;color:#6b7684}
  .bal{font-size:30px;font-weight:600;margin:10px 0}
  .bal span{font-size:14px;color:#6b7684;font-weight:400}
  .tx{font-family:ui-monospace,monospace;font-size:11px;color:#8b95a3;padding:4px 0;border-top:1px solid #1a2130;display:flex;justify-content:space-between;gap:8px}
  .ok{color:#3fb950}.bad{color:#f85149}
  a{color:#58a6ff;text-decoration:none}
</style></head><body>
<header><h1>BalamOS Agent Treasury</h1><div class="net" id="net">connecting…</div></header>
<div class="grid" id="grid">Loading…</div>
<script>
async function tick(){
  try{
    const r = await fetch('/balances'); const d = await r.json();
    document.getElementById('net').textContent = 'Hedera ' + d.network + ' · live';
    document.getElementById('grid').innerHTML = d.accounts.map(function(a){
      var txs = a.recent.map(function(t){
        return '<div class="tx"><span>'+(t.name||'tx')+'</span><span class="'+(t.result==='SUCCESS'?'ok':'bad')+'">'+(t.result||'')+'</span></div>';
      }).join('');
      var bal = a.hbar==null ? '—' : a.hbar.toLocaleString(undefined,{maximumFractionDigits:4});
      return '<div class="card"><div class="label">'+a.label+'</div><div class="id">'+a.id+'</div>'+
        '<div class="bal">'+bal+' <span>HBAR</span></div>'+txs+
        '<div style="margin-top:8px"><a href="'+a.hashscan+'" target="_blank" rel="noopener">HashScan</a></div></div>';
    }).join('');
  }catch(e){ document.getElementById('net').textContent = 'offline'; }
}
tick(); setInterval(tick, 5000);
</script></body></html>`;

const server = createServer((req, res) => {
  if ((req.url || "").startsWith("/balances")) {
    treasury().then((data) => { res.writeHead(200, { "content-type": "application/json" }); res.end(JSON.stringify(data)); })
      .catch((e) => { res.writeHead(500, { "content-type": "application/json" }); res.end(JSON.stringify({ error: String(e?.message ?? e) })); });
    return;
  }
  res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  res.end(PAGE);
});

if (import.meta.url === `file://${process.argv[1]}`) {
  server.listen(PORT, () => console.log(`balamos treasury dashboard on http://localhost:${PORT} (${NETWORK}: ${ACCOUNTS.join(", ")})`));
}
