// Read-only JSON wRPC probe for deploy.sh verify: node probe.mjs <wss://host/path> <genesis-hash>
// Prints one JSON object: { dag: getBlockDagInfo, genesisBlock: { found }, nodeStatus: getPalwNodeStatus }.
// Needs a Node.js with a global WebSocket (22+). It sends only read calls.
const [url, genesis] = process.argv.slice(2);
if (!url || !genesis) { console.error('usage: node probe.mjs <wss://host/path> <genesis-hash>'); process.exit(2); }
const ws = new WebSocket(url);
let id = 0; const pending = new Map();
const call = (method, params = {}) => new Promise((res, rej) => {
  const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params }));
});
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data); const p = pending.get(m.id);
  if (p) { pending.delete(m.id); m.error ? p.rej(m.error) : p.res(m.params ?? m.result ?? m); }
};
ws.onerror = (e) => { console.error('ws error', e.message ?? e); process.exit(1); };
ws.onopen = async () => {
  const out = {};
  try { out.dag = await call('getBlockDagInfo'); } catch (e) { out.dagError = e; }
  try {
    const b = await call('getBlock', { hash: genesis, includeTransactions: false });
    out.genesisBlock = { found: !!(b && b.block) };
  } catch (e) { out.genesisBlock = { found: false, error: e }; }
  try { out.nodeStatus = await call('getPalwNodeStatus'); } catch (e) { out.nodeStatusError = e; }
  console.log(JSON.stringify(out));
  ws.close(); process.exit(0);
};
setTimeout(() => { console.error('timeout'); process.exit(2); }, 20000);
