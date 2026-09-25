"use strict";
/* MISAKAScan — minimal explorer for the Misaka (kaspa-pq) devnet.
   Talks directly to the node's wRPC-JSON over WebSocket:
     send    {"id":N,"method":"<camelCase>","params":{...}}
     receive {"id":N,"method":"...","params":{...}}                */

const WS_PATH      = "/kaspa";       // nginx → local .51 node (sub-ms RTT) — all views except Peers
const WS_PATH_SEED = "/kaspa-seed";  // nginx → SSH tunnel → seed (mesh hub) — Peers view only
// Read-only RPC vantages used for the node census. The explorer and hub are separate nodes;
// the seed path may point at either one depending on deployment. The page unions their peer
// identities instead of pretending that one node's peer list is the whole network.
const WS_PATH_HUB = "/kaspa-hub";
const SYMBOL       = "MSK";      // Misaka devnet coin label (8 decimals)
const DECIMALS     = 8;
const RECENT_LIMIT = 25;
const TXSCAN_LIMIT = 400;        // how far back to scan for a tx id / block-less lookups

/* ---- kaspa-pq overlay (PoS / DNS-finality) additions ---- */
// Overlay transaction kinds are identified by the tx subnetwork id (40-hex on the wire).
const SUBNET = {
  "0000000000000000000000000000000000000000": { key:"native",   label:"standard",    cls:"std"   },
  "0100000000000000000000000000000000000000": { key:"coinbase", label:"coinbase",    cls:"blue"  },
  "1000000000000000000000000000000000000000": { key:"bond",     label:"stake bond",  cls:"bond"  },
  "1100000000000000000000000000000000000000": { key:"att",      label:"attestation", cls:"att"   },
  "1200000000000000000000000000000000000000": { key:"slash",    label:"slashing",    cls:"slash" },
};
/* ---- kaspa-pq EVM Lane (ADR-0020) additions ---- */
const EVM_CHAIN_ID         = 5063499;       // 0x4D534B = "MSK"
const EVM_BASE_FEE_GWEI    = 1;             // EVM_INITIAL_BASE_FEE = 1e9 wei
const EVM_SKIP_CLASS = { 1:"payload-invalid (class 1)", 2:"acceptance skip (class 2)", 3:"duplicate (class 3)",
                         4:"reverted (class 4)", 5:"gas-cap prefix skip (class 5)" };

// Decode a block's EvmExecutionPayload (the byte array `block.evmPayload` from getBlock/getBlocks)
// into the EVM transactions it carries. Borsh field order (consensus/core/src/evm/mod.rs):
//   system_ops: Vec<EvmSystemOp> | transactions: Vec<Vec<u8>> | evm_coinbase: [u8;20] | extra_data: Vec<u8>
// Each transaction is raw EIP-2718 bytes; its Ethereum hash is keccak256(raw) — the exact value
// getEvmTransactionReceipt / #/evmtx accept. (An empty payload is 32 bytes; a 1-tx block ~144.)
// Deposit-claim blocks (system_ops > 0) are rare; we report the count and skip tx extraction there.
function decodeEvmPayload(arr){
  try {
    if (!Array.isArray(arr) || arr.length < 8) return { txs: [], claims: [], systemOps: 0, empty: true };
    let o = 0;
    const u32 = () => { const v = (arr[o] | (arr[o+1]<<8) | (arr[o+2]<<16) | (arr[o+3]<<24)) >>> 0; o += 4; return v; };
    const u64 = () => { let v = 0; for (let i = 7; i >= 0; i--) v = v * 256 + (arr[o+i] & 0xff); o += 8; return v; };
    const hex = (n) => { let s = ""; for (let i = 0; i < n; i++) s += (arr[o+i] & 0xff).toString(16).padStart(2,"0"); o += n; return s; };
    // §9.2 deposit-claim system ops. Borsh EvmSystemOp = u8 tag (0 = DepositClaim) then
    // DepositClaim{ deposit_outpoint: {transaction_id:[u8;64], index:u32}, evm_address:[u8;20],
    // amount_sompi:u64, claim_tip_sompi:u64 } = 105 bytes/claim. These are the UTXO→EVM bridge
    // credits — surfaced as EVM activity (they don't have an Ethereum tx hash, they're system ops).
    const sysCount = u32();
    const claims = [];
    let otherOps = 0;
    for (let i = 0; i < sysCount && i < 4096; i++) {
      const tag = arr[o]; o += 1;
      if (tag !== 0) { otherOps = sysCount - i; break; }   // an op without a known layout: its bytes shift every field after it
      const txid = hex(64);
      const index = u32();
      const evmAddress = "0x" + hex(20);
      const amountSompi = u64();
      const tipSompi = u64();
      claims.push({ outpoint: txid + ":" + index, evmAddress, amountSompi, tipSompi });
    }
    if (otherOps) return { txs: [], claims, systemOps: sysCount, otherOps, decoded: false, unknownOp: true };
    const txCount = u32();
    const txs = [];
    for (let i = 0; i < txCount && i < 4096; i++) {
      const len = u32();
      if (len <= 0 || o + len > arr.length) break;
      const raw = arr.slice(o, o + len); o += len;
      const hash = (typeof keccak256 === "function") ? keccak256(raw) : null;
      txs.push({ hash, len, type: raw[0] });
    }
    return { txs, claims, systemOps: sysCount, otherOps: 0, decoded: true };
  } catch (e) { return { txs: [], claims: [], systemOps: 0, otherOps: 0, decoded: false }; }
}
function evmTxTypeLabel(t){ return t === 2 ? "EIP-1559" : t === 1 ? "EIP-2930" : (t >= 0xc0 ? "legacy" : "type "+t); }
const ROLLOUT_STAGES = ["Launch", "Bootstrap", "Active"];
const DNS_HEALTH = ["disabled (pre-activation)", "active", "degraded: stake quality low", "degraded: cert censored"];
const OVERLAY_REFRESH_MS = 8000;   // throttle for the (heavier) mesh-peer + on-chain overlay scan
// Cached overlay snapshot, refreshed off the fast block poll. Drives the home cards + #/overlay page.
let overlayStats = { ts:0, meshPeers:null, meshVantages:0, powAlgoId:null, activeValidators:0, bonds:[], totalStaked:0, attShards:0, epochs:[], rolloutActive:false };

/* ----------------------------- wRPC client ----------------------------- */
let ws = null, nextId = 1, reconnectDelay = 1000;
const pending = new Map();
let openResolvers = [];
let connected = false;
let heartbeatTimer = null;
const HEARTBEAT_MS = 10000;       // ping cadence
const HEARTBEAT_TIMEOUT = 7000;   // max wait for a ping reply before forcing reconnect

function wsUrl() {
  return (location.protocol === "https:" ? "wss://" : "ws://") + location.host + WS_PATH;
}

function setConn(up) {
  connected = up;
  const c = document.getElementById("conn");
  if (!c) return;
  c.className = "conn " + (up ? "up" : "down");
  document.getElementById("connText").textContent = up ? "node connected" : "reconnecting…";
}

function connect() {
  try { ws = new WebSocket(wsUrl()); }
  catch (e) { scheduleReconnect(); return; }

  ws.onopen = () => {
    setConn(true); reconnectDelay = 1000;
    openResolvers.forEach(r => r()); openResolvers = [];
    startHeartbeat();
    armSubscriptions();   // re-subscribe on every (re)connect — subscriptions die with the socket
  };
  ws.onmessage = (ev) => {
    let msg; try { msg = JSON.parse(ev.data); } catch { return; }
    const p = pending.get(msg.id);
    if (!p) {
      // **A server-push notification, not a reply** (kaspa.stream-style realtime): the node's
      // wRPC pushes subscribed events as id-less frames. Dispatch by method name; anything we
      // did not subscribe to (or a stray late reply) still falls through silently.
      const h = msg && msg.method && notifHandlers[msg.method];
      if (h) { try { h(msg.params || {}); } catch {} }
      return;
    }
    pending.delete(msg.id);
    // wRPC errors can arrive top-level ({error}) or inside params ({params:{error}})
    if (msg.error) return p.reject(new Error(msg.error.message || JSON.stringify(msg.error)));
    const body = msg.params || {};
    if (body && body.error) return p.reject(new Error(body.error.message || JSON.stringify(body.error)));
    p.resolve(body);
  };
  ws.onclose = () => { stopHeartbeat(); setConn(false); failAll("connection closed"); scheduleReconnect(); };
  ws.onerror = () => { try { ws.close(); } catch {} };
}
function failAll(reason){ pending.forEach(p => p.reject(new Error(reason))); pending.clear(); }
function scheduleReconnect(){ setTimeout(connect, reconnectDelay); reconnectDelay = Math.min(reconnectDelay*1.6, 10000); }

// Detect half-open sockets (TCP died with no close event): ping periodically and
// force a reconnect if a reply doesn't arrive — otherwise the page freezes silently.
function startHeartbeat(){
  stopHeartbeat();
  const sock = ws;
  heartbeatTimer = setInterval(async () => {
    if (ws !== sock || !ws || ws.readyState !== 1) return;
    try { await rpc("getInfo", {}, HEARTBEAT_TIMEOUT); }
    catch { if (ws === sock) { try { ws.close(); } catch {} } }
  }, HEARTBEAT_MS);
}
function stopHeartbeat(){ if (heartbeatTimer) { clearInterval(heartbeatTimer); heartbeatTimer = null; } }
function waitOpen(){ return (ws && ws.readyState===1) ? Promise.resolve() : new Promise(r=>openResolvers.push(r)); }

async function rpc(method, params = {}, timeout = 20000) {
  await waitOpen();
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const to = setTimeout(() => { pending.delete(id); reject(new Error(method + " timed out")); }, timeout);
    pending.set(id, { resolve: v => { clearTimeout(to); resolve(v); },
                      reject: e => { clearTimeout(to); reject(e); } });
    try { ws.send(JSON.stringify({ id, method, params })); }
    catch (e) { clearTimeout(to); pending.delete(id); reject(e); }
  });
}

// ---- push subscriptions (realtime page updates) --------------------------------------------
// The node pushes subscribed events as id-less frames (see ws.onmessage). One BlockAdded
// subscription serves every page: each render registers interest via onBlockAdded(), and the
// shared handler fans out. Polling stays as the fallback for nodes/proxies that drop
// notifications — pages keep their armPoll, just at a longer interval when pushes are flowing.
const notifHandlers = {};
let pushesSeen = 0;              // >0 ⇒ the push path is actually delivering on this connection
let blockAddedFns = [];          // current page's listeners (cleared on route change)
function onBlockAdded(fn){ blockAddedFns.push(fn); }
window.addEventListener("hashchange", () => { blockAddedFns = []; });
function handleBlockAdded(params){
  pushesSeen++;
  const blk = params && (params.block || params);   // shape observed live: {block:{header,transactions,verboseData}}
  for (const fn of blockAddedFns) { try { fn(blk); } catch {} }
}
async function armSubscriptions(){
  try {
    await rpc("subscribe", { BlockAdded: {} }, 8000);
    // Frame method names observed from this node are lowerCamel ("blockAdded"); register a few
    // plausible spellings so a server-side rename does not silently kill the realtime path.
    for (const name of ["blockAdded", "BlockAdded", "blockAddedNotification"]) notifHandlers[name] = handleBlockAdded;
  } catch {}
}

// One-shot query against a NAMED node over a short-lived socket, so a view can read a vantage
// other than the fast local one. `seedRpc` and `hubRpc` are the two that exist.
function pathRpc(path, method, params = {}, timeout = 12000) {
  return new Promise((resolve, reject) => {
    let s;
    try { s = new WebSocket((location.protocol === "https:" ? "wss://" : "ws://") + location.host + path); }
    catch (e) { return reject(e); }
    const done = (fn, arg) => { clearTimeout(to); try { s.close(); } catch {} fn(arg); };
    const to = setTimeout(() => done(reject, new Error(method + " timed out")), timeout);
    s.onopen = () => { try { s.send(JSON.stringify({ id: 1, method, params })); } catch (e) { done(reject, e); } };
    s.onmessage = (ev) => {
      let msg; try { msg = JSON.parse(ev.data); } catch { return done(reject, new Error("bad reply")); }
      if (msg.error) return done(reject, new Error(msg.error.message || JSON.stringify(msg.error)));
      const body = msg.params || {};
      if (body && body.error) return done(reject, new Error(body.error.message || JSON.stringify(body.error)));
      done(resolve, body);
    };
    s.onerror = () => done(reject, new Error(path + " connection failed"));
  });
}
function seedRpc(method, params = {}, timeout = 12000){ return pathRpc(WS_PATH_SEED, method, params, timeout); }
function hubRpc (method, params = {}, timeout = 12000){ return pathRpc(WS_PATH_HUB,  method, params, timeout); }

/* ------------------------------- helpers ------------------------------- */
const $ = (s, r=document) => r.querySelector(s);
const view = () => document.getElementById("view");
function esc(s){ return String(s==null?"":s).replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;"}[c])); }
function short(h, n=10){ if(!h) return "—"; return h.length>2*n ? `${h.slice(0,n)}…${h.slice(-6)}` : h; }
function num(n){ return (n==null||n==="") ? "—" : Number(n).toLocaleString("en-US"); }
function coin(sompi){ if(sompi==null) return "—"; const v=Number(sompi)/10**DECIMALS;
  return v.toLocaleString("en-US",{maximumFractionDigits:DECIMALS}); }
function ago(ms){ if(!ms) return "—"; const s=Math.max(0,(Date.now()-Number(ms))/1000);
  if(s<60)return Math.floor(s)+"s ago"; if(s<3600)return Math.floor(s/60)+"m ago";
  if(s<86400)return Math.floor(s/3600)+"h ago"; return Math.floor(s/86400)+"d ago"; }
function dt(ms){ if(!ms) return "—"; return new Date(Number(ms)).toISOString().replace("T"," ").replace(/\.\d+Z$/,"Z"); }
function linkBlock(h){ return `<a class="hash" href="#/block/${esc(h)}">${esc(short(h))}</a>`; }
// **The words a person typed, out of the chat template they were committed in.** A free-prompt job
// commits the whole templated text (`<|im_start|>user … <|im_end|>`); the table shows the human half
// and keeps the committed form in the tooltip. Attempt-lane prompts are derived from the block's
// anchor — token salad by design — and are shown dimmed, as a ticket rather than a question.
function llmUserTurn(text){
  if (text == null) return null;
  const t = String(text);
  const m = t.match(/<\|im_start\|>user\s*\n?([\s\S]*?)<\|im_end\|>/);
  if (m) return m[1].trim();
  return t.replace(/<\|im_start\|>(system|assistant|user)\s*\n?/g, "").replace(/<\|im_end\|>|<\|endoftext\|>/g, "").trim();
}
// **The answer up to the end of its turn.** The lane decodes to its budget whatever the answer's
// length, so what follows the first end-of-turn marker is the model continuing on its own. The row
// shows the answer and says how much more the claim carried; the full text is one click away.
function llmAnswerCut(text){
  if (text == null) return { shown: null, extra: 0 };
  const t = String(text);
  const i = t.search(/<\|im_end\|>|<\|endoftext\|>/);
  if (i < 0) return { shown: t.trim(), extra: 0 };
  return { shown: t.slice(0, i).trim(), extra: t.slice(i).replace(/<\|im_end\|>|<\|endoftext\|>/g, "").trim().length };
}
// A cell that clamps to a few lines and opens to the whole text. `<details>` inside a table cell
// needs no script and keeps the row's height honest while it is closed.
function llmIoCell(shown, full, cls, tail, clip=220){
  if (shown == null || shown === "") return null;
  const body = esc(shown);
  const long = shown.length > clip || (full && full !== shown);
  if (!long) return `<span class="llm-io ${cls}">${body}</span>${tail||""}`;
  return `<details class="llm-d"><summary><span class="llm-io ${cls}">${esc(shown.slice(0, clip))}${shown.length>clip?"…":""}</span>${tail||""}</summary><div class="llm-full ${cls}">${esc(full||shown)}</div></details>`;
}
function llmInCell(text, ids, anchorDerived){
  if (text == null) return `<span class="dim">decoding…</span>`;
  if (anchorDerived){
    const t = String(text);
    return `<details class="llm-d"><summary><span class="dim" title="attempt lane: this prompt is derived from the block's anchor — a lottery ticket nobody chose">🎲 anchor-derived · ${esc(t.slice(0,48))}${t.length>48?"…":""}</span></summary><div class="llm-full dim">${esc(t)}</div></details>`;
  }
  const turn = llmUserTurn(text);
  return llmIoCell(turn, String(text), "", "") || `<span class="dim">(empty prompt)</span>`;
}
// **What the page says when the chain committed something and nobody has demanded it.**
//
// A claim puts roots on chain; the bytes behind them live with the executor and reach the five
// drawn seats over an authenticated pull. That is the rule (ADR-0077 Decision 16), and since
// 2026-09-06 this page follows it instead of reading the executor's retention over its shoulder.
// A piece becomes public exactly when somebody DEMANDS it — a data-availability accusation the
// executor answers on chain (ADR-0062) — and the demand costs the accuser if the answer comes.
// **What the page says about a prompt, on a network that publishes prompts.**
//
// This site does not render the words — that is a choice, and it is the only part of this a site
// can decide. What it must not do is call them private. testnet-12 arms both prompt modes
// (`Params::palw_panel_da` is Some from DAA 0, which its held classes require): under mode 1
// (PublicDa) the commitment transaction carries the prompt's token ids WHOLE — they sit in the
// payload of the tx that carries the claim, permanently, for anyone with a node; under mode 2
// (PanelDa) it carries a digest and the ids reach the drawn seats over the authenticated pull.
// This page does not decode which mode a claim used.
//
// So the honest sentence is "committed, not shown here", and the tooltip says what each mode
// publishes. A reader who wants their prompt unread should learn that from this page and not from
// a stranger quoting it back at them.
function llmPromptOnChain(extra){
  const title = "testnet-12 arms both prompt modes. Mode 1 (PublicDa): the prompt's token ids are carried WHOLE in the "
    + "payload of the commitment transaction, so the prompt is public on chain and permanent. Mode 2 (PanelDa): the "
    + "commitment carries only a digest, and the ids reach the five drawn seats over an authenticated pull. This site "
    + "chooses not to render either; that is not privacy. Even under mode 2 a data-availability court close carries the "
    + "ids, so a disputed prompt becomes public: private unless disputed, never confidential.";
  return `<span class="dim" title="${esc(title)}">\u{1F310} prompt — <b>committed</b>, not shown here${extra||""}</span>`;
}
function llmSealed(what, commitment, extra){
  const title = `${what} is committed on chain and not published: the bytes reach the claim's five drawn seats over an `
    + `authenticated pull. A data-availability accusation is what makes a piece of it public, and it costs the accuser `
    + `if the executor answers (ADR-0062).`;
  return `<span class="dim" title="${esc(title)}">🔒 ${esc(what)} — committed, not disclosed${commitment?` <span class="hash">${esc(commitment)}</span>`:""}</span>${extra||""}`;
}
// A disclosure that HAS been demanded and answered: public because the chain carries it.
function llmDisclosedCell(list){
  if (!list || !list.length) return null;
  const body = list.map(d => `<div class="llm-full">${esc(typeof d === "string" ? d : JSON.stringify(d))}</div>`).join("");
  return `<details class="llm-d" open><summary><span class="pill blue" title="a data-availability accusation demanded this event and the executor disclosed it on chain (ADR-0062)">disclosed on demand</span></summary>${body}</details>`;
}
function llmOutCell(text, ids){
  if (text == null) return llmSealed("output", "", "");
  const cut = llmAnswerCut(text);
  const tail = cut.extra ? `<span class="llm-more" title="the lane decodes to its budget; this is what the claim carried after the end-of-turn marker">+${num(cut.extra)} chars after end-of-turn</span>` : "";
  return llmIoCell(cut.shown, String(text), "out", tail) || `<span class="dim">(empty answer)</span>`;
}

// kaspa-pq Layer-0 PoW lane (ADR-0039). 1/2/3 are hash lanes; 4 and above are PALW, where a
// candidate is won by running a pinned model rather than by hashing. Older nodes omit powAlgoId
// entirely, so an absent value is reported as unknown rather than assumed to be the hash lane.
//
// 6 is the attempt lane this network's PALW blocks carry, and it was once missing here, so every block
// rendered as a bare "algo-6" with no name — the one field that says this is not a hash chain.
function powLaneLabel(algo){
  var a = (algo === undefined || algo === null || algo === "") ? null : Number(algo);
  if (a === 10) return { cls: "chain", text: "algo-10 \u00b7 PALW execution round" };
  if (a === 9) return { cls: "chain", text: "algo-9 \u00b7 PALW execution" };
  if (a === 8) return { cls: "", text: "algo-8 \u00b7 heartbeat (no model)" };
  if (a === 7) return { cls: "chain", text: "algo-7 \u00b7 PALW receipt v3" };
  if (a === 6) return { cls: "chain", text: "algo-6 \u00b7 PALW committed v2" };
  if (a === 5) return { cls: "chain", text: "algo-5 \u00b7 PALW (ollama lane)" };
  if (a === 4) return { cls: "chain", text: "algo-4 \u00b7 PALW (audited compute)" };
  if (a === 3) return { cls: "", text: "algo-3 \u00b7 hash floor" };
  if (a === 2) return { cls: "", text: "algo-2 \u00b7 argon2id" };
  if (a === 1) return { cls: "", text: "algo-1 \u00b7 kheavyhash" };
  if (a === null) return { cls: "", text: "unknown (node does not report powAlgoId)" };
  return { cls: "", text: "algo-" + a };
}
// Whether this lane is won by inference rather than by hashing — the question every hash-shaped
// statistic on this page depends on.
function isPalwLane(algo){
  var a = (algo === undefined || algo === null || algo === "") ? null : Number(algo);
  return a !== null && a >= 4;
}
function powLaneCell(algo){
  var l = powLaneLabel(algo);
  return '<span class="pill ' + l.cls + '">' + esc(l.text) + '</span>';
}
function powLaneBadge(algo){
  var a = (algo === undefined || algo === null || algo === "") ? null : Number(algo);
  if (a === null) return "";
  return isPalwLane(a) ? '<span class="pill chain">algo-' + a + '</span>' : '<span class="pill">algo-' + a + '</span>';
}
function linkTx(h){ return `<a class="hash" href="#/tx/${esc(h)}">${esc(short(h))}</a>`; }
function linkAddr(a){ return `<a class="hash" href="#/address/${esc(a)}">${esc(a)}</a>`; }
function copyable(v){ return `<span class="hash">${esc(v)}</span>`; }
// EVM tx hashes are 32-byte (0x + 64 hex) — distinct from L1's 64-byte (128-hex) ids.
function isEvmTxHash(q){ return /^(0x)?[0-9a-f]{64}$/i.test(q); }
function linkEvmTx(h){ const x=String(h||"").replace(/^0x/i,"").toLowerCase(); return `<a class="hash" href="#/evmtx/${esc(x)}">${esc(short("0x"+x))}</a>`; }
// Classify a tx by its subnetwork id; coinbase also detected by having no inputs (fallback).
function txKind(t){
  const s = SUBNET[(t.subnetworkId||"").toLowerCase()];
  if (s) return s;
  if (!(t.inputs||[]).length) return SUBNET["0100000000000000000000000000000000000000"];
  return SUBNET["0000000000000000000000000000000000000000"];
}
function kindPill(k){ return `<span class="pill ${k.cls}">${esc(k.label)}</span>`; }

/* in-memory caches */
const txCache = new Map();    // txid -> { tx, blockHash }
let recent = [];              // newest-first block summaries
let recentLow = null;         // getBlocks anchor: a hash ~RECENT_WINDOW behind the tip
let lastSink  = null;
let cachedNet = null;         // network id the cached recent[] belongs to (guards cross-network stale cache)
const RECENT_WINDOW = RECENT_LIMIT + 10;
let shownHashes = new Set();  // block hashes already painted — flash only freshly-arrived rows
let homeBusy = false;         // re-entry guard so fast polling can't stack overlapping refreshes
function dropRecentCache(){ recent = []; recentLow = null; lastSink = null; shownHashes = new Set(); }

/* ---- network-stats hero band + latest-transactions feed (home top) ---- */
// Block rate (BPS) from a rolling window of (time, blockCount) samples — stable & cheap,
// reads the DAG block counter we already fetch each poll. Survives re-genesis (count resets
// → negative delta → ignored until the window refills).
let rateSamples = [];                 // {t, blocks}
const RATE_WINDOW_MS = 45000;
function pushRate(blockCount){
  const now = Date.now(), b = Number(blockCount);
  if (!Number.isFinite(b)) return;
  if (rateSamples.length && b < rateSamples[rateSamples.length-1].blocks) rateSamples = [];  // chain reset
  rateSamples.push({ t: now, blocks: b });
  while (rateSamples.length > 2 && now - rateSamples[0].t > RATE_WINDOW_MS) rateSamples.shift();
}
function bpsNow(){
  if (rateSamples.length < 2) return null;
  const a = rateSamples[0], z = rateSamples[rateSamples.length-1];
  const dt = (z.t - a.t)/1000, db = z.blocks - a.blocks;
  return (dt > 0 && db >= 0) ? db/dt : null;
}

// Current block reward (per-block coinbase subsidy, in sompi) — derived live from the coinbase
// emission in the recent window (see refreshTxFeedInner). NOTE: we do NOT use the REST
// /info/blockreward + /info/halving endpoints: those read Kaspa's stock deflationary table
// (500→440 MSK), which does not match Misaka's real ~2.3 MSK/block emission. Deriving from the
// actual coinbase keeps the figure honest for this chain.
let blockSubsidy = null;              // sompi; min coinbase-output-sum over the recent window

// Latest-transactions feed: derived node-direct from getBlocks(includeTransactions:true) so it
// is always live (independent of the Postgres tx index, which can lag). Also yields the recent
// miner-payout set (distinct largest-coinbase-output addresses) and the live TPS estimate.
let txFeed = [];                      // newest-first: {txid, kind, addr, value, ins, outs, block, ts, coinbase}
let txKnown = new Set();              // txids already in txFeed
let txShown = new Set();              // txids already painted (flash only new arrivals)
let txFeedBusy = false;
const TX_FEED_LIMIT = 25;
const TX_FEED_REFRESH_MS = 4000;
let recentMiners = 0;                 // distinct coinbase-payout addresses in the window
let tpsEma = null;                    // smoothed non-coinbase tx/s over the recent window
let txFeedTs = 0;
function dropTxFeed(){ txFeed = []; txKnown = new Set(); txShown = new Set(); recentMiners = 0; tpsEma = null; blockSubsidy = null; }

// formatting helpers for the hero band
function fmtHashrate(hs){
  if (hs == null || !Number.isFinite(hs)) return "—";
  const u = ["H/s","kH/s","MH/s","GH/s","TH/s","PH/s","EH/s"]; let i = 0, v = hs;
  while (v >= 1000 && i < u.length-1){ v /= 1000; i++; }
  return v.toLocaleString("en-US",{maximumFractionDigits: v<10?2:(v<100?1:0)}) + " " + u[i];
}
function fmtCompact(v){
  if (v == null || !Number.isFinite(v)) return "—";
  const a = Math.abs(v);
  if (a >= 1e12) return (v/1e12).toLocaleString("en-US",{maximumFractionDigits:2})+"T";
  if (a >= 1e9)  return (v/1e9 ).toLocaleString("en-US",{maximumFractionDigits:2})+"B";
  if (a >= 1e6)  return (v/1e6 ).toLocaleString("en-US",{maximumFractionDigits:2})+"M";
  if (a >= 1e3)  return (v/1e3 ).toLocaleString("en-US",{maximumFractionDigits:1})+"K";
  return v.toLocaleString("en-US",{maximumFractionDigits:0});
}

// Persist the recent-blocks window so a page reload paints instantly from the last session
// instead of staring at "Loading blocks…" while the cold walk runs.
const LS_KEY = "msk_recent_v2";   // v2: rows carry algo + classId
function saveCache(){ try { localStorage.setItem(LS_KEY, JSON.stringify({ net: cachedNet, recentLow, lastSink, recent })); } catch {} }
function loadCache(){
  try {
    const c = JSON.parse(localStorage.getItem(LS_KEY) || "null");
    if (c && Array.isArray(c.recent) && c.recent.length){
      recent = c.recent; recentLow = c.recentLow; lastSink = c.lastSink; cachedNet = c.net; return true;
    }
  } catch {}
  return false;
}

function summarize(blk){
  const h = blk.header, v = blk.verboseData || {};
  return { hash: h.hash, daaScore: h.daaScore, blueScore: h.blueScore, timestamp: h.timestamp,
           nParents: (h.parentsByLevel && h.parentsByLevel[0] ? h.parentsByLevel[0].length : 0),
           nTx: (v.transactionIds ? v.transactionIds.length : 0),
           isChain: !!v.isChainBlock, selectedParent: v.selectedParentHash,
           // What mined it, read off the header itself (the block page's own decoder): the lane,
           // and for a PALW block the execution class inside the PAV2 attempt envelope.
           algo: (h.powAlgoId === undefined || h.powAlgoId === null) ? null : Number(h.powAlgoId),
           classId: (h.palwCommitment && h.palwCommitment.length) ? ((llmDecodeAttempt(h.palwCommitment) || {}).classId || null) : null };
}
// The Recent blocks MODEL cell. A heartbeat block used no model at all (ADR-0066 D1) and says so;
// the floor class is not a model either; an unnamed class shows the chain's id rather than a guess.
function recentModelCell(b){
  const role = b.isChain ? "chain block (on the selected chain)" : "merged block (in the DAG, off the selected chain)";
  if (b.algo === 8)
    return `<span class="pill" title="heartbeat lane (algo-8): a plain hash lane, no model — ${role}">heartbeat · no model</span>`;
  if (b.classId){
    const c = LLM_CLASS_BY_ID[b.classId];
    if (llmIsFloor(b.classId))
      return `<span class="pill" title="PALW-BASE-0: the deterministic integer floor, no model file — ${role}">PALW-BASE-0 · floor</span>`;
    if (c) return `<span class="pill blue" title="${esc(c.model)} — ${role}">${esc(c.name)}</span>`;
    return `<span class="pill mono" title="class ${esc(b.classId)} — ${role}">class ${esc(short(b.classId,6))}</span>`;
  }
  if (b.algo === null || b.algo === undefined) return `<span class="dim" title="${role}">—</span>`;
  return `<span class="dim" title="${esc(powLaneLabel(b.algo).text)}: no attempt envelope in the header — ${role}">${esc(powLaneLabel(b.algo).text)}</span>`;
}


/* ------------------------------- faucet ------------------------------- */
async function renderFaucet(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › Faucet</div>
    <h1 class="page">Testnet Faucet <span class="dim" style="font-size:13px;font-weight:400">(testnet-12 — coins with no value, by design)</span></h1>
    <div class="note"><b>The faucet has no testnet-12 funding yet.</b> Funding it from the testnet-12 premine is the
      operator's decision, and until it is funded every request is refused — the status below says which.</div>
    <div class="note">One grant of <b><span id="fcGrant">0.5</span> ${SYMBOL}</b> per address, <b>ever</b> — and one
      request per source per day. A grant pays transaction fees and a fee float; it does <b>not</b> fund a PALW bond:
      testnet-12 runs the mainnet-assumed bonds — at least <b>13,000 ${SYMBOL}</b> for a producer and
      <b>130,000 ${SYMBOL}</b> for a panel seat. The faucet pays with a regular transaction, a <b>non-coinbase</b>
      output. Testnet coins are worthless and the chain can reset at any time.</div>
    <h2 class="sec">Status</h2>
    <div id="fcCards" class="cards"><div class="loading">Loading…</div></div>
    <h2 class="sec">Request funds</h2>
    <div class="note">
      <div style="display:flex;gap:8px;flex-wrap:wrap;align-items:stretch">
        <input id="fcAddr" placeholder="misakatest:…" spellcheck="false" autocomplete="off"
          style="flex:1;min-width:260px;background:var(--bg2);border:1px solid var(--line);border-radius:9px;color:var(--txt);padding:10px 12px;font:inherit"/>
        <button id="fcGo"
          style="background:var(--acc2);color:#0b0a12;border:0;border-radius:9px;padding:10px 18px;font:inherit;font-weight:650;cursor:pointer">Send me t${SYMBOL}</button>
      </div>
      <div id="fcOut" style="margin-top:10px"></div>
    </div>
    <h2 class="sec">Then what?</h2>
    <div class="note">Running a node needs no funds. Producing blocks needs a registered bond (above) — the floor
      execution class needs no model at all. The walkthrough is
      <a href="https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md" target="_blank" rel="noopener">docs/testnet12-join-mining.md</a>; the
      <a href="#/llm">LLM Jobs</a> page shows the classes and the verifier seats currently live.</div>`;

  const cards = document.getElementById("fcCards");
  try {
    const st = await fetch("/faucet/v1/status").then(r => r.json());
    if (st.grant_msk) document.getElementById("fcGrant").textContent = st.grant_msk;
    const bal = st.balance_msk == null ? "—" : Number(st.balance_msk).toLocaleString("en-US", {maximumFractionDigits: 2});
    cards.innerHTML = `
      <div class="card"><div class="k">Faucet balance</div><div class="v ${st.funded ? "acc" : ""}">${esc(bal)}</div><div class="sub">${st.funded ? SYMBOL + " ready to grant" : "not funded yet — requests will be refused until it is"}</div></div>
      <div class="card"><div class="k">Grant</div><div class="v">${esc(st.grant_msk || "—")}</div><div class="sub">${SYMBOL} per address, once ever</div></div>
      <div class="card"><div class="k">Addresses funded</div><div class="v">${num(st.granted)}</div><div class="sub">all time</div></div>
      <div class="card"><div class="k">Faucet address</div><div class="v sm" style="font-size:11px;word-break:break-all;line-height:1.5">${esc(st.address || "—")}</div><div class="sub">donate unused t${SYMBOL} back here</div></div>`;
  } catch (e) {
    cards.innerHTML = `<div class="card off"><div class="k">Faucet</div><div class="v sm">unreachable</div><div class="sub">${esc(String(e && e.message || e))}</div></div>`;
  }
  // claude-route-guard-v1: left during the status fetch — the form below is gone, so there is nothing to wire
  // (this used to throw "Cannot read properties of null (reading 'addEventListener')").
  if (__g !== routeGen) return;

  const out = document.getElementById("fcOut");
  const go  = document.getElementById("fcGo");
  const addrBox = document.getElementById("fcAddr");
  addrBox.addEventListener("keydown", e => { if (e.key === "Enter") go.click(); });
  go.onclick = async () => {
    const address = addrBox.value.trim();
    if (!/^misakatest:[a-z0-9]{100,140}$/.test(address)) {
      out.innerHTML = `<span style="color:#ff8f8f">That is not a testnet address — it starts with <code>misakatest:</code>.</span>`;
      return;
    }
    go.disabled = true; const label = go.textContent; go.textContent = "Sending…";
    out.innerHTML = `<span class="dim">Signing and broadcasting — this takes a few seconds…</span>`;
    try {
      const r = await fetch("/faucet/v1/claim", { method: "POST",
        headers: { "Content-Type": "application/json" }, body: JSON.stringify({ address }) });
      const d = await r.json().catch(() => ({}));
      if (r.ok && d.ok) {
        out.innerHTML = `<b style="color:var(--acc2)">Sent ${esc(d.amount_msk)} ${SYMBOL}.</b>` +
          (d.txid ? ` Transaction <a href="#/tx/${esc(d.txid)}"><code>${esc(d.txid.slice(0, 16))}…</code></a> — spendable as soon as it confirms.` : "");
      } else {
        out.innerHTML = `<span style="color:#ff8f8f">${esc(d.error || "request failed (" + r.status + ")")}</span>`;
      }
    } catch (e) {
      out.innerHTML = `<span style="color:#ff8f8f">Network error: ${esc(String(e && e.message || e))}</span>`;
    }
    go.disabled = false; go.textContent = label;
  };
}

/* ------------------------------- routing ------------------------------- */
// **claude-route-guard-v1 (2026-09-10): a page that comes back from an await must not paint over the
// page the user went to since.** Every render awaited the node and then wrote the whole #view. Press
// Home while a block page was still loading and Home painted — then the block page's last await
// returned (28 s later, measured on this node) and painted over Home, with the hash already "#/". A
// second press of Home did nothing: a hash router gets no hashchange for the hash it already has.
// So every route() is a generation; each render keeps the generation it started in and writes through
// viewFor(gen) — the live #view while it is current, a detached element once the user has moved on.
let routeGen = 0;
function viewFor(gen){ return gen === routeGen ? view() : document.createElement("div"); }
function showErrFor(gen, msg, ctx){ if (gen === routeGen) showErr(msg, ctx); }
/* ---------------- The model economy, as the chain holds it ----------------------------------
   Four consensus reads this page had no view of, added 2026-09-19. Each is a node op that
   post-dates the app.js this file patches, and each answers a question the explorer could only
   guess at before:

     getPalwModelRegistry   ADR-0135  which classes exist, what lifecycle state each is in, and
                                      how many panel seats are ready for it RIGHT NOW
     getPalwRoundLane       ADR-0125  the execution lane: this round's permits, the span's
                                      schedule, and the finals the next span is sized from
     getPalwModelLine       ADR-0088  a line's owner/developer/maintainer, its current version
                                      and the artifact roots in force — plus ADR-0101 service facts
     getPalwSettlement      ADR-0127  whether what the chain accepted at a DAA score is settled,
                                      counted in Final PALW anchors rather than in blocks

   **The readiness number is the point of the registry view.** Past ADR-0135's fence the chain
   records who is ready: `readySeatsNow` is recomputed at the tip, and `requiredReadySeats` is
   what the class needs (panel seat_count 5 + spare 2 = 7 for every live class). The LLM Jobs
   page renders that same count. The hand-kept roster there is deployment only — a seat that
   holds the file is not Ready until it files SeatReadinessProved.

   Everything rendered is a count, an identifier, a lifecycle state or a DAA score — the "shown"
   half of docs/explorer/README.md. No prompt and no answer passes through this file.
--------------------------------------------------------------------------------------------- */

// **An op the node may not know kills the WHOLE socket, not just the call** — a pre-ADR-0135
// node answers an unknown wRPC method by closing the connection, and every read after it on that
// page fails too. So each new op is tried ONCE per connection and the verdict remembered; a page
// that finds an op unsupported says so and renders what it does have.
const palwOpState = Object.create(null);   // op -> "ok" | "unsupported"
async function palwRead(op, params = {}, timeout = 12000) {
  if (palwOpState[op] === "unsupported") return { __unsupported: true };
  try {
    const v = await rpc(op, params, timeout);
    palwOpState[op] = "ok";
    return v;
  } catch (e) {
    // A timeout on a socket that is still open is a slow node, not a missing method. A closed
    // socket after an untried op is the signature the note above describes.
    if (palwOpState[op] !== "ok" && !(typeof ws === "object" && ws && ws.readyState === 1)) {
      palwOpState[op] = "unsupported";
      return { __unsupported: true };
    }
    return { __error: String(e && e.message ? e.message : e) };
  }
}
function palwUnsupportedNote(op) {
  return `<div class="card off"><b>${esc(op)}</b> is not served by the node this page is reading.
    That op ships with the registry build; a node older than it closes the connection rather than
    answering, so the page stops asking until it reconnects.</div>`;
}

// A lifecycle state, coloured by what it means for a miner pointing a worker at the class.
function stateBadge(s) {
  const t = String(s || "—");
  const good = /^Active/.test(t), warn = /Probation|Prefetch/.test(t), bad = /Held|Dormant/.test(t);
  const col = good ? "var(--ok,#4ade80)" : warn ? "var(--warn,#fbbf24)" : bad ? "var(--bad,#f87171)" : "var(--mut)";
  return `<span style="color:${col};font-weight:600">${esc(t)}</span>`;
}
// `short()`, `esc()` and `num()` are app.js's own helpers — redefining `short` here would
// silently change how every existing page prints a hash, so this file uses them as they are.


/* ---- The display layer: the name a reader came for, beside the id the chain signs -----------
   Display only. Nothing here enters `class_id`, `rules_hash` or any signature — the chain keeps
   answering in hashes and this looks the hash up. docs/explorer/user-dictionary.md is the contract.

   **A name carries the SHAPE, not just the model.** `class_id` hashes the whole
   `PalwShapeProfile` — n_ctx, n_batch, the runtime flags — so one artifact legitimately carries
   several class ids, and "@512" is part of what distinguishes them. Printing the bare model name
   would make two distinct consensus objects look like one.
--------------------------------------------------------------------------------------------- */
const MODEL_DISPLAY = {
  "PALW-BASE-0": { name: "Base Model",         sub: "Built-in deterministic model — nothing to download", size: null,     icon: "◆" },
  "QWEN36":      { name: "Qwen 3.6 35B",       sub: "Large reasoning model",                              size: "~34 GB", icon: "🤖" },
  "QWEN25-A16":  { name: "Qwen 2.5 1.5B @512", sub: "Lightweight model — chat and reasoning",             size: "~1.7 GB",icon: "🤖" },
  "QWEN25-A16-8K":{ name: "Qwen 2.5 1.5B @8k",  sub: "Held-context lightweight model — graph-v7@8192",    size: "~1.7 GB",icon: "🤖" },
  "QWEN25-A16-2M":{ name: "Qwen 2.5 1.5B @2M",  sub: "Held-context lightweight model — graph-v7@2097152 · closed at launch, takes no claims", size: "~2.7 GB",icon: "🤖" },
  "QWEN38-27B":  { name: "Qwen 3.8 27B",       sub: "Dense reasoning model",                              size: null,     icon: "🤖" },
};
function modelDisplay(classId) {
  const c = (typeof LLM_CLASS_BY_ID !== "undefined" && LLM_CLASS_BY_ID) ? LLM_CLASS_BY_ID[classId] : null;
  const d = c ? MODEL_DISPLAY[c.name] : null;
  if (d && c) return { ...d, code: c.name, spec: c.model, known: true };
  // **Degrade honestly.** The chain can report more classes than this table names:
  // a class registered permissionlessly has no display entry until somebody adds one. An unknown
  // model says so and shows its id — never a blank, and never a guess.
  return { name: null, sub: "This model has no name on the explorer yet.", size: null, icon: "？",
           code: c ? c.name : null, spec: c ? c.model : null, known: false };
}
// Simple is the default and Technical is an ADDITION to it — a researcher never switches back to
// see the human name. Remembered per reader; it changes only what is drawn, never what is read.
function techOn() { try { return localStorage.getItem("mskView") === "technical"; } catch { return false; } }
function setTech(on) { try { localStorage.setItem("mskView", on ? "technical" : "simple"); } catch {} }
function techToggle() {
  return `<label class="techtog" style="float:right;font-size:13px;color:var(--mut);cursor:pointer;user-select:none">
    <input type="checkbox" id="techTog" ${techOn() ? "checked" : ""} style="vertical-align:-1px"> Technical details</label>`;
}
function wireTech(rerender) {
  const t = document.getElementById("techTog");
  if (t) t.addEventListener("change", () => { setTech(t.checked); rerender(); });
}

async function renderRegistry() {
  const __g = routeGen;
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Models</div>
    <h1 class="page">Models</h1><div class="loading">Loading…</div>`;

  const paint = async () => {
    const reg = await palwRead("getPalwModelRegistry");
    if (curSeg() !== "registry") return;
    if (reg.__unsupported) { viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Models</div>
      <h1 class="page">Models</h1>${palwUnsupportedNote("getPalwModelRegistry")}`; return; }
    if (reg.__error) return showErrFor(__g, "getPalwModelRegistry failed: " + reg.__error);
    const r = reg.registry || reg;
    const cls = r.classes || [];

    // **"Armed" and "in force" are two different questions and the page asks both.** A registry
    // that is scheduled but not yet crossed reports every class as Legacy with no row, which is
    // the truthful answer and reads like an empty registry unless the fence is shown beside it.
    const gate = [
      ["Fence (DAA)", r.fenceDaa == null ? "—" : num(r.fenceDaa)],
      ["In force", r.active ? `<span style="color:var(--ok,#4ade80)">yes</span>` :
        `<span style="color:var(--mut)">not yet — the chain is at DAA ${num(r.tipDaa)}</span>`],
      ["Served by this node", r.available ? "yes" : "no"],
      ["Panel seats", num(r.seatCount)],
      ["Bonds active / with headroom", `${num(r.bondsActive)} / ${num(r.bondsWithHeadroom)}`],
      ["Grace until (DAA)", r.graceUntilDaa == null ? "—" : num(r.graceUntilDaa)],
    ];
    const counts = [
      ["Registered", r.classesRegistered], ["Active", r.classesActive],
      ["Active (limited)", r.classesActiveLimited], ["Probation", r.classesProbation],
      ["Prefetching", r.classesPrefetching], ["Held", r.classesHeld],
    ].filter(([, v]) => v != null);

    // **Cards first, table behind Technical.** A reader came for "which models can I use and are
    // they working"; the ids are what the chain signs and stay one click away, never removed.
    const live = cls.filter(c => /^Active/.test(String(c.state || "")));
    const cards = cls.map(c => {
      const id = String(c.classId || "");
      const d = modelDisplay(id);
      const ok = /^Active/.test(String(c.state || ""));
      const ready = c.readySeatsNow, need = c.requiredReadySeats;
      const shortOf = ready != null && need > 0 && ready < need;
      return `<a class="card" href="#/line/${encodeURIComponent(id)}" style="display:block;text-align:left">
        <b style="font-size:15px">${d.icon} ${d.name ? esc(d.name) : `<span class="mono">${esc(short(id, 8))}</span>`}</b>
        <span style="display:block;color:var(--mut);margin:2px 0 8px">${esc(d.sub)}</span>
        <span style="display:block">${stateBadge(c.state)}${c.isBaseClass ? ` <span style="color:var(--mut)">· built in</span>` : ""}</span>
        ${c.isBaseClass
          ? `<span style="display:block;margin-top:6px;color:var(--mut)" title="every node verifies the floor by construction; it never files SeatReadinessProved and is not readiness-gated">Checkers: every node · not gated</span>`
          : `<span style="display:block;margin-top:6px${shortOf ? ";color:var(--bad,#f87171)" : ""}">Checkers ready
          <b>${ready == null ? "—" : num(ready)}</b>${need ? ` of ${num(need)}` : ""}</span>`}
        ${d.size ? `<span style="display:block;color:var(--mut)">Model size ${esc(d.size)}</span>` : ""}
        <span style="display:block;margin-top:6px;color:var(--mut)" class="mono">${esc(short(id, 8))}</span>
      </a>`;
    }).join("");

    const rows = cls.map(c => {
      const id = String(c.classId || "");
      const d = modelDisplay(id);
      const link = `<a href="#/line/${encodeURIComponent(id)}" class="mono">${esc(short(id, 10))}</a>`;
      const ready = c.readySeatsNow == null ? "—" : num(c.readySeatsNow);
      const need = c.requiredReadySeats == null ? "—" : num(c.requiredReadySeats);
      const short_ = c.readySeatsNow != null && c.requiredReadySeats > 0 && c.readySeatsNow < c.requiredReadySeats;
      return `<tr>
        <td>${d.name ? esc(d.name) : `<span style="color:var(--mut)">unnamed</span>`}${d.code ? ` <span style="color:var(--mut)">${esc(d.code)}</span>` : ""}</td>
        <td>${link}</td>
        <td>${stateBadge(c.state)}</td>
        <td style="text-align:right${short_ && !c.isBaseClass ? ";color:var(--bad,#f87171)" : ""}">${c.isBaseClass ? `<span style="color:var(--mut)" title="the floor is not readiness-gated">not gated</span>` : `${ready} / ${need}`}</td>
        <td style="text-align:right">${c.probesPassed == null ? "—" : num(c.probesPassed)}</td>
        <td class="mono" style="color:var(--mut)">${esc(short(c.artifactRoot, 8))}</td>
        <td style="color:var(--mut)">${esc(c.reason || "")}</td></tr>`;
    }).join("");

    const tech = techOn() ? `
      <h2 class="sec">Registry gate <span style="font-size:13px;color:var(--mut)">ADR-0135 · getPalwModelRegistry</span></h2>
      <div class="kv">${gate.map(x => `<div class="row"><div class="key">${x[0]}</div><div class="val">${x[1]}</div></div>`).join("")}</div>
      ${counts.length ? `<h2 class="sec">Classes by lifecycle state</h2>
        <div class="cards">${counts.map(([k, v]) => `<div class="card"><b>${num(v)}</b><span>${esc(k)}</span></div>`).join("")}</div>` : ""}
      <h2 class="sec">Execution classes (${cls.length})</h2>
      <p style="color:var(--mut);margin:0 0 8px">A class is a model at ONE runtime shape — <span class="mono">class_id</span>
        hashes the whole profile, so the same weights at a different context size is a different
        class. <b>Ready / required</b> is the chain's own count of panel seats holding the artifact
        now; below the requirement no claim of that class can be licensed.</p>
      <div class="tblscroll"><table class="tbl">
        <thead><tr><th>Model</th><th>class_id</th><th>State</th><th style="text-align:right">Ready / required</th>
        <th style="text-align:right">Probes</th><th>artifact_root</th><th>Reason</th></tr></thead>
        <tbody>${rows || `<tr><td colspan="7" style="color:var(--mut)">no classes</td></tr>`}</tbody>
      </table></div>` : "";

    viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Models</div>
      <h1 class="page">Models ${techToggle()}</h1>
      <p style="color:var(--mut);margin:0 0 10px">AI models running on the MISAKA network.
        <b>${num(cls.length)}</b> model${cls.length === 1 ? "" : "s"} ·
        <b>${num(live.length)}</b> actively producing${r.active ? "" :
        ` · <span title="The registry's rules start applying at this height.">the registry starts at
          network progress ${num(r.fenceDaa)} and the chain is at ${num(r.tipDaa)}</span>`}</p>
      <div class="cards">${cards || `<div class="card off">no models</div>`}</div>
      ${tech}
      <div id="settleBox"></div>`;
    wireTech(paint);

    // ADR-0127: settlement is counted in Final PALW anchors, never in blocks — so a depth of 3
    // here is three settled anchors, which is a much stronger statement than three confirmations.
    const s = await palwRead("getPalwSettlement", { daaScore: Number(r.tipDaa || 0) });
    const box = document.getElementById("settleBox");
    if (!box || curSeg() !== "registry") return;
    if (s.__unsupported) { box.innerHTML = `<h2 class="sec">Settlement</h2>${palwUnsupportedNote("getPalwSettlement")}`; return; }
    if (s.__error) { box.innerHTML = `<h2 class="sec">Settlement</h2><div class="card off">getPalwSettlement failed: ${esc(s.__error)}</div>`; return; }
    const sr = [
      ["At DAA", num(s.daaScore)],
      ["Settled", s.settled ? `<span style="color:var(--ok,#4ade80)">yes</span>` : `<span style="color:var(--mut)">not yet</span>`],
      ["Depth (settled anchors)", `${num(s.depth)}${s.depthIsLowerBound ? " (lower bound)" : ""}`],
      ["Anchors pending", num(s.pendingAnchors)],
      ["Safe frontier (DAA)", num(s.safeFrontierDaa)],
      ["Sink (DAA)", num(s.sinkDaa)],
    ];
    box.innerHTML = `<h2 class="sec">Settlement <span style="font-size:13px;color:var(--mut)">ADR-0127</span></h2>
      <p style="color:var(--mut);margin:0 0 8px">Depth is counted in <b>settled PALW anchors</b> —
        blocks whose claim reached Final — never in blocks. Execution and heartbeat blocks add no
        anchor, however many of them there are.</p>
      <div class="kv">${sr.map(x => `<div class="row"><div class="key">${x[0]}</div><div class="val">${x[1]}</div></div>`).join("")}</div>`;
  };

  await paint();
  armPoll(() => curSeg() === "registry", paint, 30000);
}

async function renderLane() {
  const __g = routeGen;
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Execution lane</div>
    <h1 class="page">Execution lane</h1><div class="loading">Loading…</div>`;

  const paint = async () => {
    const l = await palwRead("getPalwRoundLane");
    if (curSeg() !== "lane") return;
    if (l.__unsupported) { viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Execution lane</div>
      <h1 class="page">Execution lane</h1>${palwUnsupportedNote("getPalwRoundLane")}`; return; }
    if (l.__error) return showErrFor(__g, "getPalwRoundLane failed: " + l.__error);

    const head = [
      ["Armed", l.armed ? "yes" : `<span style="color:var(--mut)">no</span>`],
      ["Open", l.open ? `<span style="color:var(--ok,#4ade80)">yes</span>` : `<span style="color:var(--mut)">closed</span>`],
      ["Virtual DAA", num(l.virtualDaa)],
      ["Round", num(l.round)],
      ["Span", `${num(l.span)} <span style="color:var(--mut)">(${num(l.scheduleSpanDaa)} DAA)</span>`],
      ["Permits per round", num(l.permitsPerRound)],
      ["Max per mergeset", num(l.maxPerMergeset)],
      ["Accepted in span", num(l.acceptedInSpan)],
      [`Finals (span ${num(l.finalsSpan)})`, num(l.finals)],
      ["Permits next round", num(l.nextRoundPermits)],
    ];
    const permits = (l.permits || []).map(p => `<tr>
        <td style="text-align:right">${num(p.index)}</td>
        <td class="mono">${esc(short(p.domain, 8))}</td>
        <td class="mono" style="color:var(--mut)">${esc(short(p.operatorId, 8))}</td>
        <td>${p.used ? "used" : `<span style="color:var(--ok,#4ade80)">free</span>`}</td></tr>`).join("");
    // **The quota is why a second class matters.** One domain at 1000‰ means the whole round is
    // one class's; the lane only widens when another domain has finals to be credited for.
    const domains = (l.domains || []).map(d => `<tr>
        <td class="mono"><a href="#/line/${encodeURIComponent(String(d.domain || ""))}">${esc(short(d.domain, 10))}</a></td>
        <td style="text-align:right">${num(d.credits)}</td>
        <td style="text-align:right">${d.quotaPermille == null ? "—" : (Number(d.quotaPermille) / 10).toFixed(1) + " %"}</td>
        <td style="text-align:right">${num(d.bonds)}</td></tr>`).join("");

    viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Execution lane</div>
      <h1 class="page">Execution lane <span style="font-size:14px;color:var(--mut)">ADR-0125</span></h1>
      <div class="kv">${head.map(x => `<div class="row"><div class="key">${x[0]}</div><div class="val">${x[1]}</div></div>`).join("")}</div>
      <h2 class="sec">This round's permits (${(l.permits || []).length})</h2>
      <p style="color:var(--mut);margin:0 0 8px">A permit is one seat in the round, drawn per
        operator. The span two ahead is sized from the finals counted now, so an empty lane stays
        narrow until somebody finishes work in it.</p>
      <div class="tblscroll"><table class="tbl">
        <thead><tr><th style="text-align:right">#</th><th>Domain (class)</th><th>Operator</th><th>State</th></tr></thead>
        <tbody>${permits || `<tr><td colspan="4" style="color:var(--mut)">no permits this round</td></tr>`}</tbody>
      </table></div>
      <h2 class="sec">Domains (${(l.domains || []).length})</h2>
      <div class="tblscroll"><table class="tbl">
        <thead><tr><th>Domain</th><th style="text-align:right">Credits</th><th style="text-align:right">Quota</th><th style="text-align:right">Bonds</th></tr></thead>
        <tbody>${domains || `<tr><td colspan="4" style="color:var(--mut)">no domain holds credits</td></tr>`}</tbody>
      </table></div>`;
  };

  await paint();
  armPoll(() => curSeg() === "lane", paint, 20000);
}

async function renderModelLine(lineId) {
  const __g = routeGen;
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  const id = String(lineId || "");
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › <a href="#/registry">Models</a> › Line</div>
    <h1 class="page">Model line</h1><div class="loading">Loading…</div>`;
  if (!/^[0-9a-f]{16,}$/i.test(id)) return showErrFor(__g, "A line id is hex — a class id names its own founding line.");

  const m = await palwRead("getPalwModelLine", { lineId: id });
  if (curSeg() !== "line") return;
  if (m.__unsupported) { viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › <a href="#/registry">Models</a> › Line</div>
    <h1 class="page">Model line</h1>${palwUnsupportedNote("getPalwModelLine")}`; return; }
  if (m.__error) return showErrFor(__g, "getPalwModelLine failed: " + m.__error);
  if (m.exists === false) return showErrFor(__g, "The chain holds no line with that id.");

  const ln = m.line || {};
  // **A bond is an outpoint, not a string.** `owner` arrives as `{transactionId, index}`; printing
  // it straight gives "[object Object]", which is what the first cut of this page did.
  const bond = (b) => {
    if (!b) return null;
    if (typeof b === "string") return esc(short(b, 12));
    if (b.transactionId != null) return `${esc(short(b.transactionId, 10))}<span style="color:var(--mut)">:${esc(String(b.index ?? 0))}</span>`;
    return esc(short(JSON.stringify(b), 12));
  };
  // ADR-0088: a role left unset IS the owner — the row stores null rather than repeating it, so a
  // dash here would read as "nobody holds this role" when the truth is "the owner does".
  const role = (v) => bond(v) || `<span style="color:var(--mut)">the owner</span>`;
  const rows = [
    ["Line id", `<span class="mono">${esc(short(id, 16))}</span>`],
    ["Name", ln.name ? esc(ln.name) : `<span style="color:var(--mut)">unnamed</span>`],
    ["Class", `<span class="mono">${esc(short(ln.classId, 10))}</span>`],
    ["Status", esc(String(ln.status ?? "—"))],
    ["Founded (DAA)", ln.foundedDaa == null ? "—" : num(ln.foundedDaa)],
    ["Owner", `<span class="mono">${bond(ln.owner) || "—"}</span>`],
    ["Developer", `<span class="mono">${role(ln.developer)}</span>`],
    ["Maintainer", `<span class="mono">${role(ln.maintainer)}</span>`],
    ["Current version", ln.current == null ? "—" : num(ln.current)],
    ["Versions published", ln.versionsPublished == null ? "—" : num(ln.versionsPublished)],
    ["Previews open", num((ln.previews || []).length)],
    ["Current root", `<span class="mono">${esc(short(m.currentRoot, 12))}</span>`],
    ["Roots in force", num((m.rootsInForce || []).length)],
    ["Retired", ln.retiredDaa ? `at DAA ${num(ln.retiredDaa)}` : "no"],
    ["At DAA", num(m.tipDaa)],
  ];

  // ADR-0101: what a provider may truthfully say it serves. The roots here are the ones this line
  // OWNS and are in force — not "the class's root plus every version", which would let a copy line
  // advertise somebody else's artifact.
  const sf = m.serviceFacts || null;
  const sfBox = sf ? `<h2 class="sec">Service facts <span style="font-size:13px;color:var(--mut)">ADR-0101</span></h2>
      <div class="kv">${Object.entries(sf).slice(0, 10).map(([k, v]) =>
    `<div class="row"><div class="key">${esc(k)}</div><div class="val mono">${esc(
      Array.isArray(v) ? v.map(x => short(String(x), 8)).join(", ") : short(String(v), 20))}</div></div>`).join("")}</div>`
    : `<h2 class="sec">Service facts</h2><div class="card off">This node returned no service facts for the line
        (they answer from response version 3 of <span class="mono">getPalwModelLine</span>).</div>`;

  const bn = m.benefits || null;
  const bnBox = bn && Object.keys(bn).length
    ? `<h2 class="sec">Declared benefits <span style="font-size:13px;color:var(--mut)">ADR-0095</span></h2>
       <div class="kv">${Object.entries(bn).slice(0, 10).map(([k, v]) =>
      `<div class="row"><div class="key">${esc(k)}</div><div class="val">${esc(String(v)).slice(0, 80)}</div></div>`).join("")}</div>`
    : `<h2 class="sec">Declared benefits</h2><div class="card off">This line declares none. A position in a
        line that declares no benefit grants nothing to hold — which is why misakaoptions offers
        "buy access" only where a declaration exists.</div>`;

  const roots = (m.rootsInForce || []).map(r => `<tr><td class="mono">${esc(short(r, 16))}</td></tr>`).join("");
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › <a href="#/registry">Models</a> › Line ${esc(short(id, 8))}</div>
    <h1 class="page">Model line <span style="font-size:14px;color:var(--mut)">ADR-0088</span></h1>
    <div class="kv">${rows.map(x => `<div class="row"><div class="key">${x[0]}</div><div class="val">${x[1]}</div></div>`).join("")}</div>
    ${sfBox}
    ${bnBox}
    <h2 class="sec">Artifact roots in force (${(m.rootsInForce || []).length})</h2>
    <div class="tblscroll"><table class="tbl"><thead><tr><th>Root</th></tr></thead>
      <tbody>${roots || `<tr><td style="color:var(--mut)">none in force</td></tr>`}</tbody></table></div>`;
}

function route(){
  routeGen++;
  blockAddedFns = [];   // a re-render of the same hash gets no hashchange, so the listener reset lives here too
  const h = location.hash.replace(/^#\/?/, "");
  const [path, ...rest] = h.split("/");
  const arg = rest.join("/");
  if (!path)                 return renderHome();
  if (path === "block")      return renderBlock(decodeURIComponent(arg));
  if (path === "tx")         return renderTx(decodeURIComponent(arg));
  if (path === "address")    return renderAddress(decodeURIComponent(arg));
  if (path === "peers")      return renderPeers();
  if (path === "blockdag" || path === "dag" || path === "palw" || path === "llm") return renderLlm();
  if (path === "transactions" || path === "txs") return renderTransactions();
  if (path === "miners")     return renderMiners();
  if (path === "overlay" || path === "validators") return renderOverlay();
  if (path === "finality" || path === "dns") return renderFinality();
  if (path === "evm")        return renderEvmLane();
  if (path === "faucet")     return renderFaucet();
  if (path === "evmtx")      return renderEvmTx(decodeURIComponent(arg));
  if (path === "mtp" || path === "points") return renderMtp(arg ? decodeURIComponent(arg) : null);
  if (path === "registry" || path === "classes") return renderRegistry();
  if (path === "lane" || path === "rounds")     return renderLane();
  if (path === "line" || path === "model")      return renderModelLine(decodeURIComponent(arg));
  return renderHome();
}
window.addEventListener("hashchange", () => {
  // **Scroll to the top on every route change.** A hash router replaces the page's contents
  // without touching the scroll position, so navigating from the bottom of a long page (the
  // block list, the LLM jobs table) lands on the new page's empty tail — a full screen of
  // background colour with the header off-screen above. Reported as "the site is down": the
  // page had in fact rendered, 800px above the viewport. `scrollRestoration` is the browser's
  // half of the same problem on reload, and manual is what a router-driven page wants.
  window.scrollTo(0, 0);
  route();
});
if ("scrollRestoration" in history) {
  history.scrollRestoration = "manual";
}
// claude-route-guard-v1: a click on a link to the page you are already on re-renders it. The Home logo
// and every "Home" crumb point at "#/"; before this, the button a lost user reaches for did nothing
// whenever the hash already said "#/" — which is exactly the state a stale render leaves behind.
document.addEventListener("click", (e) => {
  if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  const a = e.target && e.target.closest ? e.target.closest('a[href^="#/"]') : null;
  if (!a || (a.target && a.target !== "_self")) return;
  const norm = (x) => (x || "").replace(/^#\/?/, "").replace(/\/+$/, "");
  if (norm(a.getAttribute("href")) !== norm(location.hash)) return;   // another page: the hashchange path renders it
  e.preventDefault();
  window.scrollTo(0, 0);
  route();
});

/* search */
$("#searchForm").addEventListener("submit", async (e) => {
  e.preventDefault();
  let q = $("#searchInput").value.trim();
  if (!q) return;
  // MTP ledger id. `addr:<address>` is the only form the points ledger issues now — points accrue
  // to the address that did the work, with no registration anywhere in the path.
  if (/^addr:misaka(dev|test|sim)?:[a-z0-9]+$/i.test(q)) { location.hash = "#/mtp/" + encodeURIComponent(q); return; }
  if (/^misaka(dev|test|sim)?:/i.test(q)) { location.hash = "#/address/" + q; return; }
  // `gh:<handle>` resolves only for the ids issued before the 2026-08 address policy, so their
  // published ledgers stay readable. Nothing new is issued under it.
  if (/^gh:[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$/.test(q)) { location.hash = "#/mtp/" + encodeURIComponent(q); return; }
  if (/^[0-9a-fA-F]{128}$/.test(q)) {
    // 128hex: try as a block first, else treat as tx id
    try { const r = await rpc("getBlock", { hash: q.toLowerCase(), includeTransactions: false });
          if (r && r.block) { location.hash = "#/block/" + q.toLowerCase(); return; } } catch {}
    location.hash = "#/tx/" + q.toLowerCase(); return;
  }
  // 32-byte (0x + 64 hex) = an EVM transaction hash (L1 ids/hashes are 128-hex).
  if (isEvmTxHash(q)) { location.hash = "#/evmtx/" + q.toLowerCase().replace(/^0x/, ""); return; }
  alert("Enter a 128-hex block hash / L1 tx id, an EVM tx hash (0x + 64 hex), a Misaka address, or an MTP id (addr:misakatest:…).");
});

/* ------------------------------- HOME ---------------------------------- */
let pollTimer = null;
// First hash segment, e.g. "" / "miners" / "blockdag". Used by armPoll to know which page is live.
function curSeg(){ return location.hash.replace(/^#\/?/,"").split("/")[0]; }
// Install a page poller safely: pages clear the global pollTimer on entry, then `await` an initial
// refresh, so the timer is set AFTER the await. If the user navigated away during that await we must
// NOT install (would orphan-leak + cross-kill the new page's timer). The interval also clears its
// OWN captured id (never the global) and only nulls the global if it still points at this id.
function armPoll(match, refresh, ms){
  if (!match()) return;                       // navigated away during the awaited initial refresh
  const id = setInterval(() => {
    if (match()) { if (!document.hidden) refresh(); }   // hidden/background tab → don't poll the node
    else { clearInterval(id); if (pollTimer === id) pollTimer = null; }
  }, ms);
  pollTimer = id;
}
async function renderHome(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `
    <section class="overview">
      <div class="ov-title">Network overview</div>
      <div id="ovGrid"><div class="loading">Loading network overview…</div></div>
    </section>
    <div class="sec-row"><h2 class="sec">Model economy</h2></div>
    <div class="cards">
      <a class="card" href="#/registry"><b>Registry</b><span>class lifecycle · ready seats</span></a>
      <a class="card" href="#/lane"><b>Execution lane</b><span>permits · domains · finals</span></a>
      <a class="card" href="#/llm"><b>LLM jobs</b><span>submissions · verification</span></a>
    </div>
    <div class="sec-row"><h2 class="sec">Recent blocks</h2><a class="sec-more" href="#/llm">LLM jobs view →</a></div>
    <div id="recentWrap" class="tblscroll"><div class="spin">Loading blocks…</div></div>
    <div class="sec-row"><h2 class="sec">Latest transactions <span class="dim" style="font-size:13px">(node-direct · newest first)</span></h2><a class="sec-more" href="#/transactions">View all →</a></div>
    <div id="txWrap" class="tblscroll"><div class="spin">Loading transactions…</div></div>`;
  if (loadCache()) renderRecent();   // instant paint from the previous session; refreshed below
  if (txFeed.length) renderTxFeed(); // instant paint of the in-memory tx feed on nav-back
  if (lastOverview) paintOverview(lastOverview);   // instant paint of the overview grid on nav-back
  await refreshHome();
  armPoll(() => curSeg()==="", refreshHome, 1500);
  onBlockAdded(() => refreshHome());   // push: a new block repaints the page the moment it lands
}

async function refreshHome(){
  if (homeBusy) return;          // fast polling: never stack overlapping refreshes (avoids cache races)
  homeBusy = true;
  try { await refreshHomeInner(); } finally { homeBusy = false; }
}
async function refreshHomeInner(){
  let info, dag, sbs, peers;
  try {
    [info, dag, sbs, peers] = await Promise.all([
      rpc("getInfo"), rpc("getBlockDagInfo"),
      rpc("getSinkBlueScore"), rpc("getConnectedPeerInfo")
    ]);
  } catch (e) {
    const sc = $("#ovGrid"); if (sc) sc.innerHTML = `<div class="err" style="grid-column:1/-1">Node query failed: ${esc(e.message)}</div>`;
    return;
  }
  // Drop the cached window when the node switches networks, OR when the devnet is reset under the
  // same name (fresh genesis ⇒ the live tip's blue score falls far below our newest cached block),
  // OR when the live tip has run FAR AHEAD of the cached window (e.g. the tab/cache sat stale for
  // hours: the cached top is thousands of blocks behind the sink). In the last case the bounded
  // getBlocks(lowHash=oldAnchor) walk would crawl forward only a little per poll and never reach
  // the tip, so the screen looks "frozen in the past" — drop the cache and cold-fill from the
  // current sink so we ALWAYS resync to the latest blocks.
  const liveBlue  = Number(sbs.blueScore || 0);
  const cachedTop = recent.length ? Number(recent[0].blueScore || 0) : -1;
  const netChanged  = cachedNet && cachedNet !== dag.network;
  const chainReset  = cachedNet === dag.network && recent.length && (liveBlue + 20 < cachedTop);
  // forward gap: tip is more than 2 windows ahead of our newest cached block → stale, resync.
  const staleAhead  = recent.length && cachedTop >= 0 && (liveBlue - cachedTop > RECENT_WINDOW * 2);
  if (netChanged || chainReset || staleAhead) {
    dropRecentCache();
    dropTxFeed();           // the latest-tx feed + rate window belong to the dead chain too
    rateSamples = [];
    // A re-genesis / network switch invalidates the persisted attestation log too — its entries
    // point at a dead chain's anchors (and can sit at a HIGHER DAA than the fresh tip), which is
    // what makes the "Recent attestations" table show stale, out-of-order rows. Drop it.
    attLog = []; saveAttLog(attLog); overlayStats.attLog = [];
  }
  cachedNet = dag.network;
  pushRate(dag.blockCount);   // feed the rolling BPS window
  // coin supply needs --utxoindex; treat as best-effort so it never blanks the page
  let supply = null;
  try { supply = await rpc("getCoinSupply"); } catch { supply = null; }
  // DNS-finality overlay status (kaspa-pq) — cheap, best-effort
  let dns = null;
  try { dns = await rpc("getDnsConfirmation"); } catch { dns = null; }
  await refreshBridgeAnchorBlue(dns);   // claude-bridge-fresh-v2
  // **This chain is not found by hashing, so it does not report a hashrate.**
  //
  // PALW attempt blocks carry `powAlgoId` 6 — where a candidate is won by running a pinned
  // LLM inference and the lottery is over ATTEMPTS, not hashes. `estimateNetworkHashesPerSecond`
  // still answers, because it only divides accumulated blue work by elapsed time, and the answer
  // ("30.6 kH/s") is a unit this network has no machine for: nothing on it computes hashes at any
  // rate. What a hashrate is a proxy for — is the network producing at the cadence it targets — is
  // observable directly, so that is what the box shows now.
  const powAlgo = overlayStats.powAlgoId;   // read off the tip header by the throttled refresh
  // throttled heavier work: mesh-wide peer count (via seed hub) + on-chain overlay scan
  maybeRefreshOverlay(dag.sink, recentLow);

  // Network node count — a LOWER BOUND, and labelled as one. `maybeRefreshOverlay` unions the peer
  // sets of every endpoint the site can reach and counts distinct node identities plus the vantage nodes;
  // see the note there for why one node's degree was the wrong number. Falls back to this node's
  // own peers + itself when no vantage answers.
  // **Sockets are not nodes.** `peerInfo` has one entry per live TCP connection, and a peer
  // routinely holds several — a reconnect whose old socket has not timed out, a co-located node
  // reachable both over loopback and over the host's public address. The fallback now uses the
  // same p2pId-based identity set as the healthy mesh path, so a first paint cannot count sockets
  // or same-host addresses as separate nodes.
  //
  // Count the same thing the healthy path counts, so the tile means one thing either way.
  refreshPeerCensus();   // throttled, not awaited: a first paint without it just omits the note
  const localPeers = distinctPeerNodes(peers.peerInfo).length;
  const meshNodes  = overlayStats.meshPeers;     // distinct nodes seen across every vantage we hold
  const nodeCount  = meshNodes != null ? meshNodes : localPeers + 1;
  const peersVal   = num(nodeCount);
  const peersSub   = (meshNodes != null
    ? `distinct nodes observed across ${num(overlayStats.meshVantages)} vantage${overlayStats.meshVantages===1?"":"s"}`
    : (info.isSynced ? "this node + its distinct peer nodes" : "not synced")) + censusTileNote();
  document.getElementById("footNet").textContent = dag.network || "—";
  const supplyVal = supply ? coin(supply.circulatingSompi)+" "+SYMBOL : "n/a";
  const supplySub = supply ? "max "+coin(supply.maxSompi)+" "+SYMBOL : "needs --utxoindex";
  const dnsVal = dns ? (dns.dnsConfirmed ? "DNS confirmed" : (dns.powConfirmed ? "PoW confirmed" : "pending"))
                     : "n/a";
  const dnsSub = dns ? `${ROLLOUT_STAGES[dns.rolloutStage]||("stage "+dns.rolloutStage)} · ${DNS_HEALTH[dns.health]||("health "+dns.health)}`
                       + bridgeFreshNote(dns, sbs)
                     : "overlay";
  // ONE uniform grid: headline metrics + chain/overlay/economics detail, every box the same size
  // (dynamic TPS/BPS/Miners/Block reward come from the rolling rate window + tx sweep).
  const bps = bpsNow();
  const circ = supply ? Number(supply.circulatingSompi)/10**DECIMALS : null;
  const maxc = supply ? Number(supply.maxSompi)/10**DECIMALS : null;
  const minedPct = (circ!=null && maxc) ? (circ/maxc*100) : null;
  // powAlgoId is read off the tip header by a THROTTLED refresh, so on first paint it is null —
  // and this site fronts a PALW network, so the null case must read PALW, not sprout a hashrate
  // box for a chain that computes no hashes.
  const palw = powAlgo == null ? true : isPalwLane(powAlgo);
  // **Cadence from block timestamps, not from the poll's rolling counter.**
  //
  // Cadence is what a hashrate box is really asking about: is the network producing at the rate it
  // targets? Deriving it from `bpsNow()` does not survive this chain's cadence — the rate window
  // spans a couple of minutes and the target is ONE BLOCK per 120 s, so the usual sample holds
  // db = 0 and reports a network at a dead stop while it is in fact on time. The blocks the page
  // has already fetched carry their own timestamps, which measure the same thing over a real span.
  const stamps = recent.map(r => Number(r.timestamp)).filter(Number.isFinite).sort((a,b)=>b-a);
  const spb = (stamps.length >= 2 && stamps[0] > stamps[stamps.length-1])
    ? (stamps[0] - stamps[stamps.length-1]) / 1000 / (stamps.length - 1)
    : ((bps != null && bps > 0) ? 1/bps : null);
  const bpsShown = spb != null ? 1/spb : bps;
  const targetSpb = 120;
  const cadenceSub = spb == null ? "target 120 s/block"
    : `target 120 s · ${spb <= targetSpb ? "ahead" : "behind"} by ${Math.abs(spb-targetSpb).toLocaleString("en-US",{maximumFractionDigits:0})} s`;
  // Sectioned (2026-08-27 redesign): Chain facts, then what makes this chain itself (PALW),
  // then economics, then who is on the network. Same boxes, grouped so the page reads.
  const boxes = [
    { sec:"Chain", k:"Block count", v: num(dag.blockCount), s:"headers "+num(dag.headerCount) },
    { sec:"Chain", k:"DAA score", v: num(dag.virtualDaaScore), s:"virtual" },
    { sec:"Chain", k:"Sink blue score", v: num(sbs.blueScore), s:"" },
    { sec:"Chain", k:"Block cadence", v: spb==null?"—":(spb.toLocaleString("en-US",{maximumFractionDigits:0})+" s/block"), s: cadenceSub, cls:"acc" },
    { sec:"Chain", k:"Difficulty", v: Number(dag.difficulty||0).toLocaleString("en-US",{maximumFractionDigits:0}),
      s: palw ? "attempt target" : "hash target" },
    { sec:"Chain", k:"Sink (tip)", v: `<a class="hash" href="#/block/${esc(dag.sink)}">${esc(short(dag.sink,8))}</a>`, s:"utxoindex "+(info.isUtxoIndexed?"on":"off") },

    palw
      ? (overlayStats.lane
          ? { sec:"PALW consensus", k:"Consensus", v: "PALW", cls: overlayStats.lane.alarm ? "" : "acc",
              s: overlayStats.lane.alarm
                ? `<span class="bad" title="${esc(overlayStats.lane.alarm)}">no work block in the last ${num(overlayStats.lane.window)} — heartbeats only</span>`
                : `<span title="${esc(overlayStats.lane.mix)}">work ${num(overlayStats.lane.work)} · heartbeat ${num(overlayStats.lane.heartbeat)} of the last ${num(overlayStats.lane.window)} blocks</span>` }
          : { sec:"PALW consensus", k:"Consensus", v: "PALW", cls:"acc",
              s: powAlgo == null ? "LLM attempt lottery" : "tip " + powLaneLabel(powAlgo).text.replace(/^algo-\d+ \u00b7 /,"algo-"+powAlgo+" · ") })
      : { sec:"PALW consensus", k:"Hashrate", v: fmtHashrate(Number.isFinite(Number(dag.difficulty)) ? Number(dag.difficulty)*2 : null), s:"≈ difficulty ×2" },
    { sec:"PALW consensus", k:"BPS", v: bpsShown==null?"—":bpsShown.toLocaleString("en-US",{maximumFractionDigits:4}), s:"DAG blocks/s" },
    { sec:"PALW consensus", k:"TPS", v: tpsEma==null?"—":tpsEma.toLocaleString("en-US",{maximumFractionDigits:2}), s:"excl. coinbase" },
    { sec:"PALW consensus", k:"Mempool", v: num(info.mempoolSize), s:"txs waiting" },
    { sec:"PALW consensus", k:"Miners", v: recentMiners?num(recentMiners):"—", s:"recent payouts", href:"#/miners" },

    { sec:"Supply", k:"Circulating", v: circ==null?"—":fmtCompact(circ)+" "+SYMBOL,
      // NOT "mined": on this chain the overwhelming majority of circulating supply is the genesis
      // premine, and calling 47% of the cap "mined" claimed an emission history that never happened.
      s: minedPct==null?"supply":(minedPct.toLocaleString("en-US",{maximumFractionDigits:2})+"% of cap"), cls:"good" },
    { sec:"Supply", k:"Block reward", v: blockSubsidy==null?"—":(Number(blockSubsidy)/10**DECIMALS).toLocaleString("en-US",{maximumFractionDigits:2})+" "+SYMBOL,
      s: palw ? "mean minted/block · rest escrowed" : "per block" },
    { sec:"Supply", k:"Max supply", v: maxc==null?"—":fmtCompact(maxc)+" "+SYMBOL, s:"MSK cap" },

    { sec:"Network", k:"Network", v: esc(dag.network||"—"), s:"v"+esc(info.serverVersion||"?"), cls:"acc" },
    { sec:"Network", k:"Nodes", v: peersVal, s: peersSub, href:"#/peers", cls:"ov" },
    { sec:"Network", k:"DNS finality", v: dnsVal, s: dnsSub, href:"#/overlay", cls:"ov" },
    { sec:"Network", k:"Validators", v: num(overlayStats.activeValidators),
      s: overlayStats.bonds.length ? overlayStats.bonds.length+" active bond"+(overlayStats.bonds.length>1?"s":"") : (overlayStats.rolloutActive?"overlay active":"0 bonds"), href:"#/overlay", cls:"ov" },
    { sec:"Network", k:"Staked", v: overlayStats.bonds.length ? coin(overlayStats.totalStaked)+" "+SYMBOL : (overlayStats.rolloutActive?"syncing…":"—"),
      s: overlayStats.bonds.length?"active bonds":"from attestations", href:"#/overlay", cls:"ov" },
  ];
  if (!document.getElementById("ovGrid")) return;   // navigated away during the awaited refresh
  paintOverview(boxes);
  maybeRefreshTxFeed(dag.sink);   // throttled node-direct sweep → latest-tx feed + miners + TPS
  await updateRecent(dag.sink, liveBlue);
}

// Paint the unified Network-overview grid. Every box is the same size (.ovbox); the array is
// already built by the poll (refreshHomeInner). Cached so it can instant-paint on nav-back.
let lastOverview = null;
function paintOverview(boxes){
  lastOverview = boxes;
  const el = document.getElementById("ovGrid"); if (!el) return;
  const box = b => {
    const body = `<div class="k">${b.k}</div><div class="v ${b.cls||""}">${b.v}</div>${b.s?`<div class="s">${b.s}</div>`:""}`;
    return `<div class="ovbox">${b.href?`<a href="${b.href}">${body}</a>`:body}</div>`;
  };
  // Group by section, order of first appearance. Boxes without a section render first, unheaded.
  const order = []; const by = new Map();
  for (const b of boxes){ const k = b.sec || ""; if (!by.has(k)){ by.set(k, []); order.push(k); } by.get(k).push(b); }
  el.innerHTML = order.map(sec =>
    (sec ? `<div class="ov-sec">${sec}</div>` : "") +
    `<div class="ovgrid">${by.get(sec).map(box).join("")}</div>`
  ).join("");
}

// Cold start: walk the selected chain back from the sink, summarizing each block we fetch
// (instead of discarding it) so the first screenful streams in row-by-row immediately, and use
// the deepest block as the getBlocks anchor for steady-state polling.
async function coldFill(sink){
  let h = sink; const acc = [];
  for (let i=0; i<RECENT_WINDOW && h; i++){
    let r; try { r = await rpc("getBlock", { hash: h, includeTransactions: false }); } catch { break; }
    if (!r || !r.block) break;
    acc.push(summarize(r.block));
    if (acc.length <= RECENT_LIMIT) { recent = acc.slice(); renderRecent(); }  // stream rows as they arrive
    const sp = r.block.verboseData && r.block.verboseData.selectedParentHash;
    if (!sp) break;
    h = sp;
  }
  recent = acc.slice(0, RECENT_LIMIT);
  return acc.length ? acc[acc.length-1].hash : null;
}

// Mirror the official kaspa-explorer feed: pull the recent DAG window via getBlocks(lowHash)
// and present it newest-first by blueScore. Stable under reorgs/orphaning, unlike walking
// selectedParentHash one block at a time (which jumps around when the chain reorganizes).
async function updateRecent(sink, liveBlue){
  if (sink && sink === lastSink) { renderRecent(); return; }   // tip unchanged → just refresh ages
  if (!recentLow) {                                            // cold start: stream rows + capture anchor
    recentLow = await coldFill(sink);
    lastSink = sink; saveCache(); return;
  }
  let gb;
  try { gb = await rpc("getBlocks", { lowHash: recentLow, includeBlocks: true, includeTransactions: false }); }
  catch { recentLow = null; return; }   // anchor pruned/invalid → re-init next poll
  const blocks = gb.blocks || [];
  if (blocks.length){
    const map = new Map(recent.map(b => [b.hash, b]));
    for (const blk of blocks) { const s = summarize(blk); map.set(s.hash, s); }
    const all = Array.from(map.values()).sort((a,b) =>
      (Number(b.blueScore) - Number(a.blueScore)) || (Number(b.daaScore) - Number(a.daaScore)));
    // Self-heal a stale anchor: if our newest row is still well behind the live sink, the bounded
    // getBlocks window can't reach the tip (it would crawl forward a little per poll, looking
    // frozen). Re-anchor straight at the current sink so the next paint shows the latest blocks.
    const newestNow = all.length ? Number(all[0].blueScore || 0) : -1;
    if (Number(liveBlue||0) > 0 && newestNow >= 0 && (Number(liveBlue) - newestNow > RECENT_WINDOW * 2)) {
      dropRecentCache();
      recentLow = await coldFill(sink);
      lastSink = sink; saveCache(); return;
    }
    recent = all.slice(0, RECENT_LIMIT);
    const anchor = all[Math.min(all.length - 1, RECENT_WINDOW)];
    if (anchor) recentLow = anchor.hash;   // keep the window bounded but overlapping
  } else {
    // getBlocks returned nothing forward of the anchor while the tip moved → anchor is stale/behind;
    // re-cold-fill from the current sink so we don't sit on an old window.
    if (sink !== lastSink) { dropRecentCache(); recentLow = await coldFill(sink); lastSink = sink; saveCache(); return; }
  }
  lastSink = sink;
  saveCache();
  renderRecent();
}

function renderRecent(){
  const w = $("#recentWrap"); if (!w) return;
  if (!recent.length){ w.innerHTML = `<div class="spin">No blocks yet…</div>`; shownHashes = new Set(); return; }
  const firstPaint = shownHashes.size === 0;   // don't flash the whole table on the very first paint
  // `recent` is kept in CONSENSUS order (blueScore desc) because refreshHomeInner's staleness
  // checks read recent[0].blueScore to decide whether to re-anchor. That order is NOT time order,
  // and in a DAG the two diverge badly: measured on the live chain 2026-09-06, 482 of 1,723
  // adjacent pairs had the UPPER row older than the row beneath it, by up to 1h50m. A table whose
  // Age column walks backwards is what a reader calls broken. So sort a COPY for display only —
  // the stored order, and every control path that reads it, is untouched.
  const rows = recent.slice().sort((a,b) =>
    (Number(b.timestamp) - Number(a.timestamp)) ||
    (Number(b.blueScore) - Number(a.blueScore)) ||
    (Number(b.daaScore)  - Number(a.daaScore)));
  w.innerHTML = `<table class="tbl"><thead><tr>
      <th>Block hash</th><th class="num">DAA</th><th class="num">Blue</th>
      <th class="num">Parents</th><th class="num">Txs</th><th title="the model that mined the block, from its header">Model</th><th class="right">Age</th>
    </tr></thead><tbody>${rows.map(b => `
      <tr class="${(!firstPaint && !shownHashes.has(b.hash)) ? 'rowNew' : ''}"><td>${linkBlock(b.hash)}</td>
          <td class="num">${num(b.daaScore)}</td>
          <td class="num">${num(b.blueScore)}</td>
          <td class="num">${num(b.nParents)}</td>
          <td class="num">${num(b.nTx)}</td>
          <td class="nowrap">${recentModelCell(b)}</td>
          <td class="right dim" title="${esc(dt(b.timestamp))}">${ago(b.timestamp)}</td></tr>`).join("")}
    </tbody></table>`;
  shownHashes = new Set(rows.map(b => b.hash));   // baseline for the next render's new-row flash
}

/* ----------------------- LATEST TRANSACTIONS (home) -------------------- */
// Link an address with a shortened label (Misaka ML-DSA addresses are long; the full value
// is the href so the row stays a single line).
function linkAddrShort(a){ return `<a class="hash" href="#/address/${esc(a)}">${esc(short(a,14))}</a>`; }

// Throttled, node-direct sweep of the recent DAG window for transactions. Independent of the
// Postgres tx index (which can lag) → the feed is always live. Also derives the recent
// miner-payout set and the live (non-coinbase) TPS estimate.
async function maybeRefreshTxFeed(sink){
  if (txFeedBusy) return;
  if (Date.now() - txFeedTs < TX_FEED_REFRESH_MS) return;
  txFeedBusy = true;
  try { await refreshTxFeedInner(sink); } catch {} finally { txFeedTs = Date.now(); txFeedBusy = false; }
}
async function refreshTxFeedInner(sink){
  // getBlocks returns blocks FORWARD of lowHash, so anchor a window behind the sink. Reuse the
  // recent-blocks anchor when available; otherwise walk selectedParentHash back from the sink.
  let anchor = recentLow;
  if (!anchor && sink){
    anchor = sink;
    for (let i=0;i<25 && anchor;i++){
      let r; try { r = await rpc("getBlock", { hash: anchor, includeTransactions:false }); } catch { break; }
      const sp = r && r.block && r.block.verboseData && r.block.verboseData.selectedParentHash;
      if (!sp) break; anchor = sp;
    }
  }
  if (!anchor) return;
  let gb; try { gb = await rpc("getBlocks", { lowHash: anchor, includeBlocks:true, includeTransactions:true }); } catch { return; }
  const blocks = gb.blocks || [];
  if (!blocks.length){ renderTxFeed(); return; }   // swept, nothing forward → paint the empty-state note

  const miners = new Set();
  let nNonCoinbase = 0, minTs = Infinity, maxTs = 0;
  // **Mean coinbase, not minimum.** The minimum is not the subsidy on this chain: a PALW block
  // withholds its claim's escrowed worker reward from its own coinbase (ADR-0042 Decision 10) and
  // pays it out only when that claim reaches Final, so per-block coinbase legitimately ranges from
  // near zero to several times the base. Taking the smallest sample reported 0.0037 MSK/block on
  // the previous network against a measured ~573 MSK/block actually minted.
  let cbSum = 0, cbBlocks = 0;
  const fresh = [];
  const windowIds = new Set();        // every non-coinbase txid in THIS sweep's window (dedup floor)
  for (const blk of blocks){
    const bh = blk.header && blk.header.hash;
    const bts = Number((blk.header && blk.header.timestamp)||0);
    if (bts){ if (bts<minTs) minTs=bts; if (bts>maxTs) maxTs=bts; }
    for (const t of (blk.transactions||[])){
      const kind = txKind(t);
      // true largest output (by value) is the representative payee — miner subsidy for coinbase,
      // recipient/change for a transfer. If it has no address (non-standard), addr stays null.
      const outs = (t.outputs||[]);
      let maxOut=null, value=0;
      for (const o of outs){ const v=Number(o.value||0); value+=v; if (!maxOut || v>Number(maxOut.value||0)) maxOut=o; }
      const addr = (maxOut && maxOut.verboseData && maxOut.verboseData.scriptPublicKeyAddress) || null;
      if (kind.key === "coinbase"){                 // mining issuance → Miners stat + subsidy, not the feed
        if (addr) miners.add(addr);
        if (value>0){ cbSum += value; cbBlocks++; }
        continue;
      }
      nNonCoinbase++;
      const id = t.verboseData && t.verboseData.transactionId;
      if (!id) continue;
      windowIds.add(id);
      if (txKnown.has(id)) continue;
      fresh.push({ txid:id, kind, addr, value, ins:(t.inputs||[]).length, outs:outs.length, block:bh, ts:bts });
    }
  }
  if (fresh.length){
    // de-dupe within this sweep (a tx can appear in multiple parallel blocks) before merging
    const uniq = new Map(); fresh.forEach(t => { if (!uniq.has(t.txid)) uniq.set(t.txid, t); });
    const add = [...uniq.values()].sort((a,b)=>(b.ts-a.ts) || (Number(b.value)-Number(a.value)));
    add.forEach(t => txKnown.add(t.txid));
    txFeed = add.concat(txFeed).slice(0, TX_FEED_LIMIT);
    // bound dedup memory without resurrecting in-window txs: floor = this window's ids ∪ feed ids
    if (txKnown.size > 4000) txKnown = new Set([...windowIds, ...txFeed.map(t=>t.txid)]);
  }
  recentMiners = miners.size;
  if (cbBlocks > 0) blockSubsidy = cbSum / cbBlocks;        // mean minted per block (sompi)
  const span = (maxTs>minTs) ? (maxTs-minTs)/1000 : 0;
  if (span > 0){ const tps = nNonCoinbase/span; tpsEma = (tpsEma==null) ? tps : (tpsEma*0.6 + tps*0.4); }
  renderTxFeed();
}

function renderTxFeed(){
  const w = $("#txWrap"); if (!w) return;
  if (!txFeed.length){
    w.innerHTML = `<div class="note">No standard transactions in the recent block window — only coinbase issuance. User transactions (sends, stake/attestation, EVM) appear here, newest first, as they are mined.</div>`;
    txShown = new Set(); return;
  }
  const firstPaint = txShown.size === 0;
  w.innerHTML = `<table class="tbl"><thead><tr>
      <th>Transaction id</th><th>Type</th><th>To <span class="dim">(largest output)</span></th>
      <th class="num">Amount</th><th class="right">Age</th>
    </tr></thead><tbody>${txFeed.map(t=>`
      <tr class="${(!firstPaint && !txShown.has(t.txid))?'rowNew':''}">
        <td>${linkTx(t.txid)}</td>
        <td>${kindPill(t.kind)}</td>
        <td>${t.addr?linkAddrShort(t.addr):'<span class="muted">non-standard</span>'}</td>
        <td class="num coin">${coin(t.value)} ${SYMBOL}</td>
        <td class="right dim" title="${esc(dt(t.ts))}">${t.ts?ago(t.ts):"—"}</td></tr>`).join("")}
    </tbody></table>`;
  txShown = new Set(txFeed.map(t=>t.txid));
}

/* ------------------------------- BLOCK --------------------------------- */
async function renderBlock(hash){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Block</div>
    <h1 class="page">Block</h1><div class="loading">Loading block…</div>`;
  let r;
  try { r = await rpc("getBlock", { hash: hash.toLowerCase(), includeTransactions: true }); }
  catch (e) { return showErrFor(__g, `Block not found: ${esc(e.message)}`, hash); }
  if (!r || !r.block) return showErrFor(__g, "Block not found.", hash);
  const blk = r.block, hd = blk.header, vd = blk.verboseData || {};
  const txs = blk.transactions || [];
  txs.forEach(t => { const id = t.verboseData && t.verboseData.transactionId; if (id) txCache.set(id, { tx: t, blockHash: hd.hash }); });
  const parents = (hd.parentsByLevel && hd.parentsByLevel[0]) || [];

  // DNS finality for this block (kaspa-pq ADR-0009): is it covered by the stake-confirmed anchor,
  // and which validators have attested (scored) it? Best-effort — overlay calls never block the page.
  let bdns = null; try { bdns = await rpc("getDnsConfirmation", { blockHash: String(hd.hash).toLowerCase() }); } catch {}
  const _bDaa       = Number(hd.daaScore || 0);
  const _anchorHash = bdns && bdns.lastDnsConfirmedAnchor ? String(bdns.lastDnsConfirmedAnchor).toLowerCase() : "";
  const _anchorDaa  = bdns ? Number(bdns.lastDnsConfirmedAnchorDaaScore || 0) : 0;
  // Prefer the node's authoritative per-block evaluation (block_* from getDnsConfirmation(blockHash));
  // fall back to the client-side heuristic only on older nodes that do not return them.
  const _hasSrv     = bdns && (typeof bdns.blockIsDnsFinal === "boolean" || typeof bdns.blockIsConfirmedAnchor === "boolean");
  const _isAnchor   = _hasSrv ? !!bdns.blockIsConfirmedAnchor
                              : (_anchorHash && /[1-9a-f]/.test(_anchorHash) && _anchorHash === String(hd.hash).toLowerCase());
  const _dnsFinal   = _hasSrv ? !!bdns.blockIsDnsFinal
                              : (bdns && bdns.dnsConfirmed && _anchorDaa > 0 && vd.isChainBlock && _bDaa <= _anchorDaa);
  const _dnsCell = _isAnchor
    ? '<span class="pill chain">stake-confirmed anchor</span>'
    : _dnsFinal
      ? `<span class="pill chain">DNS-final</span> <span class="dim">(≤ confirmed anchor @ DAA ${num(_anchorDaa)})</span>`
      : (bdns && bdns.dnsConfirmed)
        ? `<span class="pill">not yet DNS-final</span> <span class="dim">(beyond the confirmed anchor @ DAA ${num(_anchorDaa)})</span>`
        : '<span class="dim">DNS overlay not confirming</span>';
  // DNS score = the StakeScore (stake-weight) confirming this block's finality, plus the attestation
  // shards in the recent window whose anchor sits at/above this block (so they transitively finalize it).
  // A validator attests ~one anchor per epoch, so an exact-hash match is rare — coverage is the real signal.
  const _coverAtts   = (attLog || []).filter(a => Number(a.targetDaa) >= _bDaa);
  const _coverShards = _coverAtts.reduce((s,a)=>s + (Number(a.nAtt)||1), 0);
  const _coverVals   = [...new Set(_coverAtts.map(a=>short(a.validatorId,8)))];
  let _scoreCell;
  if (_isAnchor || _dnsFinal){
    const _sd = (bdns && bdns.stakeDepth!=null && bdns.stakeDepth!=="")
      ? `<span class="pill chain">StakeScore ${esc(bdns.stakeDepth)} / ${esc(bdns.requiredStakeDepth)} req</span>` : "";
    const _ev = _coverAtts.length
      ? `<span class="dim">${num(_coverShards)} attestation shard(s) in window confirm this block · ${_coverVals.join(", ")}</span>`
      : `<span class="dim">confirmed by the stake-anchor (no shard in the recent window)</span>`;
    _scoreCell = _sd ? `${_sd} ${_ev}` : _ev;
  } else if (bdns && bdns.dnsConfirmed){
    _scoreCell = `<span class="dim">pending — beyond the confirmed anchor @ DAA ${num(_anchorDaa)}</span>`;
  } else {
    _scoreCell = '<span class="dim">DNS overlay not confirming</span>';
  }

  const rows = [
    ["Hash", copyable(hd.hash)],
    ["Type", vd.isChainBlock?'<span class="pill chain">chain block</span>':'<span class="pill red">merged (red/blue)</span>'],
    ["Timestamp", `${esc(dt(hd.timestamp))} <span class="dim">(${ago(hd.timestamp)})</span>`],
    ["DAA score", num(hd.daaScore)],
    ["Blue score", num(hd.blueScore)],
    ["DNS finality", _dnsCell],
    ["DNS score", _scoreCell],
    ["Blue work", `<span class="hash">${esc(hd.blueWork)}</span>`],
    ["Difficulty", Number(vd.difficulty||0).toLocaleString("en-US",{maximumFractionDigits:0})],
    ["Version", num(hd.version)],
    ["PoW lane", powLaneCell(hd.powAlgoId)],
    ["Bits", num(hd.bits)],
    ["Nonce", `<span class="mono">${esc(hd.nonce)}</span>`],
    ["Selected parent", vd.selectedParentHash ? linkBlock(vd.selectedParentHash) : "—"],
    ["Parents (L0)", parents.length ? parents.map(linkBlock).join("<br>") : "— (genesis)"],
    ["Merkle root (tx)", `<span class="hash">${esc(hd.hashMerkleRoot)}</span>`],
    ["Accepted-ID merkle root", `<span class="hash">${esc(hd.acceptedIdMerkleRoot)}</span>`],
    ["UTXO commitment", `<span class="hash">${esc(hd.utxoCommitment)}</span>`],
    ["Pruning point", linkBlock(hd.pruningPoint)],
    ["Merge set (blues)", num((vd.mergeSetBluesHashes||[]).length)],
    ["Merge set (reds)", num((vd.mergeSetRedsHashes||[]).length)],
  ];
  // kaspa-pq EVM Lane (ADR-0020 §4): v2 blocks carry the two EVM header commitments. Show them
  // plus whether this block's own payload carries EVM content (bytes beyond the empty baseline).
  let _evm = { txs: [], systemOps: 0 };
  if (hd.evmPayloadHash || hd.evmCommitmentRoot){
    _evm = decodeEvmPayload(blk.evmPayload);
    const _summary = _evm.txs.length
      ? `<span class="pill evm">${num(_evm.txs.length)} EVM tx${_evm.txs.length>1?"s":""}</span>`
      : (_evm.systemOps ? `<span class="pill evm">${num(_evm.systemOps)} deposit-claim op${_evm.systemOps>1?"s":""}</span>`
                        : `<span class="dim">empty (no EVM txs in this block)</span>`);
    rows.push(["EVM payload", _summary]);
    rows.push(["EVM payload hash", `<span class="hash">${esc(hd.evmPayloadHash||"—")}</span>`]);
    rows.push(["EVM commitment root", `<a class="hash" href="#/evm">${esc(hd.evmCommitmentRoot||"—")}</a>`]);
  }
  // kaspa-pq EVM Lane: the EVM transactions this block carries (data availability). Each links to
  // its receipt/inclusion view; the hash is keccak256 of the raw EIP-2718 bytes (the Ethereum hash).
  const _evmTxSection = _evm.txs.length ? `
    <h2 class="sec">EVM transactions (${_evm.txs.length}) <span class="dim" style="font-size:13px">— carried in this block's payload</span></h2>
    <table class="tbl"><thead><tr><th>EVM tx hash</th><th>Type</th><th class="num">Size</th></tr></thead><tbody>${
      _evm.txs.map(t=>`<tr>
        <td>${t.hash ? linkEvmTx(t.hash) : '<span class="dim">keccak unavailable</span>'}</td>
        <td><span class="pill evm">${esc(evmTxTypeLabel(t.type))}</span></td>
        <td class="num">${num(t.len)} B</td></tr>`).join("")
    }</tbody></table>` : "";
  const _evmClaimSection = (_evm.claims && _evm.claims.length) ? `
    <h2 class="sec">Bridge deposit-claims (${_evm.claims.length}) <span class="dim" style="font-size:13px">— UTXO→EVM credits in this block's payload</span></h2>
    <table class="tbl"><thead><tr><th>EVM address (credited)</th><th class="num">Amount (MSK)</th><th class="num">Tip</th><th>Lock outpoint</th></tr></thead><tbody>${
      _evm.claims.map(c=>`<tr>
        <td><span class="mono">${esc(c.evmAddress)}</span></td>
        <td class="num">${coin(c.amountSompi)}</td>
        <td class="num dim">${coin(c.tipSompi)}</td>
        <td><span class="hash" title="${esc(c.outpoint)}">${esc(c.outpoint.slice(0,12))}…:${esc(c.outpoint.split(":")[1]||"0")}</span></td></tr>`).join("")
    }</tbody></table>` : "";
  const _evmSection = _evmTxSection + _evmClaimSection;
  // **What this block's inference said.** The header holds the roots; the feed (llm-jobs.json,
  // decoded server-side from the same material every panel seat re-derives) holds the text where
  // the fleet retained it. Two kinds of job can sit in one block: the attempt-lane job that mined
  // it (keyed by block), and free-prompt claims whose carrier transaction it included (keyed by
  // claim, indexed by block). Both are shown the way the LLM Jobs page shows them.
  await refreshLlmJobs();
  const _bh = String(hd.hash).toLowerCase();
  const _llmJob = llmJobs.get(_bh) || llmJobs.get(hd.hash) || null;
  const _llmFps = llmFpByBlock.get(_bh) || [];
  const _llmCard = (job, fp) => {
    // Same rule as the LLM Jobs page: an anchor-derived input is public because anyone recomputes
    // it; a person's prompt and any answer are committed and not disclosed.
    const inT  = job.prompt_text != null ? job.prompt_text : ((job.prompt_ids||[]).join(" ") || null);
    const floor = llmIsFloor(job.class_id || job.classId);
    const lane = fp ? '<span class="pill blue">free-prompt</span>' : (floor ? '<span class="pill">floor</span>' : '<span class="pill">attempt</span>');
    const cls = job.class ? esc(job.class) : (job.class_id ? esc(llmClassName(job.class_id)) : "—");
    const inCell  = floor
      ? '<span class="dim">deterministic integer job (no tokens)</span>'
      : (fp ? (llmDisclosedCell(job.disclosed) || llmPromptOnChain(job.prompt_tokens?` <span class="dim">· ${num(job.prompt_tokens)} tokens</span>`:""))
            : llmInCell(inT, job.prompt_ids, true));
    // ADR-0078's derived artifacts are on chain — the kind, the transformer, the id and the size —
    // so they are named here beside the sealed output. They are what a free prompt PUBLISHED.
    const derived = (job.derived && job.derived.length)
      ? ` <span class="dim">→ derived: ${job.derived.map(d=>esc(`${d.kind_name||("kind "+d.kind)} ${d.artifact_bytes} B`)).join(", ")}</span>` : "";
    const outCell = floor ? '<span class="dim">—</span>' : (llmDisclosedCell(job.disclosed) || llmSealed("output", "", derived));
    return `<div class="llm-card">
      <div class="llm-card-head">${lane} <b>${cls}</b>${(()=>{ const pwu = job.pwu!=null&&job.pwu!==""?job.pwu:(job.work_leaves!=null?job.work_leaves:null); return pwu!=null?` <span class="dim">· ${num(pwu)} pwu</span>`:""; })()}${(()=>{ const pre = job.prefill!=null?job.prefill:job.prompt_tokens, dec = job.decode!=null?job.decode:job.decode_tokens; return dec!=null?` <span class="dim">· prompt ${num(pre)} / decode ${num(dec)} tokens</span>`:""; })()}${job.claim?` <span class="hash dim" title="${esc(job.claim)}">· claim ${esc(short(job.claim,8))}</span>`:""}</div>
      <div class="llm-card-row"><div class="key">Input${fp?"":" (anchor-derived)"}</div><div class="io">${inCell}</div></div>
      <div class="llm-card-row"><div class="key">Output</div><div class="io">${outCell}</div></div>
    </div>`;
  };
  const _isLlmLane = isPalwLane(Number(hd.powAlgoId));
  // The input/output card still comes from the feed — it is the only thing that decodes the
  // ANSWER. Everything about WHO made the block and WHO judged it now comes from the block itself,
  // so a feed that has not caught up no longer erases the block's own provenance.
  const _llmSection = (_llmJob || _llmFps.length) ? `
    <h2 class="sec">LLM job in this block <span class="dim" style="font-size:13px">— the inference this block claims, and the prompts it carried</span></h2>
    ${_llmJob ? _llmCard(_llmJob, false) : ""}
    ${_llmFps.map(f => _llmCard(f, true)).join("")}`
    : "";
  const _provSection = blockProductionSection(hd, _llmJob);
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › Block ${esc(short(hd.hash,8))}</div>
    <h1 class="page">Block <span class="hash" style="font-size:14px;color:var(--mut)">${esc(short(hd.hash,14))}</span></h1>
    <div class="kv">${rows.map(r=>`<div class="row"><div class="key">${r[0]}</div><div class="val">${r[1]}</div></div>`).join("")}</div>
    ${_evmSection}
    ${_provSection}
    ${_llmSection}
    <h2 class="sec">Transactions (${txs.length})</h2>
    ${renderTxTable(txs)}`;
  // After paint, never before: the verification half costs a state read plus a bounded forward
  // walk, and the block's own facts should be on screen while that runs. Fire-and-forget — a
  // failure leaves the section's own "could not ask" wording, which is the truthful one.
  fillBlockVerification(hd.hash);
}

function renderTxTable(txs){
  if (!txs.length) return `<div class="note">No transactions.</div>`;
  return `<table class="tbl"><thead><tr>
      <th>Transaction id</th><th class="num">Inputs</th><th class="num">Outputs</th>
      <th class="num">Out value</th><th>Type</th></tr></thead><tbody>${txs.map(t=>{
        const id = t.verboseData && t.verboseData.transactionId;
        const outSum = (t.outputs||[]).reduce((a,o)=>a+Number(o.value||0),0);
        return `<tr><td>${id?linkTx(id):"—"}</td>
          <td class="num">${num((t.inputs||[]).length)}</td>
          <td class="num">${num((t.outputs||[]).length)}</td>
          <td class="num coin">${coin(outSum)} ${SYMBOL}</td>
          <td>${kindPill(txKind(t))}</td></tr>`;
      }).join("")}</tbody></table>`;
}

/* --------------------------------- TX ---------------------------------- */
/* ---- explorer REST index (kaspa-rest-server + Postgres) for instant lookups ---- */
async function apiGet(path){
  try {
    const r = await fetch(path, { headers: { "Accept": "application/json" } });
    if (!r.ok) return null;
    return await r.json();
  } catch { return null; }
}
// Map a REST get_transaction payload to the wRPC-block "hit" shape renderTx uses.
function restTxToHit(j){
  if (!j || !j.transaction_id) return null;
  return {
    blockHash: (Array.isArray(j.block_hash) && j.block_hash[0]) || j.accepting_block_hash || null,
    tx: {
      subnetworkId: j.subnetwork_id || "",
      version: 0,
      mass: j.mass,
      payload: "",
      verboseData: { transactionId: j.transaction_id, hash: j.hash, blockTime: j.block_time, computeMass: j.mass },
      inputs: (j.inputs || []).map(i => ({ previousOutpoint: { transactionId: i.previous_outpoint_hash, index: i.previous_outpoint_index } })),
      outputs: (j.outputs || []).map(o => ({ value: o.amount, verboseData: { scriptPublicKeyAddress: o.script_public_key_address, scriptPublicKeyType: o.script_public_key_type } })),
    }
  };
}
async function fetchTxFromIndex(txid){
  const j = await apiGet(`/transactions/${txid}?inputs=true&outputs=true&resolve_previous_outpoints=light`);
  if (!j) return null;
  // The index can know the tx's blocks but answer with inputs/outputs null (observed live 2026-09-11
  // on every tx). Mapped as-is that renders "Coinbase — newly minted" with no outputs, so read the
  // body from the node's copy of the first containing block instead — one call, exact.
  if ((!j.inputs || !j.outputs) && Array.isArray(j.block_hash) && j.block_hash[0]){
    try {
      const r = await rpc("getBlock", { hash: j.block_hash[0], includeTransactions: true });
      const t = r && r.block && (r.block.transactions || []).find(x => x.verboseData && x.verboseData.transactionId === txid);
      if (t){ txCache.set(txid, { tx: t, blockHash: r.block.header.hash }); return txCache.get(txid); }
    } catch {}
  }
  return restTxToHit(j);
}

async function renderTx(txid){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  txid = txid.toLowerCase();
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Transaction</div>
    <h1 class="page">Transaction</h1><div class="loading">Looking up transaction…</div>`;
  let hit = txCache.get(txid);
  if (!hit) hit = await fetchTxFromIndex(txid);   // instant: explorer index (no block scan)
  if (!hit) hit = await scanForTx(txid);          // fallback: scan recent blocks if not yet indexed
  if (!hit) return showErrFor(__g, `Transaction not found. It may still be pending (waiting to be mined), or outside the indexed range.`, txid);
  const t = hit.tx, vd = t.verboseData || {};
  const outSum = (t.outputs||[]).reduce((a,o)=>a+Number(o.value||0),0);
  const coinbase = !(t.inputs||[]).length;
  const kind = txKind(t);
  const rows = [
    ["Transaction id", copyable(vd.transactionId || txid)],
    ["In block", linkBlock(hit.blockHash)],
    ["Block time", `${esc(dt(vd.blockTime))} <span class="dim">(${ago(vd.blockTime)})</span>`],
    ["Type", kindPill(kind)],
    ["Mass", num(vd.computeMass || t.mass)],
    ["Subnetwork", `<span class="mono">${esc(t.subnetworkId)}</span>`],
    ["Output total", `<span class="coin">${coin(outSum)} ${SYMBOL}</span>`],
    ["Version", num(t.version)],
  ];
  // kaspa-pq: for a stake-bond tx, read the bond's on-chain status at the node's sink
  if (kind.key === "bond"){
    try {
      const b = await rpc("getStakeBond", { bondOutpoint: txid + ":0" });
      if (b && b.available){
        rows.push(["Bond validator id", copyable(b.validatorId)]);
        rows.push(["Bond amount", `<span class="coin">${coin(b.amount)} ${SYMBOL}</span>`]);
        rows.push(["Activation DAA", num(b.activationDaaScore)]);
        rows.push(["Bond status", b.effectiveStatus==="active"?'<span class="pill chain">active</span>':`<span class="pill">${esc(b.effectiveStatus||"—")}</span>`]);
      }
    } catch {}
  }
  const inputs = (t.inputs||[]);
  const outputs = (t.outputs||[]);
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › Tx ${esc(short(txid,8))}</div>
    <h1 class="page">Transaction <span class="hash" style="font-size:14px;color:var(--mut)">${esc(short(txid,14))}</span></h1>
    <div class="kv">${rows.map(r=>`<div class="row"><div class="key">${r[0]}</div><div class="val">${r[1]}</div></div>`).join("")}</div>
    <div class="txbox"><b>Inputs &amp; outputs</b>
      <div class="io">
        <div class="side"><div class="dim" style="margin-bottom:4px">Inputs (${inputs.length})</div>
          ${coinbase?'<div class="line muted">Coinbase — newly minted</div>':
            inputs.map(i=>{const op=i.previousOutpoint||{};return `<div class="line">${linkTx(op.transactionId)} <span class="dim">#${esc(op.index||0)}</span></div>`;}).join("")||'<div class="line muted">—</div>'}
        </div>
        <div class="arrow">➔</div>
        <div class="side"><div class="dim" style="margin-bottom:4px">Outputs (${outputs.length})</div>
          ${outputs.map(o=>{const a=o.verboseData&&o.verboseData.scriptPublicKeyAddress;
            const lk = a ? null : parseDepositLock(typeof o.scriptPublicKey==="string" ? o.scriptPublicKey : (o.scriptPublicKey&&o.scriptPublicKey.script)||"");
            const pfx = (bridgeScan&&bridgeScan.prefix) || String(outputs.map(x=>x.verboseData&&x.verboseData.scriptPublicKeyAddress).find(Boolean)||"misakatest:").split(":")[0];
            const who = lk ? `<span class="pill evm">EVM deposit lock</span> → <span class="mono">${esc(lk.evmAddress)}</span><div class="dim">refund ${lk.timeoutDaa==null?"never":"from DAA "+num(lk.timeoutDaa)}${lk.tipSompi?` · claim tip ${coin(lk.tipSompi)} ${SYMBOL}`:""} · depositor ${linkAddrShort(p2pkhMldsaAddress(lk.refundScript, pfx))} · <a href="#/evm">bridge</a></div>` : (a?linkAddr(a):'<span class="muted">non-standard</span>');
            return `<div class="line">${who}<br><span class="coin">${coin(o.value)} ${SYMBOL}</span> <span class="dim">· ${esc(o.verboseData&&o.verboseData.scriptPublicKeyType||(lk?"evmdepositlock":""))}</span></div>`;}).join("")}
        </div>
      </div>
    </div>
    ${t.payload?`<div class="txbox"><b>Payload</b><div class="hash" style="margin-top:6px;font-size:12px">${esc(t.payload)}</div></div>`:""}`;
}

async function scanForTx(txid){
  // No node tx-index: walk the selected-parent chain from the sink looking for the id.
  let dag; try { dag = await rpc("getBlockDagInfo"); } catch { return null; }
  let h = dag.sink;
  const wrap = $(".loading");
  for (let i=0; i<TXSCAN_LIMIT && h; i++){
    if (wrap && i%20===0) wrap.textContent = `Scanning recent blocks for transaction… (${i}/${TXSCAN_LIMIT})`;
    let r; try { r = await rpc("getBlock", { hash: h, includeTransactions: true }); } catch { return null; }
    if (!r || !r.block) return null;
    for (const t of (r.block.transactions||[])){
      const id = t.verboseData && t.verboseData.transactionId;
      if (id){ txCache.set(id, { tx: t, blockHash: r.block.header.hash });
               if (id === txid) return txCache.get(id); }
    }
    h = r.block.verboseData && r.block.verboseData.selectedParentHash;
  }
  return null;
}

/* ------------------------------- ADDRESS ------------------------------- */
async function renderAddress(addr){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Address</div>
    <h1 class="page">Address</h1><div class="loading">Loading address…</div>`;
  let bal;
  try {
    bal = await rpc("getBalanceByAddress", { address: addr });
  } catch (e) {
    if (/utxoindex/i.test(e.message)) {
      viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Address</div>
        <h1 class="page">Address</h1>
        <div class="kv"><div class="row"><div class="key">Address</div><div class="val">${copyable(addr)}</div></div><div class="row"><div class="key">MTP points</div><div class="val"><a class="hash" href="#/mtp/${encodeURIComponent("addr:" + addr)}">addr:${esc(addr)}</a> <span class="dim">— testnet points for this address</span></div></div></div>
        <div class="note">Balance &amp; UTXO lookups are unavailable because the node is running without <span class="mono">--utxoindex</span>. Restart the node with that flag to enable address views.</div>`;
      return;
    }
    return showErrFor(__g, `Address query failed: ${esc(e.message)}`, addr);
  }
  // The node caps getUtxosByAddresses (250k entries / ~50 MiB). A mining payout address that
  // has never been consolidated exceeds it, so treat that as "too many to enumerate" rather
  // than an error: page the first slice and keep the rest of the view intact.
  const UTXO_PAGE = 100;
  let entries = [], utxoTotal = null, utxoPartial = false, utxoErr = null;
  try {
    const utxos = await rpc("getUtxosByAddresses", { addresses: [addr] });
    entries = utxos.entries || [];
    utxoTotal = entries.length;
  } catch (e) {
    utxoPartial = true;
    // The cap error carries the real count; it is the only place the node reports it.
    const m = /(\d+)\s+UTXOs/.exec(e.message || "");
    if (m) utxoTotal = Number(m[1]);
    try {
      const pg = await rpc("getUtxosByAddressPage", { address: addr, cursor: "", limit: UTXO_PAGE });
      entries = pg.entries || [];
    } catch (e2) { utxoErr = e2.message; }
  }
  // Instant transaction history from the explorer index (kaspa-rest-server + Postgres).
  const [txs, cnt] = await Promise.all([
    apiGet(`/addresses/${encodeURIComponent(addr)}/full-transactions?limit=50&resolve_previous_outpoints=light`),
    apiGet(`/addresses/${encodeURIComponent(addr)}/transactions-count`)
  ]);
  const txList = Array.isArray(txs) ? txs : [];
  const txCount = (cnt && typeof cnt.total === "number") ? cnt.total : null;
  const rows = [
    ["Address", copyable(addr)],
    ["Balance", `<span class="coin">${coin(bal.balance)} ${SYMBOL}</span>`],
    ["UTXO count", utxoTotal != null ? num(utxoTotal) : (utxoPartial ? `<span class="dim">too many to enumerate</span>` : num(entries.length))],
    ["Transactions", txCount != null ? num(txCount) : (txList.length ? num(txList.length) + "+" : "—")],
    ["MTP points", `<a class="hash" href="#/mtp/${encodeURIComponent("addr:" + addr)}">addr:${esc(addr)}</a> <span class="dim">— testnet points for this address</span>`],
  ];
  const netFor = (t) => {
    let recv = 0, sent = 0;
    for (const o of (t.outputs || [])) if (o.script_public_key_address === addr) recv += Number(o.amount || 0);
    for (const i of (t.inputs || [])) if (i.previous_outpoint_address === addr) sent += Number(i.previous_outpoint_amount || 0);
    return recv - sent;
  };
  const txRows = txList.map(t => {
    const net = netFor(t), inb = net >= 0;
    const amt = `<span class="coin" style="color:${inb ? "var(--ok,#34d399)" : "var(--bad,#f87171)"}">${inb ? "+" : "−"}${coin(Math.abs(net))} ${SYMBOL}</span>`;
    return `<tr><td>${linkTx(t.transaction_id)}</td>
      <td>${esc(dt(t.block_time))} <span class="dim">(${ago(t.block_time)})</span></td>
      <td><span class="pill ${inb ? "chain" : ""}">${inb ? "received" : "sent"}</span></td>
      <td class="num">${amt}</td></tr>`;
  }).join("");
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › Address</div>
    <h1 class="page">Address</h1>
    <div class="kv">${rows.map(r=>`<div class="row"><div class="key">${r[0]}</div><div class="val">${r[1]}</div></div>`).join("")}</div>
    <h2 class="sec">Transactions${txCount != null ? ` (${txCount})` : ""}</h2>
    ${txList.length ? `<table class="tbl"><thead><tr><th>Tx id</th><th>Time</th><th>Direction</th><th class="num">Amount</th></tr></thead><tbody>${txRows}</tbody></table>${txCount != null && txCount > txList.length ? `<div class="note">Showing the latest ${txList.length} of ${txCount}.</div>` : ""}`
      : `<div class="note">No indexed transactions for this address yet. The indexer may still be catching up — recent transactions appear here within minutes.</div>`}
    <h2 class="sec">Unspent outputs (${utxoTotal != null ? num(utxoTotal) : num(entries.length)})</h2>
    ${utxoPartial ? `<div class="note">This address has more unspent outputs than the node will return in one response, so only the first ${num(entries.length)} are listed (<span class="mono">getUtxosByAddressPage</span>). The balance above is the full, exact total.</div>` : ""}
    ${entries.length?`<table class="tbl"><thead><tr><th>Outpoint (tx id)</th><th class="num">Index</th><th class="num">Amount</th><th class="num">DAA</th><th>Coinbase</th></tr></thead><tbody>${
      entries.map(e=>{const op=e.outpoint||{};const u=e.utxoEntry||{};return `<tr>
        <td>${linkTx(op.transactionId)}</td><td class="num">${num(op.index||0)}</td>
        <td class="num coin">${coin(u.amount)} ${SYMBOL}</td><td class="num">${num(u.blockDaaScore)}</td>
        <td>${u.isCoinbase?'<span class="pill blue">yes</span>':'no'}</td></tr>`;}).join("")
    }</tbody></table>`:(utxoErr?`<div class="err">Unspent outputs unavailable — ${esc(utxoErr)}</div>`:`<div class="note">No unspent outputs.</div>`)}`;
}

/* ---------------------- kaspa-pq OVERLAY (PoS) ------------------------- */
// Throttled refresh of the mesh-peer count + on-chain overlay scan. Decoupled from the
// 1.5s block poll so the cheap stats stay snappy while this heavier work runs ~every 8s.
let overlayBusy = false;
async function maybeRefreshOverlay(sink, anchor){
  if (overlayBusy) return;
  if (Date.now() - overlayStats.ts < OVERLAY_REFRESH_MS) return;
  overlayBusy = true;
  try {
    // **Count nodes from every vantage we have, not from one node's degree.**
    //
    // This used to be `seedRpc("getConnectedPeerInfo").length + 1` — the number of peers ONE node
    // happens to hold open. That is a degree, not a census, and on the previous network the node it asked was
    // the worst possible witness: it runs with `--nodnsseed` and two `--addpeer` entries, only one
    // of which is reachable, so it reported 1 peer and the site said "Nodes 2" while the network
    // had nine. Most participants sit behind NAT — they dial out to a public node and accept no
    // inbound — so a node's own degree systematically undercounts by however many of its siblings
    // cannot be dialled.
    //
    // Union the peer sets of every endpoint the site already talks to and count DISTINCT node
    // identities plus the vantage nodes themselves. An address is not a node identity here:
    // several kaspad processes can share one public IP, and one process can appear on loopback
    // and its public address at the same time. Still a lower bound — nothing here crawls — so the
    // label says "observed", not "are".
    try {
      const seen = new Set();
      // Distinct vantage NODES, by the node's own p2p id — several of these paths tunnel to the
      // same node, and counting endpoints is what dropped the +1 below.
      const vantageNodes = new Set();
      let vantages = 0;
      for (const call of [hubRpc, seedRpc, rpc]) {
        try {
          const sp = await call("getConnectedPeerInfo");
          const list = sp.peerInfo || [];
          vantages++;
          try {
            const gi = await call("getInfo");
            vantageNodes.add(gi && gi.p2pId ? `id:${gi.p2pId}` : "vantage-" + vantages);
          } catch { vantageNodes.add("vantage-" + vantages); }
          for (const pi of list) {
            const key = peerNodeKey(pi);
            if (key) seen.add(key);
          }
        } catch {}
      }
      // A vantage does not list itself. Add each distinct queried node exactly once; if it also
      // appears in another vantage's peer list, the identity set naturally collapses that copy.
      const nodes = new Set([...seen, ...vantageNodes]);
      overlayStats.meshPeers = vantages ? nodes.size : null;
      overlayStats.meshVantages = vantageNodes.size || vantages;
    } catch {}
    // The tip header names the consensus algorithm this chain is actually running (6 = PALW). It
    // is read here rather than in the 1.5 s poll because it changes once per network, not per block.
    try {
      const tb = await rpc("getBlock", { hash: sink, includeTransactions: false });
      const h = (tb && tb.block && tb.block.header) || {};
      const a = Number(h.powAlgoId);
      if (Number.isFinite(a)) overlayStats.powAlgoId = a;
    } catch {}
    // **The lane mix, not the tip's algo** (route-matrix #8): one header says which lane won ONE
    // block. The node counts the selected chain's recent blocks by lane (getPalwNodeStatus v3), and
    // a chain living on heartbeats alone is the one fact the tip's algo cannot show.
    try {
      const ns = await palwRead("getPalwNodeStatus", {}, 8000);
      if (ns && !ns.__unsupported && !ns.__error && Number(ns.laneWindowBlocks) > 0) {
        overlayStats.lane = {
          window: Number(ns.laneWindowBlocks), work: Number(ns.laneWorkBlocks), heartbeat: Number(ns.laneHeartbeatBlocks),
          lastWorkDaa: Number(ns.laneLastWorkDaa), mix: String(ns.laneMix || ""), alarm: String(ns.laneAlarm || ""),
        };
      } else if (ns && !ns.__error) {
        // A node that ANSWERED without a lane window (pre-v3 build, or one that has not taken its
        // first mix yet) must not keep showing a mix read from an earlier node: fall back to the tip.
        overlayStats.lane = null;
      }
    } catch {}
    await scanOverlay(anchor || sink);
  } catch {}
  finally { overlayStats.ts = Date.now(); overlayBusy = false; }
}

// Persistent set of bond outpoints ("txid:index") ever discovered, so a bond that has scrolled
// out of the recent getBlocks window is NOT forgotten (the bond tx is mined once but the bond
// stays active for thousands of blocks). Survives reloads via localStorage.
const BONDS_LS_KEY = "msk_bonds_v1";
function loadKnownBonds(){
  try { const a = JSON.parse(localStorage.getItem(BONDS_LS_KEY) || "[]"); if (Array.isArray(a)) return new Set(a); } catch {}
  return new Set();
}
function saveKnownBonds(set){ try { localStorage.setItem(BONDS_LS_KEY, JSON.stringify([...set])); } catch {} }
let knownBondOutpoints = loadKnownBonds();   // "txid:index"

// Parse the borsh header of a StakeAttestationShardPayload (hex). Layout (consensus
// dns_finality.rs, proven against live data): version u16 | epoch u64 | target_hash[64] |
// target_daa_score u64 | validator_set_commitment[64] | n_attestations u32 | then per
// attestation: version u16 | validator_id[64] | ... . We only need the shard header + the
// first attestation's validator_id — all fixed-offset, no full borsh decode needed.
function parseAttestationShard(payloadHex){
  try {
    if (!payloadHex || payloadHex.length < (2+8+64+8+64+4+2+64)*2) return null;
    const b = []; for (let i=0;i<payloadHex.length;i+=2) b.push(parseInt(payloadHex.substr(i,2),16));
    let o = 0;
    const u16 = () => { const v = b[o] | (b[o+1]<<8); o+=2; return v>>>0; };
    const u32 = () => { const v = (b[o]|(b[o+1]<<8)|(b[o+2]<<16)|(b[o+3]<<24))>>>0; o+=4; return v; };
    const u64 = () => { let v=0n; for(let i=0;i<8;i++) v |= BigInt(b[o+i])<<BigInt(8*i); o+=8; return v; }; // LE
    const h64 = () => { let s=""; for(let i=0;i<64;i++) s += b[o+i].toString(16).padStart(2,"0"); o+=64; return s; };
    const version = u16();
    const epoch = u64();
    const targetHash = h64();
    const targetDaa = u64();
    h64(); // validator_set_commitment (skip)
    const nAtt = u32();
    let validatorId = "", bondOutpoint = "";
    if (nAtt >= 1) {
      u16();                       // attestation[0].version
      validatorId = h64();         // attestation[0].validator_id (Hash64)
      const bondTxid = h64();      // attestation[0].bond_outpoint.transaction_id (Hash64)
      const bondIndex = u32();     // attestation[0].bond_outpoint.index (u32)
      bondOutpoint = bondTxid + ":" + bondIndex;
    }
    if (version !== 1) return null;
    return { epoch: epoch.toString(), targetHash, targetDaa: targetDaa.toString(), nAtt, validatorId, bondOutpoint };
  } catch { return null; }
}

// Rolling, persisted log of recent attestations (which block each validator "scored"): keyed by
// targetHash+validatorId so re-scans don't duplicate. Capped + newest-first.
const ATT_LS_KEY = "msk_attlog_v1";
const ATT_LOG_LIMIT = 60;
function loadAttLog(){ try { const a = JSON.parse(localStorage.getItem(ATT_LS_KEY)||"[]"); return Array.isArray(a)?a:[]; } catch { return []; } }
function saveAttLog(a){ try { localStorage.setItem(ATT_LS_KEY, JSON.stringify(a.slice(0, ATT_LOG_LIMIT))); } catch {} }
let attLog = loadAttLog();

// Recognize overlay txs in the recent DAG window AND re-verify every bond ever seen against live
// node state. Active-validator count is whichever is larger of (verified-active bonds) and the
// node's own rollout signal (rolloutStage===Active means >= min_active_validators are bonded —
// authoritative even when the one-time bond tx has long scrolled past the scan window).
async function scanOverlay(anchor){
  // 0) kaspa-pq server-side bond list: fetch EVERY stake-bond outpoint the indexer knows
  // (subnetwork txs + seeded pruned bonds) so validators that predate this browser's scan
  // window — or are pruned from the node — are still counted. Each is re-verified via
  // getStakeBond below, so stale/unbonded entries drop out. See rest-server /info/stake-bonds.
  try {
    const _r = await fetch("/info/stake-bonds", { cache: "no-store" });
    if (_r.ok) { const _j = await _r.json(); for (const _op of (_j.bondOutpoints || [])) { if (/^[0-9a-f]{128}:\d+$/.test(_op)) knownBondOutpoints.add(_op); } }
  } catch {}
  // 1) sweep the recent window for fresh bond/attestation txs (subnetwork id)
  let attShards = 0; const seenKeys = new Set(attLog.map(a => a.key)); let attLogChanged = false;
  let maxSweptDaa = 0;   // highest DAA in the recent window ≈ current tip; used to prune dead-chain attestations
  if (anchor){
    try {
      const gb = await rpc("getBlocks", { lowHash: anchor, includeBlocks: true, includeTransactions: true });
      for (const blk of (gb.blocks || [])){
        const blkHash = blk.header && blk.header.hash;
        const blkTime = blk.header && blk.header.timestamp;
        maxSweptDaa = Math.max(maxSweptDaa, Number((blk.header && blk.header.daaScore) || 0));
        for (const t of (blk.transactions || [])){
          const k = txKind(t);
          if (k.key === "bond"){ const id = t.verboseData && t.verboseData.transactionId; if (id) knownBondOutpoints.add(id + ":0"); }
          else if (k.key === "att"){
            attShards++;
            const a = parseAttestationShard(t.payload);
            if (a && a.targetHash){
              // Each attestation carries its bond_outpoint — and attestations recur EVERY epoch,
              // so this discovers the live bond even when the one-time bond tx scrolled out of the
              // window (fixes "Staked —" for browsers that never saw the bond tx). getStakeBond
              // then resolves the amount below.
              if (a.bondOutpoint && /^[0-9a-f]{128}:\d+$/.test(a.bondOutpoint)) knownBondOutpoints.add(a.bondOutpoint);
              const key = a.targetHash + ":" + a.validatorId + ":" + a.epoch;
              if (!seenKeys.has(key)){
                seenKeys.add(key); attLogChanged = true;
                attLog.unshift({ key, epoch: a.epoch, targetHash: a.targetHash, targetDaa: a.targetDaa,
                                 validatorId: a.validatorId, nAtt: a.nAtt, inBlock: blkHash, ts: blkTime });
              }
            }
          }
        }
      }
    } catch {}
  }
  // Prune attestations whose attested-anchor DAA sits beyond the current chain's tip — leftovers
  // from a previous (longer) chain after a devnet re-genesis. targetDaa always lags the tip, so
  // anything materially above the recent window's max DAA is stale.
  if (maxSweptDaa > 0){
    const before = attLog.length;
    attLog = attLog.filter(a => Number(a.targetDaa) <= maxSweptDaa + 10000);
    if (attLog.length !== before) attLogChanged = true;
  }
  // Always present newest-anchor-first (by attested DAA), independent of localStorage insertion order.
  attLog.sort((a, b) => Number(b.targetDaa) - Number(a.targetDaa));
  if (attLogChanged){ attLog = attLog.slice(0, ATT_LOG_LIMIT); saveAttLog(attLog); }
  overlayStats.attLog = attLog;
  saveKnownBonds(knownBondOutpoints);

  // 2) re-verify EVERY known bond against live node state (getStakeBond is by-outpoint, O(bonds))
  const bonds = []; const validators = new Set(); let totalStaked = 0;
  for (const op of [...knownBondOutpoints].slice(0, 200)){
    try {
      const b = await rpc("getStakeBond", { bondOutpoint: op });
      if (b && b.available){
        const txid = op.split(":")[0];
        bonds.push({ txid, validatorId: b.validatorId, amount: Number(b.amount||0),
                     activationDaaScore: b.activationDaaScore, status: b.effectiveStatus });
        if (b.effectiveStatus === "active"){ validators.add(b.validatorId); totalStaked += Number(b.amount||0); }
      } else if (b && b.available === false){
        knownBondOutpoints.delete(op);   // bond no longer exists (e.g. devnet re-genesis) — forget it
      }
    } catch {}
  }
  saveKnownBonds(knownBondOutpoints);

  // 3) reconcile with the node's rollout signal: rolloutStage===2 (Active) guarantees the network
  //    has >= min_active_validators (>=1) bonded right now, even if we haven't (yet) discovered the
  //    bond outpoint in a scan window. Never under-report below what the node asserts.
  let activeValidators = validators.size;
  try {
    const dns = await rpc("getDnsConfirmation");
    if (dns && Number(dns.rolloutStage) === 2 && activeValidators < 1) activeValidators = 1;
    overlayStats.rolloutActive = dns ? Number(dns.rolloutStage) === 2 : false;
  } catch {}

  overlayStats.bonds = bonds;
  overlayStats.activeValidators = activeValidators;
  overlayStats.totalStaked = totalStaked;
  overlayStats.attShards = attShards;
}

async function renderOverlay(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Overlay</div>
    <h1 class="page">PoS overlay <span class="dim" style="font-size:14px">(kaspa-pq staking &amp; DNS finality)</span></h1>
    <div class="loading">Loading overlay state…</div>`;
  // fresh DNS + local validator status; reuse cached bonds (kick a scan if stale)
  let dns=null, vs=null, dag=null;
  try { dag = await rpc("getBlockDagInfo"); } catch {}
  try { dns = await rpc("getDnsConfirmation"); } catch {}
  try { vs  = await rpc("getValidatorStatus"); } catch {}
  if (dag) { overlayStats.ts = 0; await maybeRefreshOverlay(dag.sink, recentLow || dag.sink); }

  const dnsRows = dns ? [
    ["Available", dns.available ? "yes" : "no"],
    ["Rollout stage", `${ROLLOUT_STAGES[dns.rolloutStage]||dns.rolloutStage}`],
    ["Health", `${DNS_HEALTH[dns.health]||dns.health}`],
    ["PoW confirmed", dns.powConfirmed ? '<span class="pill blue">yes</span>' : "no"],
    ["DNS confirmed", dns.dnsConfirmed ? '<span class="pill chain">yes</span>' : '<span class="pill">no</span>'],
    ["Anchor block", dns.blockHash ? linkBlock(dns.blockHash) : "—"],
    ["Work depth", `${num(dns.workDepth)} / ${num(dns.requiredWorkDepth)} req`],
    ["Stake depth", `${esc(dns.stakeDepth)} / ${esc(dns.requiredStakeDepth)} req`],
    ["Note", `<span class="dim">${esc(dns.note||"")}</span>`],
  ] : [["DNS finality", '<span class="dim">unavailable</span>']];

  const vsActive = vs && vs.enabled;
  const vsRows = vsActive ? [
    ["Enabled", "yes"],
    ["Mode", esc(vs.mode||"—")],
    ["Validator id", vs.validatorId ? copyable(vs.validatorId) : "—"],
    ["Funding address", vs.fundingAddress ? linkAddr(vs.fundingAddress) : "—"],
    ["Bond status", esc(vs.bondStatus||"—")],
    ["Active validator", vs.isActiveValidator ? "yes" : "no"],
    ["Epoch", num(vs.epoch)],
    ["Last signed epoch", num(vs.lastSignedEpoch)],
    ["Status", esc(vs.statusLabel||vs.status)],
  ] : null;

  const bonds = overlayStats.bonds || [];
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › Overlay</div>
    <h1 class="page">PoS overlay <span class="dim" style="font-size:14px">(kaspa-pq staking &amp; DNS finality)</span></h1>

    <div class="cards">
      <div class="card"><div class="k">Active validators</div><div class="v ov">${num(overlayStats.activeValidators)}</div><div class="sub">${bonds.length ? bonds.length+" bonds seen" : (overlayStats.rolloutActive ? "rollout Active (bond off scan window)" : "0 bonds")}</div></div>
      <div class="card"><div class="k">Total staked</div><div class="v ov sm">${bonds.length ? coin(overlayStats.totalStaked)+" "+SYMBOL : "—"}</div><div class="sub">${bonds.length ? "active bonds" : "bond tx off-window"}</div></div>${"".concat()}
      <div class="card"><div class="k">Attestation shards</div><div class="v sm">${num(overlayStats.attShards)}</div><div class="sub">recent window</div></div>
      <div class="card"><div class="k">DNS finality</div><div class="v sm">${dns?(dns.dnsConfirmed?"confirmed":(dns.powConfirmed?"PoW only":"pending")):"n/a"}</div><div class="sub">${dns?(ROLLOUT_STAGES[dns.rolloutStage]||""):""}</div></div>
    </div>

    <h2 class="sec">DNS finality (ADR-0009)</h2>
    <div class="kv">${dnsRows.map(r=>`<div class="row"><div class="key">${r[0]}</div><div class="val">${r[1]}</div></div>`).join("")}</div>

    ${vsRows ? `<h2 class="sec">This node's validator</h2>
      <div class="kv">${vsRows.map(r=>`<div class="row"><div class="key">${r[0]}</div><div class="val">${r[1]}</div></div>`).join("")}</div>`
      : `<h2 class="sec">This node's validator</h2><div class="note">This node is not running the validator service (<span class="mono">--enable-validator</span> off). Validator/stake figures above are derived from on-chain bond &amp; attestation transactions.</div>`}

    <h2 class="sec">Stake bonds (${bonds.length})</h2>
    ${bonds.length ? `<table class="tbl"><thead><tr><th>Bond (tx id)</th><th>Validator id</th><th class="num">Amount</th><th class="num">Activation DAA</th><th>Status</th></tr></thead><tbody>${
      bonds.map(b=>`<tr>
        <td>${linkTx(b.txid)}</td>
        <td class="hash">${esc(short(b.validatorId,10))}</td>
        <td class="num coin">${coin(b.amount)} ${SYMBOL}</td>
        <td class="num">${num(b.activationDaaScore)}</td>
        <td>${b.status==="active"?'<span class="pill chain">active</span>':`<span class="pill">${esc(b.status||"—")}</span>`}</td></tr>`).join("")
    }</tbody></table>` : `<div class="note">No stake bonds found in the recent block window. When a validator bonds stake (subnetwork <span class="mono">10..</span>) and attests (subnetwork <span class="mono">11..</span>), they appear here.</div>`}

    <h2 class="sec">Recent attestations <span class="dim" style="font-size:13px">(which block each validator scored)</span></h2>
    <div class="note" style="margin-bottom:8px">Each row is a validator attestation shard (subnetwork <span class="mono">11..</span>): the validator signs an
      <b>approval</b> of a selected-chain anchor block, advancing that block's <b>StakeScore</b> toward DNS finality
      (ADR-0009). "Scored block" is the attested anchor; "epoch" is the attestation epoch the shard signs for.</div>
    ${(overlayStats.attLog && overlayStats.attLog.length) ? `<table class="tbl"><thead><tr>
        <th class="num">Epoch</th><th>Scored block (anchor)</th><th class="num">Target DAA</th>
        <th>Validator</th><th class="num">#att</th><th class="right">Age</th></tr></thead><tbody>${
      overlayStats.attLog.map(a=>`<tr>
        <td class="num">${esc(a.epoch)}</td>
        <td>${linkBlock(a.targetHash)}</td>
        <td class="num">${num(a.targetDaa)}</td>
        <td class="hash" title="${esc(a.validatorId)}">${esc(short(a.validatorId,10))}</td>
        <td class="num">${num(a.nAtt)}</td>
        <td class="right dim" title="${esc(dt(a.ts))}">${a.ts?ago(a.ts):"—"}</td></tr>`).join("")
    }</tbody></table>` : `<div class="note">No attestations seen in the recent block window yet. A bonded validator submits an attestation shard roughly every epoch; they will appear here as blocks arrive (newest first, last ${ATT_LOG_LIMIT} kept).</div>`}`;
}

/* ----------------------- DNS FINALITY (ordered feed) ------------------- */
// Live, ordered list of blocks that have passed DNS finality: the selected-chain
// blocks at or below the stake-confirmed anchor (ADR-0009). Newest first; as the
// anchor advances, newly-finalized blocks are prepended (and briefly highlighted).
let finalFeed = [];            // newest-first: {hash, daa, blue, ts, isChain}
let finalKnown = new Set();    // lowercase hashes already in finalFeed
let finalAnchor = null;        // last anchor hash we walked back from
let finalBusy = false;         // re-entry guard for the poll
const FINAL_FEED_LIMIT = 150;  // cap the rendered feed
const FINAL_WALK_MAX = 60;     // max blocks to walk per refresh

// Walk the selected chain backward from `fromHash`, collecting blocks until we reach
// `stopAt` (exclusive), run out of chain, or hit `limit`. Returns newest-first. Every
// block on this walk is a selected-chain block at/below the anchor ⇒ DNS-final.
async function walkChainBack(fromHash, stopAt, limit){
  const out = []; let cur = fromHash;
  const stop = stopAt ? String(stopAt).toLowerCase() : null;
  for (let i=0; i<limit && cur && /[1-9a-f]/i.test(cur); i++){
    if (stop && String(cur).toLowerCase() === stop) break;
    let r; try { r = await rpc("getBlock", { hash: String(cur).toLowerCase(), includeTransactions:false }); } catch { break; }
    if (!r || !r.block) break;
    const hd = r.block.header, vd = r.block.verboseData || {};
    // mergeSet = {selectedParent} ∪ {parallel blocks merged}; the selected parent is mergeSetBlues[0]
    // and is itself the next chain block — so parallel (non-chain) blocks absorbed = mergeset − 1.
    // DAA counts every DAG block, hence daa(B) − daa(selectedParent) = 1 + (parallel merged).
    const merges = Math.max(0, (vd.mergeSetBluesHashes||[]).length + (vd.mergeSetRedsHashes||[]).length - 1);
    out.push({ hash: hd.hash, daa: Number(hd.daaScore||0), blue: Number(hd.blueScore||0), ts: Number(hd.timestamp||0), isChain: !!vd.isChainBlock, merges });
    cur = vd.selectedParentHash;
  }
  return out;
}

async function renderFinality(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  finalFeed = []; finalKnown = new Set(); finalAnchor = null;
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › DNS Finality</div>
    <h1 class="page">DNS-final blocks <span class="dim" style="font-size:14px">(stake-confirmed, in order)</span></h1>
    <div class="cards" id="finalCards"><div class="loading">Loading DNS-final chain…</div></div>
    <div class="note" style="margin:10px 0">Every block below sits on the selected chain at or below the stake-confirmed anchor — <b>irreversible under DNS finality</b> (ADR-0009). Newest first; the anchor advances as validators attest. This is the <b>selected-chain backbone</b>: <b>Merged</b> counts the parallel blocks each chain block absorbs but which aren't on the chain. Those merged blocks are finalized too, as part of the chain's past. On testnet-12 DAA is the chain's clock (about one step per 120 s), not a block count, so it does not step by the number of blocks between rows. Verified per-block via <span class="mono">getDnsConfirmation(blockHash)</span>.</div>
    <table class="tbl"><thead><tr>
      <th class="num">DAA</th><th class="num">Blue score</th><th class="num">Merged</th><th>Block</th><th>Age</th><th class="num">Attestations</th><th>Status</th>
    </tr></thead><tbody id="finalBody"><tr><td colspan="7" class="spin">Walking the selected chain…</td></tr></tbody></table>`;
  await refreshFinality();
  armPoll(()=>{ const s=curSeg(); return s==="finality"||s==="dns"; }, refreshFinality, 3000);
}

async function refreshFinality(){
  if (finalBusy) return; finalBusy = true;
  try {
    const cards = document.getElementById("finalCards"), body = document.getElementById("finalBody");
    if (!cards || !body) return;   // navigated away mid-flight
    let dns=null, dag=null;
    try { dns = await rpc("getDnsConfirmation"); } catch {}
    try { dag = await rpc("getBlockDagInfo"); } catch {}
    if (dag) { try { await maybeRefreshOverlay(dag.sink, recentLow || dag.sink); } catch {} }

    const anchor = dns && dns.lastDnsConfirmedAnchor ? String(dns.lastDnsConfirmedAnchor).toLowerCase() : "";
    const anchorOk = !!(anchor && /[1-9a-f]/i.test(anchor) && dns && dns.dnsConfirmed);
    if (!anchorOk){
      cards.innerHTML = `<div class="card"><div class="k">DNS finality</div><div class="v sm">${dns?(dns.powConfirmed?"PoW only":"pending"):"n/a"}</div><div class="sub">${dns?esc(DNS_HEALTH[dns.health]||dns.health):""}</div></div>`;
      body.innerHTML = `<tr><td colspan="7" class="dim">DNS finality is not confirming yet${dns?` — stage ${esc(ROLLOUT_STAGES[dns.rolloutStage]||dns.rolloutStage)}`:""}. Blocks become DNS-final once a bonded validator's attestations advance a selected-chain anchor past the required StakeScore (see <a href="#/overlay">PoS overlay</a>). On testnet-12 DNS finality activates only once at least 6 validators hold at least 120,000,000 ${SYMBOL} of active stake; validators are funded after launch. Until then, treat a payment as final only at finality depth (600 blue, about 4 hours) or once a <b>Final</b> PALW anchor covers it (<span class="mono">getPalwSettlement</span>).</td></tr>`;
      finalAnchor = null;
      return;
    }

    const anchorDaa = Number(dns.lastDnsConfirmedAnchorDaaScore||0);
    const fresh = new Set();
    if (!finalFeed.length){
      const seed = await walkChainBack(anchor, null, FINAL_WALK_MAX);
      finalFeed = seed; finalFeed.forEach(b => finalKnown.add(b.hash.toLowerCase()));
    } else if (anchor !== finalAnchor){
      const added = await walkChainBack(anchor, finalAnchor, FINAL_WALK_MAX);
      const news = added.filter(b => !finalKnown.has(b.hash.toLowerCase()));
      news.forEach(b => { finalKnown.add(b.hash.toLowerCase()); fresh.add(b.hash.toLowerCase()); });
      finalFeed = news.concat(finalFeed).slice(0, FINAL_FEED_LIMIT);
    }
    finalAnchor = anchor;

    const attBy = {};
    (attLog||[]).forEach(a => { const k=String(a.targetHash||"").toLowerCase(); if(k) attBy[k]=(attBy[k]||0)+(Number(a.nAtt)||1); });
    const sink = dag ? Number(dag.virtualDaaScore||0) : 0;
    const lag = (sink && anchorDaa) ? Math.max(0, sink-anchorDaa) : 0;

    cards.innerHTML = `
      <div class="card"><div class="k">Confirmed anchor</div><div class="v sm">${linkBlock(anchor)}</div><div class="sub">finality frontier</div></div>
      <div class="card"><div class="k">Anchor DAA</div><div class="v">${num(anchorDaa)}</div><div class="sub">DNS-final ≤ this</div></div>
      <div class="card"><div class="k">Lag from tip</div><div class="v sm">${num(lag)} DAA</div><div class="sub">sink @ ${num(sink)}</div></div>
      <div class="card"><div class="k">Health</div><div class="v sm">${esc(DNS_HEALTH[dns.health]||dns.health)}</div><div class="sub">${esc(ROLLOUT_STAGES[dns.rolloutStage]||dns.rolloutStage)} · ${num(finalFeed.length)} shown</div></div>`;

    body.innerHTML = finalFeed.map(b => {
      const isA = b.hash.toLowerCase() === anchor;
      const att = attBy[b.hash.toLowerCase()] || 0;
      return `<tr class="${fresh.has(b.hash.toLowerCase())?'fresh':''}">
        <td class="num">${num(b.daa)}</td>
        <td class="num">${num(b.blue)}</td>
        <td class="num" title="parallel DAG blocks this chain block merged (absorbed but not on the selected chain)">${b.merges?num(b.merges):'<span class="dim">0</span>'}</td>
        <td>${linkBlock(b.hash)}</td>
        <td class="dim" title="${esc(dt(b.ts))}">${b.ts?ago(b.ts):"—"}</td>
        <td class="num">${att?num(att):'<span class="dim">—</span>'}</td>
        <td>${isA?'<span class="pill blue">confirmed anchor</span>':'<span class="pill chain">DNS-final</span>'}</td></tr>`;
    }).join("") || `<tr><td colspan="7" class="dim">No DNS-final blocks yet.</td></tr>`;
  } finally { finalBusy = false; }
}

/* ----------------------- kaspa-pq EVM LANE (ADR-0020) ------------------ */
// EVM transaction view: receipt (acceptance/execution) + inclusion status (DA tier) + logs.
// Reads the node's misaka_* EVM RPCs over the same wRPC-JSON socket as every other view.
async function renderEvmTx(hash){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  const h = String(hash||"").replace(/^0x/i,"").toLowerCase();
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › <a href="#/evm">EVM Lane</a> › Transaction</div>
    <h1 class="page">EVM transaction</h1><div class="loading">Looking up EVM transaction…</div>`;
  if (!/^[0-9a-f]{64}$/.test(h)) return showErrFor(__g, "Not a valid EVM transaction hash (expected a 32-byte / 64-hex value).", hash);
  let rc=null, st=null;
  try { rc = await rpc("getEvmTransactionReceipt", { transactionHash: h }); } catch {}
  try { st = await rpc("getEvmTxInclusionStatus", { transactionHash: h }); } catch {}
  if (!rc && !st) return showErrFor(__g, "EVM lookup failed — the node is unreachable or the EVM lane is not active on this network.", "0x"+h);
  const found    = !!(rc && rc.found);
  const included = (st && st.includedIn) || [];
  const accepted = !!(st && st.acceptedIn && /[1-9a-f]/.test(String(st.acceptedIn)));
  const pending  = !!(st && st.pending);
  let statusPill;
  if (found && rc.succeeded)        statusPill = '<span class="pill chain">accepted · success</span>';
  else if (found && !rc.succeeded)  statusPill = '<span class="pill slash">accepted · reverted</span>';
  else if (included.length && pending) statusPill = '<span class="pill att">included (DA) · pending acceptance</span>';
  else if (included.length)         statusPill = '<span class="pill att">included (DA)</span>';
  else if (pending)                 statusPill = '<span class="pill">pending in mempool</span>';
  else if (st && st.lastSkipClass)  statusPill = `<span class="pill slash">skipped — ${esc(EVM_SKIP_CLASS[st.lastSkipClass]||("class "+st.lastSkipClass))}</span>`;
  else                              statusPill = '<span class="pill">not found</span>';
  const rows = [
    ["EVM tx hash", copyable("0x"+h)],
    ["Status", statusPill],
  ];
  if (found){
    rows.push(["Accepting block", rc.acceptingBlock ? linkBlock(rc.acceptingBlock) : "—"]);
    rows.push(["EVM block number", num(rc.evmNumber)]);
    rows.push(["Receipt index", num(rc.receiptIndex)]);
    rows.push(["Succeeded", rc.succeeded ? '<span class="pill chain">yes</span>' : '<span class="pill slash">no (reverted / out-of-gas)</span>']);
    rows.push(["Gas used", `${num(rc.gasUsed)} <span class="dim">(cumulative ${num(rc.cumulativeGasUsed)})</span>`]);
  }
  if (st){
    rows.push(["Pending (mempool)", pending ? "yes" : "no"]);
    rows.push(["Accepted in", accepted ? linkBlock(st.acceptedIn) : '<span class="dim">not yet accepted</span>']);
    if (st.lastSkipClass) rows.push(["Last skip class", esc(EVM_SKIP_CLASS[st.lastSkipClass]||st.lastSkipClass)]);
  }
  const logs = (rc && rc.logs) || [];
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › <a href="#/evm">EVM Lane</a> › Tx ${esc(short("0x"+h,8))}</div>
    <h1 class="page">EVM transaction <span class="hash" style="font-size:14px;color:var(--mut)">${esc(short("0x"+h,14))}</span></h1>
    <div class="kv">${rows.map(r=>`<div class="row"><div class="key">${r[0]}</div><div class="val">${r[1]}</div></div>`).join("")}</div>
    <h2 class="sec">Included in <span class="dim" style="font-size:13px">(data availability — inclusion ≠ execution)</span></h2>
    ${included.length ? `<div class="note" style="margin-bottom:8px">Payload blocks carrying this transaction's bytes. Under mergeset delayed acceptance the tx executes when a selected child accepts the mergeset — see <b>Accepting block</b> above.</div>
      <table class="tbl"><thead><tr><th>Payload block</th></tr></thead><tbody>${included.map(b=>`<tr><td>${linkBlock(b)}</td></tr>`).join("")}</tbody></table>`
      : `<div class="note">Not yet included in a payload block${pending?" — still pending in the mempool":""}.</div>`}
    <h2 class="sec">Event logs (${logs.length})</h2>
    ${logs.length ? `<table class="tbl"><thead><tr><th>Address</th><th>Topics</th><th>Data</th></tr></thead><tbody>${
      logs.map(l=>`<tr><td class="hash">${esc(l.address||"")}</td>
        <td class="hash" style="font-size:11px">${(l.topics||[]).map(t=>esc(short(t,10))).join("<br>")||"—"}</td>
        <td class="hash" style="font-size:11px">${esc(short(l.data||"",20))||"—"}</td></tr>`).join("")
    }</tbody></table>` : `<div class="note">No event logs emitted.</div>`}`;
}

// EVM lane overview: live status from the sink's v2 commitments, lane parameters,
// a tx-hash lookup box, and recent payload-bearing blocks (heuristic on payload size).
async function renderEvmLane(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › EVM Lane</div>
    <h1 class="page">EVM lane <span class="dim" style="font-size:14px">(ADR-0020 — selected-parent EVM execution on L1)</span></h1>
    <div class="loading">Loading EVM lane state…</div>`;
  let dag=null, sinkBlk=null;
  try { dag = await rpc("getBlockDagInfo"); } catch {}
  if (dag && dag.sink){ try { sinkBlk = await rpc("getBlock", { hash: dag.sink, includeTransactions:false }); } catch {} }
  const hd = sinkBlk && sinkBlk.block && sinkBlk.block.header;
  const active = !!(hd && hd.evmCommitmentRoot && /[1-9a-f]/.test(String(hd.evmCommitmentRoot)));
  // Scan the recent window for EVM-bearing blocks. getBlocks(lowHash) returns blocks FORWARD of
  // lowHash, so anchor on a block ~40 deep (walk selectedParentHash back from the sink) — anchoring
  // on the sink itself returns ~nothing (the earlier "1 scanned" bug). Decode each block's payload
  // and surface the actual EVM transactions, not a byte heuristic.
  let evmBlocks=[], recentTxs=[], recentClaims=[], scanned=0;
  if (dag && dag.sink){
    try {
      let anchor = dag.sink;
      for (let i=0; i<40; i++){
        const r = await rpc("getBlock", { hash: anchor, includeTransactions:false });
        const sp = r && r.block && r.block.verboseData && r.block.verboseData.selectedParentHash;
        if (!sp) break; anchor = sp;
      }
      const gb = await rpc("getBlocks", { lowHash: anchor, includeBlocks:true, includeTransactions:false });
      const seen = new Set();
      for (const b of (gb.blocks||[])){
        scanned++;
        const dec = decodeEvmPayload(b.evmPayload);
        const daa = Number(b.header.daaScore||0), ts = Number(b.header.timestamp||0);
        if (dec.txs.length){
          evmBlocks.push({ hash:b.header.hash, daa, ts, n:dec.txs.length });
          for (const t of dec.txs){ if (t.hash && !seen.has(t.hash)){ seen.add(t.hash); recentTxs.push({ hash:t.hash, type:t.type, block:b.header.hash, daa, ts }); } }
        }
        if (dec.claims && dec.claims.length){
          // Only a CHAIN block's payload executes (ADR-0020): a sibling that carried the same claim is
          // not a second credit, and a row that did not say so read as one (2026-09-11: c57eec8d twice).
          const chain = !!(b.verboseData && b.verboseData.isChainBlock);
          for (const c of dec.claims){ recentClaims.push({ outpoint:c.outpoint, evmAddress:c.evmAddress, amountSompi:c.amountSompi, tipSompi:c.tipSompi, block:b.header.hash, daa, ts, chain }); }
        }
      }
      evmBlocks.sort((a,b)=>b.daa-a.daa);
      recentTxs.sort((a,b)=>b.daa-a.daa);
      recentClaims.sort((a,b)=>b.daa-a.daa);
    } catch {}
  }
  // ADR-0020's flag day is DAA 0 on testnet-12 (`evm_activation_daa_score` = 0, the testnet base):
  // every block from genesis commits to its (possibly empty) EVM payload, so a sink header without
  // the commitment is not a lane waiting for its fence — it is worth saying so rather than printing
  // a countdown to a height the chain has already passed.
  let laneSub;
  if (active) {
    laneSub = "EVM commitments live (ADR-0020) · active from genesis";
  } else if (hd) {
    laneSub = "the sink header carries no EVM commitment — unexpected: testnet-12 runs the lane from genesis";
  } else {
    laneSub = "sink header unavailable";
  }
  const cards = [
    ["Lane status", active?'<span class="pill chain">active</span>':(hd?'<span class="pill">inactive</span>':"n/a"), laneSub, "ov"],
    ["Chain ID", num(EVM_CHAIN_ID), "0x4D534B · &quot;MSK&quot;", "sm"],
    ["Base fee", EVM_BASE_FEE_GWEI+" gwei", "EIP-1559 initial", "sm"],
    ["Payload cap", "128 KiB", "per DAG block", "sm"],
    ["Native unit", "1 MSK = 10¹⁸ wei", "L1 sompi × 10¹⁰", "sm"],
  ];
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › EVM Lane</div>
    <h1 class="page">EVM lane <span class="dim" style="font-size:14px">(ADR-0020 — selected-parent EVM execution on L1)</span></h1>
    <div class="cards">${cards.map(c=>`<div class="card"><div class="k">${c[0]}</div><div class="v ${c[3]||""}">${c[1]}</div>${c[2]?`<div class="sub">${c[2]}</div>`:""}</div>`).join("")}</div>
    <div class="txbox" style="margin-top:14px"><b>Look up an EVM transaction</b>
      <form id="evmLookup" autocomplete="off" style="display:flex;gap:8px;margin-top:8px">
        <input id="evmHash" type="text" placeholder="EVM tx hash (0x + 64 hex)" style="flex:1;min-width:0" />
        <button type="submit">Open</button>
      </form>
      <div class="dim" style="margin-top:6px">EVM transactions use 32-byte (0x + 64 hex) hashes — distinct from L1's 128-hex ids.</div>
    </div>
    <h2 class="sec">Selected-parent EVM commitments <span class="dim" style="font-size:13px">(sink header)</span></h2>
    ${hd ? `<div class="kv">
      <div class="row"><div class="key">Sink block</div><div class="val">${linkBlock(dag.sink)}</div></div>
      <div class="row"><div class="key">EVM payload hash</div><div class="val"><span class="hash">${esc(hd.evmPayloadHash||"—")}</span></div></div>
      <div class="row"><div class="key">EVM commitment root</div><div class="val"><span class="hash">${esc(hd.evmCommitmentRoot||"—")}</span></div></div>
    </div>` : `<div class="note">Sink header unavailable.</div>`}
    <h2 class="sec">Recent EVM transactions <span class="dim" style="font-size:13px">(decoded from payloads · ${num(scanned)} blocks scanned)</span></h2>
    ${recentTxs.length ? `<table class="tbl"><thead><tr><th>EVM tx hash</th><th>Type</th><th>In block</th><th class="num">DAA</th><th class="right">Age</th></tr></thead><tbody>${
      recentTxs.slice(0,40).map(t=>`<tr>
        <td>${linkEvmTx(t.hash)}</td>
        <td><span class="pill evm">${esc(evmTxTypeLabel(t.type))}</span></td>
        <td>${linkBlock(t.block)}</td>
        <td class="num">${num(t.daa)}</td>
        <td class="right dim" title="${esc(dt(t.ts))}">${ago(t.ts)}</td></tr>`).join("")
    }</tbody></table>${evmBlocks.length?`<div class="note" style="margin-top:8px">${num(recentTxs.length)} EVM transaction(s) across ${num(evmBlocks.length)} payload block(s) in the recent window.</div>`:""}`
      : `<div class="note">No EVM transactions in the recent block window. Blocks carry EVM transactions only while accounts are transacting — submit one (or use the lookup above) and it appears here. On testnet-12 the lane is active from genesis: every block commits to its (possibly empty) payload hash.${(typeof keccak256!=="function")?" <b>Note:</b> the keccak library did not load, so tx hashes can't be derived in-browser.":""}</div>`}
    <h2 class="sec">Recent bridge deposit-claims <span class="dim" style="font-size:13px">(UTXO→EVM credits · §9.2 system ops in payloads)</span></h2>
    ${recentClaims.length ? `<table class="tbl"><thead><tr><th>EVM address (credited)</th><th class="num">Amount (MSK)</th><th class="num">Tip</th><th>Lock outpoint</th><th>In block</th><th>Executed</th><th class="right">Age</th></tr></thead><tbody>${
      recentClaims.slice(0,40).map(c=>`<tr>
        <td><span class="mono">${esc(c.evmAddress)}</span></td>
        <td class="num">${coin(c.amountSompi)}</td>
        <td class="num dim">${coin(c.tipSompi)}</td>
        <td><span class="hash" title="${esc(c.outpoint)}">${esc(c.outpoint.slice(0,12))}…:${esc(c.outpoint.split(":")[1]||"0")}</span></td>
        <td>${linkBlock(c.block)}</td>
        <td>${c.chain ? `<span class="pill chain">credited</span>` : `<span class="pill" title="Only a chain block's payload executes; this block is not on the selected chain, so this copy of the claim credits nothing.">not executed · off the selected chain</span>`}</td>
        <td class="right dim" title="${esc(dt(c.ts))}">${ago(c.ts)}</td></tr>`).join("")
    }</tbody></table><div class="note" style="margin-top:8px">${num(recentClaims.length)} deposit-claim(s) in the recent window, ${num(recentClaims.filter(c=>c.chain).length)} executed by a chain block — a claim two producers both carried shows twice, and only the chain block's copy credits. Claims are §9.2 bridge system ops (UTXO→EVM credits), not Ethereum transactions, so they carry no 0x tx hash.</div>`
      : `<div class="note">No bridge deposit-claims in the recent block window. A claim appears here when a deposit-lock is claimed on a mining node (credits the destination EVM address). Producers carry a claim only while DNS finality is confirmed and keeping up with the tip; on testnet-12 DNS finality is in Bootstrap until its validators are funded, so deposit claims wait until then.</div>`}
    <div class="note" style="margin-top:12px"><b>Bridge:</b> UTXO→EVM via a deposit-lock output claimed on a mining node (credits the EVM address); EVM→UTXO via the <span class="mono">0x…F002</span> withdraw precompile, which materializes a synthetic UTXO at the destination. Native <b>MSK</b> is the EVM gas + value token (18 decimals).</div>
    ${evmLaneExtraSections()}`;
  const f = document.getElementById("evmLookup");
  if (f) f.addEventListener("submit",(e)=>{ e.preventDefault();
    const v = $("#evmHash").value.trim().toLowerCase().replace(/^0x/,"");
    if (/^[0-9a-f]{64}$/.test(v)) location.hash = "#/evmtx/"+v; else alert("Enter an EVM tx hash: 0x + 64 hex."); });
  // The two new sections load on their own (JSON-RPC and wRPC are independent sources) and keep
  // refreshing incrementally while the page stays open — a lock's status moves from "waiting" to
  // "credited" the moment a chain block carries its claim.
  fillEvmTxSection(__g); fillBridgeSection(__g);
  armPoll(() => curSeg() === "evm" && __g === routeGen, () => { fillEvmTxSection(__g); fillBridgeSection(__g); }, 30000);
}

/* ---------------- #/evm additions: EVM-lane transactions + the UTXO↔EVM bridge ---------------- */
// Two sections appended to the EVM lane page. Both read live sources only:
//   · EVM transactions — the node's Ethereum JSON-RPC (POST /evm): eth_blockNumber,
//     eth_getBlockByNumber [n, true] scanned backwards from the head in batches, and
//     eth_getTransactionReceipt for the status. Withdrawals (EVM→UTXO) are the txs whose `to`
//     is the 0x…F002 precompile; their calldata is `[spk version u16 BE][destination script]`.
//   · Bridge — every EVM_DEPOSIT_LOCK output found in the recent L1 window (getBlocks above an
//     anchor ~BRIDGE_WALK_BACK chain steps below the sink, chain AND non-chain blocks), joined
//     with the DepositClaim system ops carried by CHAIN blocks' payloads (only those execute),
//     the virtual chain's accepted-tx lists (getVirtualChainFromBlock), any L1 input spending
//     the lock (refund), and the explorer index (/transactions/{id}) as the fallback for a lock
//     whose acceptance the window cannot decide.
// Wire formats mirror crypto/txscript/src/script_class.rs (the lock) and consensus/core/src/evm
// (DepositClaim / F002). Everything between the @bridge-pure markers is DOM-free and I/O-free so
// the verification scripts can run the very same code.
/* @bridge-pure-begin */
const BRIDGE_WALK_BACK = 100;                 // selected-chain steps below the sink that anchor the L1 scan
const BRIDGE_NEAR_BACK = 30;                  // the first, fast pass covers this many chain steps; the rest streams in behind it
const BRIDGE_MAX_BLOCKS = 260;                // cap on blocks pulled per scan (chain + non-chain)
const BRIDGE_CACHE_BLOCKS = 700;              // in-memory summaries kept across incremental refreshes
const BRIDGE_UNKNOWN_GRACE_MS = 90 * 60 * 1000; // a lock the index cannot see after this long is "not accepted"
const BRIDGE_CHAIN_SHORTCUT_SPAN = 20000;     // one getVirtualChainFromBlock call replaces the walk while the chain is this short
const BRIDGE_PAGE_TIMEOUT_MS = 120000;        // a getBlocks page can be ~180 ML-DSA-signed blocks (several MB): measured 14–30 s live
const EVMTX_SCAN_BLOCKS = 300;                // how far back the EVM-block scan goes before giving up
const EVMTX_WANT = 25;                        // stop once this many txs are found
const EVMTX_BATCH = 8;                        // eth_getBlockByNumber calls per JSON-RPC batch
const EVMTX_PARALLEL = 3;                     // batches in flight at once
const EVM_WITHDRAW_PRECOMPILE = "0x000000000000000000000000000000000000f002";
const EVM_PRECOMPILE_LABELS = {
  "0x000000000000000000000000000000000000f001": { label: "WMISAKA (0x…F001)", title: "WMISAKA predeploy — the WETH9-equivalent wrapped native token" },
  "0x000000000000000000000000000000000000f002": { label: "Withdraw → UTXO", title: "0x…F002 withdraw precompile: burns EVM MSK and materializes a synthetic L1 UTXO at the destination script" },
  "0x000000000000000000000000000000000000f003": { label: "ML-DSA verify (0x…F003)", title: "0x…F003 ML-DSA-87 signature-verify precompile" },
};
const U64_MAX_HEX_LE = "ffffffffffffffff";
// little-endian u64 hex (16 chars) → Number (values here are DAA scores and sompi: < 2^53)
function leU64(hex16){ let v = 0; for (let i = 14; i >= 0; i -= 2) v = v * 256 + parseInt(hex16.slice(i, i + 2), 16); return v; }
function hexToByteArr(h){ const out = []; for (let i = 0; i + 1 < h.length; i += 2) out.push(parseInt(h.slice(i, i + 2), 16)); return out; }
// The Misaka address encoder (crypto/addresses/src/bech32.rs): cashaddr-style 5-bit groups with the
// 40-bit polymod checksum; version 2 = PubKeyHashMlDsa87 (a 64-byte BLAKE2b-512 of the ML-DSA key).
const MSK_B32 = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
function mskConv8to5(bytes){
  const out = []; let buff = 0, bits = 0;
  for (const c of bytes){ buff = ((buff << 8) | c) & 0xffff; bits += 8;
    while (bits >= 5){ bits -= 5; out.push((buff >> bits) & 0x1f); buff &= (1 << bits) - 1; } }
  if (bits > 0) out.push((buff << (5 - bits)) & 0x1f);
  return out;
}
function mskAddress(prefix, version, payloadBytes){
  const five = mskConv8to5([version & 0xff, ...payloadBytes]);
  const values = [...prefix].map(ch => ch.charCodeAt(0) & 0x1f).concat([0], five, [0,0,0,0,0,0,0,0]);
  let c = 1n;
  for (const d of values){
    const c0 = c >> 35n;
    c = ((c & 0x07ffffffffn) << 5n) ^ BigInt(d);
    if (c0 & 0x01n) c ^= 0x98f2bc8e61n;
    if (c0 & 0x02n) c ^= 0x79b76d99e2n;
    if (c0 & 0x04n) c ^= 0xf33e5fb3c4n;
    if (c0 & 0x08n) c ^= 0xae2eabe2a8n;
    if (c0 & 0x10n) c ^= 0x1e4f43e470n;
  }
  c ^= 1n;
  const ck = []; for (let i = 4; i >= 0; i--) ck.push(Number((c >> BigInt(8 * i)) & 0xffn));
  return prefix + ":" + five.concat(mskConv8to5(ck)).map(x => MSK_B32[x]).join("");
}
// A standard ML-DSA P2PKH script (69 bytes): OpDup OpBlake2b512 OpData64 <hash64> OpEqualVerify OpCheckSigMlDsa87.
function p2pkhMldsaHash(scriptHex){ const m = /^76c440([0-9a-f]{128})88a6$/.exec(String(scriptHex||"").toLowerCase()); return m ? m[1] : null; }
function p2pkhMldsaAddress(scriptHex, prefix){ const h = p2pkhMldsaHash(scriptHex); return h ? mskAddress(prefix || "misakatest", 2, hexToByteArr(h)) : null; }
// EVM_DEPOSIT_LOCK output script (108 bytes): OpNop OpData36 <evm(20) ‖ timeout_daa u64 LE ‖ claim_tip u64 LE> OpDrop <69-byte refund P2PKH>.
// Accepts the bare 216-hex script (REST `script_public_key`) or the wRPC form with the 2-byte spk
// version in front ("0000" + script). Returns null for anything that is not exactly the lock.
function parseDepositLock(spk){
  let h = String(spk || "").toLowerCase().replace(/^0x/, "");
  if (h.length === 220 && h.startsWith("0000")) h = h.slice(4);
  if (h.length !== 216 || h.slice(0, 4) !== "6124" || h.slice(76, 78) !== "75") return null;
  const refundScript = h.slice(78);
  const refundHash = p2pkhMldsaHash(refundScript);
  if (!refundHash) return null;
  const timeoutHex = h.slice(44, 60);
  return {
    evmAddress: "0x" + h.slice(4, 44),
    timeoutDaa: timeoutHex === U64_MAX_HEX_LE ? null : leU64(timeoutHex),   // null = never refundable (u64::MAX)
    tipSompi: leU64(h.slice(60, 76)),
    refundScript, refundHash,
  };
}
// F002 withdraw calldata: `[spk version u16 BE][destination script]` → the destination, when it is a standard P2PKH.
function withdrawDestination(input, prefix){
  const h = String(input || "").toLowerCase().replace(/^0x/, "");
  if (h.length < 4) return null;
  return { version: parseInt(h.slice(0, 4), 16), script: h.slice(4), address: p2pkhMldsaAddress(h.slice(4), prefix) };
}
// wei (hex or decimal string) → MSK with thousands separators and trailing zeros trimmed (exact, BigInt).
function fmtWei(v){
  let w; try { w = BigInt(v == null || v === "" ? 0 : v); } catch { return "—"; }
  const neg = w < 0n; if (neg) w = -w;
  const unit = 10n ** 18n;
  const ip = (w / unit).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  const fp = (w % unit).toString().padStart(18, "0").replace(/0+$/, "");
  return (neg ? "-" : "") + ip + (fp ? "." + fp : "");
}
function evmPartyLabel(addr){ const a = String(addr || "").toLowerCase(); if (!a) return null;
  if (EVM_PRECOMPILE_LABELS[a]) return EVM_PRECOMPILE_LABELS[a];
  if (/^0x0{36}f0[0-9a-f]{2}$/.test(a)) return { label: "precompile 0x…" + a.slice(-4).toUpperCase(), title: a };
  return null; }
// One scanned L1 block, reduced to what the bridge needs (the raw block is not kept: ML-DSA
// signatures make a full block ~5 KB per input).
function bridgeSummarize(b){
  const hd = b.header || {}, vd = b.verboseData || {};
  const s = { hash: hd.hash, daa: Number(hd.daaScore || 0), blue: Number(hd.blueScore || 0), ts: Number(hd.timestamp || 0),
              isChain: !!vd.isChainBlock, sp: vd.selectedParentHash || null,
              mergeset: (vd.mergeSetBluesHashes || []).concat(vd.mergeSetRedsHashes || []),
              locks: [], claims: [], spends: [], prefix: null };
  const dec = decodeEvmPayload(b.evmPayload);
  for (const c of (dec.claims || [])) s.claims.push({ outpoint: c.outpoint, evmAddress: c.evmAddress, amountSompi: c.amountSompi, tipSompi: c.tipSompi });
  for (const t of (b.transactions || [])){
    const id = t.verboseData && t.verboseData.transactionId; if (!id) continue;
    for (const i of (t.inputs || [])){ const op = i.previousOutpoint; if (op && op.transactionId) s.spends.push({ op: op.transactionId + ":" + Number(op.index || 0), txid: id }); }
    (t.outputs || []).forEach((o, ix) => {
      const spk = o.scriptPublicKey;
      const hex = typeof spk === "string" ? spk : (spk && (spk.script || spk.scriptPublicKey)) || "";
      const f = parseDepositLock(hex);
      if (f) s.locks.push({ txid: id, index: ix, value: Number(o.value != null ? o.value : (o.amount || 0)), ...f });
      const a = o.verboseData && o.verboseData.scriptPublicKeyAddress;
      if (a && !s.prefix && a.indexOf(":") > 0) s.prefix = a.split(":")[0];
    });
  }
  return s;
}
// Join locks, claims, spends and acceptance into one row per deposit, newest first.
//   accepted: Map(txid → accepting chain block hash) from getVirtualChainFromBlock(anchor)
//   rest:     Map(txid → /transactions/{id} body | null when the index does not know the tx); absent = not asked
//   virtualDaa/now: for the refund window and the 90-minute unknown grace
function bridgeDeriveRows(blocks, accepted, rest, virtualDaa, now){
  const claimsBy = new Map(), spentBy = new Map(), locks = new Map(), merged = new Set();
  const ordered = blocks.slice().sort((a, b) => (a.daa - b.daa) || (a.ts - b.ts));
  for (const b of ordered){
    if (b.isChain){
      for (const h of b.mergeset) merged.add(String(h).toLowerCase());
      for (const c of b.claims) if (!claimsBy.has(c.outpoint)) claimsBy.set(c.outpoint, { ...c, block: b.hash, daa: b.daa, ts: b.ts });
    }
    for (const s of b.spends) if (!spentBy.has(s.op)) spentBy.set(s.op, { ...s, block: b.hash, daa: b.daa, ts: b.ts });
    for (const l of b.locks){
      const key = l.txid + ":" + l.index, cur = locks.get(key);
      if (!cur) locks.set(key, { ...l, key, block: b.hash, daa: b.daa, ts: b.ts, isChain: b.isChain, blocks: [b.hash] });
      else { cur.blocks.push(b.hash); if (b.isChain && !cur.isChain){ cur.block = b.hash; cur.isChain = true; } }
    }
  }
  const rows = [];
  for (const l of locks.values()){
    const r = { ...l, status: "pending", claim: null, spend: null, note: "", acceptingBlock: accepted.get(l.txid) || null, orphan: false };
    const c = claimsBy.get(l.key), s = spentBy.get(l.key), j = rest.get(l.txid);
    const decided = l.blocks.some(h => merged.has(String(h).toLowerCase()));   // its block was merged by a chain block in the window
    if (c){ r.status = "credited"; r.claim = c; }
    else if (s){ r.status = "refunded"; r.spend = s; }
    else if (r.acceptingBlock || (j && j.is_accepted === true)){
      if (!r.acceptingBlock && j) r.acceptingBlock = j.accepting_block_hash || null;
      r.status = (l.timeoutDaa != null && virtualDaa >= l.timeoutDaa) ? "expired" : "waiting";
    }
    else if (decided){ r.status = "not-accepted"; r.note = "merged by a chain block that did not accept it"; }
    else if (j && j.is_accepted === false && now - l.ts >= BRIDGE_UNKNOWN_GRACE_MS){ r.status = "not-accepted"; r.note = "the index shows it unaccepted after 90 min"; }
    else if (j === null && now - l.ts >= BRIDGE_UNKNOWN_GRACE_MS){ r.status = "not-accepted"; r.note = "unknown to the index after 90 min"; }
    rows.push(r);
  }
  for (const [op, c] of claimsBy){
    if (locks.has(op)) continue;   // the lock is older than the scan window: the claim is all we have
    const [txid, ix] = op.split(":");
    rows.push({ key: op, txid, index: Number(ix || 0), value: c.amountSompi, evmAddress: c.evmAddress, tipSompi: c.tipSompi, timeoutDaa: null,
                refundHash: null, block: c.block, daa: c.daa, ts: c.ts, isChain: true, blocks: [c.block], status: "credited", claim: c, spend: null,
                note: "", acceptingBlock: null, orphan: true });
  }
  rows.sort((a, b) => (b.ts - a.ts) || (b.daa - a.daa) || (a.key < b.key ? -1 : 1));
  return rows;
}
function bridgeCounts(rows){
  const n = { total: rows.length, waiting: 0, credited: 0, notAccepted: 0, pending: 0, refunded: 0 };
  for (const r of rows){
    if (r.status === "waiting" || r.status === "expired") n.waiting++;
    else if (r.status === "credited") n.credited++;
    else if (r.status === "not-accepted") n.notAccepted++;
    else if (r.status === "refunded") n.refunded++;
    else n.pending++;
  }
  return n;
}
// An EVM JSON-RPC block+tx → the list row (value stays a hex string; fmtWei is exact).
function evmTxRow(t, blk){
  return { hash: String(t.hash || "").toLowerCase(), from: t.from || null, to: t.to || null, value: t.value || "0x0", input: t.input || "0x",
           nonce: t.nonce, type: t.type, blockNumber: parseInt(blk.number, 16), blockHash: String(blk.hash || "").toLowerCase(),
           ts: parseInt(blk.timestamp, 16) * 1000, status: "pending", gasUsed: null };
}
/* @bridge-pure-end */

// ---- EVM JSON-RPC (POST /evm, same origin). Batches are supported by the node; fall back to one call at a time.
async function evmRpcRaw(body, timeout = 15000){
  const ctl = (typeof AbortController === "function") ? new AbortController() : null;
  const to = ctl ? setTimeout(() => ctl.abort(), timeout) : null;
  try {
    const r = await fetch("/evm", { method: "POST", headers: { "Content-Type": "application/json", "Accept": "application/json" },
                                    body: JSON.stringify(body), signal: ctl ? ctl.signal : undefined });
    if (!r.ok) throw new Error("EVM RPC HTTP " + r.status);
    return await r.json();
  } finally { if (to) clearTimeout(to); }
}
async function evmRpc(method, params = []){
  const j = await evmRpcRaw({ jsonrpc: "2.0", id: 1, method, params });
  if (j && j.error) throw new Error(j.error.message || JSON.stringify(j.error));
  return j ? j.result : null;
}
async function evmRpcBatch(calls){   // calls: [[method, params], …] → results in the same order (null on a per-call error)
  if (!calls.length) return [];
  const body = calls.map((c, i) => ({ jsonrpc: "2.0", id: i + 1, method: c[0], params: c[1] || [] }));
  let arr = null;
  try { const j = await evmRpcRaw(body); if (Array.isArray(j)) arr = j; } catch {}
  if (arr){ const out = new Array(calls.length).fill(null); for (const r of arr){ const i = Number(r && r.id) - 1; if (i >= 0 && i < out.length && !r.error) out[i] = r.result; } return out; }
  const out = []; for (const c of calls){ try { out.push(await evmRpc(c[0], c[1])); } catch { out.push(null); } } return out;
}

// ---- EVM transactions: scan backwards from the head; cached per page session, extended incrementally.
let evmTxScan = null;   // { head, txs (newest first), scanned, lowest, exhausted }
let evmTxBusy = false;
async function evmTxCollect(onProgress){
  const head = parseInt(await evmRpc("eth_blockNumber", []), 16);
  if (!(head >= 0)) throw new Error("eth_blockNumber returned nothing");
  const prev = evmTxScan;
  if (prev && prev.head === head) return prev;
  const fromN = head, toN = prev ? prev.head + 1 : Math.max(0, head - EVMTX_SCAN_BLOCKS + 1);
  const found = []; let n = fromN, scanned = 0;
  while (n >= toN && (prev || found.length < EVMTX_WANT)){
    const batches = [];
    for (let b = 0; b < EVMTX_PARALLEL && n >= toN; b++){ const nums = []; for (let k = 0; k < EVMTX_BATCH && n >= toN; k++) nums.push(n--); batches.push(nums); }
    const rounds = await Promise.all(batches.map(nums => evmRpcBatch(nums.map(x => ["eth_getBlockByNumber", ["0x" + x.toString(16), true]]))));
    for (const res of rounds) res.forEach(blk => { if (!blk) return; scanned++; for (const t of (blk.transactions || [])) if (t && typeof t === "object") found.push(evmTxRow(t, blk)); });
    if (onProgress) onProgress(`Scanning EVM blocks… ${num(scanned)} scanned · ${num(found.length)} transaction${found.length === 1 ? "" : "s"}`);
  }
  if (found.length){
    const rc = await evmRpcBatch(found.map(t => ["eth_getTransactionReceipt", [t.hash]]));
    found.forEach((t, i) => { const r = rc[i]; if (r){ t.status = (r.status === "0x1" || r.status === 1) ? "success" : "failed"; t.gasUsed = r.gasUsed; } });
  }
  found.sort((a, b) => (b.blockNumber - a.blockNumber) || (b.ts - a.ts));
  const txs = prev ? found.concat(prev.txs) : found;
  evmTxScan = { head, txs: txs.slice(0, 200), scanned: scanned + (prev ? prev.scanned : 0), lowest: prev ? prev.lowest : n + 1,
                exhausted: prev ? prev.exhausted : (found.length < EVMTX_WANT && n + 1 <= toN) };
  return evmTxScan;
}
function evmPartyCell(addr, tx, prefix){
  if (!addr) return '<span class="dim">contract creation</span>';
  const p = evmPartyLabel(addr);
  if (!p) return `<span class="mono" title="${esc(addr)}">${esc(short(addr, 8))}</span>`;
  let extra = "";
  if (String(addr).toLowerCase() === EVM_WITHDRAW_PRECOMPILE && tx){
    const d = withdrawDestination(tx.input, prefix);
    if (d && d.address) extra = ` <span class="dim">→</span> ${linkAddrShort(d.address)}`;
    else if (d && d.script) extra = ` <span class="dim" title="${esc(d.script)}">→ script ${esc(short(d.script, 6))}</span>`;
  }
  return `<span class="pill evm" title="${esc(p.title || addr)}">${esc(p.label)}</span>${extra}`;
}
function evmStatusPill(s){
  if (s === "success") return '<span class="pill chain">success</span>';
  if (s === "failed")  return '<span class="pill red">failed</span>';
  return '<span class="pill blue">pending</span>';
}
function paintEvmTxs(gen){
  if (gen !== routeGen) return;
  const box = document.getElementById("evmTxSec"), head = document.getElementById("evmTxSecHead");
  if (!box || !evmTxScan) return;
  const s = evmTxScan, prefix = (bridgeScan && bridgeScan.prefix) || null;
  const l1 = new Map();   // EVM block hash = the first 32 bytes of the L1 chain block's hash
  if (bridgeScan) for (const b of bridgeScan.blocks.values()) if (b.isChain && b.hash) l1.set("0x" + String(b.hash).slice(0, 64).toLowerCase(), b.hash);
  const span = s.exhausted || s.lowest != null ? ` · EVM blocks #${num(s.lowest)}–#${num(s.head)}` : "";
  if (head) head.innerHTML = `EVM transactions <span class="dim" style="font-size:13px">(Ethereum JSON-RPC · newest ${num(Math.min(s.txs.length, EVMTX_WANT))} of ${num(s.txs.length)} · ${num(s.scanned)} blocks scanned${span})</span>`;
  if (!s.txs.length){
    box.innerHTML = `<div class="note">No EVM transactions yet on this network — the ${num(s.scanned)} most recent EVM blocks (#${num(s.lowest)}–#${num(s.head)}) carry none. The first submitted transaction appears here with its receipt status; a withdrawal to L1 shows as <span class="pill evm">Withdraw → UTXO</span>.</div>`;
    return;
  }
  box.innerHTML = `<table class="tbl"><thead><tr><th>EVM tx hash</th><th class="num">Block</th><th>From</th><th>To</th><th class="num">Value (${SYMBOL})</th><th>Status</th><th class="right">Age</th></tr></thead><tbody>${
    s.txs.slice(0, EVMTX_WANT).map(t => {
      const l1h = l1.get(t.blockHash);
      const blockCell = `${num(t.blockNumber)}${l1h ? ` <span class="dim">·</span> ${linkBlock(l1h)}` : ` <span class="dim mono" title="${esc(t.blockHash)}">${esc(short(t.blockHash, 6))}</span>`}`;
      return `<tr><td>${linkEvmTx(t.hash)}</td>
        <td class="num nowrap">${blockCell}</td>
        <td>${evmPartyCell(t.from, null, prefix)}</td>
        <td>${evmPartyCell(t.to, t, prefix)}</td>
        <td class="num coin">${fmtWei(t.value)}</td>
        <td>${evmStatusPill(t.status)}</td>
        <td class="right dim" title="${esc(dt(t.ts))}">${ago(t.ts)}</td></tr>`;
    }).join("")}</tbody></table>`;
}
async function fillEvmTxSection(gen){
  if (evmTxBusy) return; evmTxBusy = true;
  const box = document.getElementById("evmTxSec");
  try {
    if (!evmTxScan && box && gen === routeGen) box.innerHTML = `<div class="spin">Reading the EVM head…</div>`;
    await evmTxCollect(msg => { const b = document.getElementById("evmTxSec"); if (b && gen === routeGen && !evmTxScan) b.innerHTML = `<div class="spin">${esc(msg)}</div>`; });
    paintEvmTxs(gen);
  } catch (e) {
    const b = document.getElementById("evmTxSec");
    if (b && gen === routeGen && !evmTxScan) b.innerHTML = `<div class="note">EVM JSON-RPC unavailable (${esc(e.message || e)}). The lane's Ethereum endpoint is <span class="mono">POST /evm</span> on this host; the sections above are read over wRPC and are unaffected.</div>`;
  } finally { evmTxBusy = false; }
}

// ---- Bridge: the L1 window scan (cached per page session; refreshed incrementally above the last chain block seen).
let bridgeScan = null;   // { net, sink, anchor, virtualDaa, blocks: Map(hash → summary), accepted: Map(txid → block), rest: Map(txid → body|null), prefix, topChain }
let bridgeBusy = false;
async function bridgeAnchorHashes(dag, onProgress){
  // → { near, far }: the chain blocks BRIDGE_NEAR_BACK and BRIDGE_WALK_BACK steps below the sink.
  // The near one anchors a fast first pass (the newest deposits paint in seconds); the far one the
  // full window, read behind it. While the chain from the pruning point is short, one
  // getVirtualChainFromBlock call lists it whole (2 round trips); otherwise walk
  // selectedParentHash back one block per call, which is the always-correct path.
  try {
    const pp = await rpc("getBlock", { hash: dag.pruningPointHash, includeTransactions: false });
    const span = Number(dag.virtualDaaScore || 0) - Number(pp && pp.block && pp.block.header && pp.block.header.daaScore || 0);
    if (span > 0 && span <= BRIDGE_CHAIN_SHORTCUT_SPAN){
      const vc = await rpc("getVirtualChainFromBlock", { startHash: dag.pruningPointHash, includeAcceptedTransactionIds: false });
      const added = (vc && vc.addedChainBlockHashes) || [];
      if (added.length && String(added[added.length - 1]).toLowerCase() === String(dag.sink).toLowerCase())
        return { near: added[Math.max(0, added.length - 1 - BRIDGE_NEAR_BACK)], far: added[Math.max(0, added.length - 1 - BRIDGE_WALK_BACK)] };
    }
  } catch {}
  let anchor = dag.sink, near = null;
  for (let i = 0; i < BRIDGE_WALK_BACK && anchor; i++){
    if (onProgress && i % 10 === 0) onProgress(`Anchoring the L1 scan… ${i}/${BRIDGE_WALK_BACK} chain blocks back`);
    if (i === BRIDGE_NEAR_BACK) near = anchor;
    let r; try { r = await rpc("getBlock", { hash: anchor, includeTransactions: false }); } catch { break; }
    const sp = r && r.block && r.block.verboseData && r.block.verboseData.selectedParentHash;
    if (!sp || !/[1-9a-f]/i.test(sp)) break;
    anchor = sp;
  }
  return { near: near || anchor, far: anchor };
}
// Page getBlocks(lowHash) forward, feeding summaries into `scan`; returns the highest chain block reached.
async function bridgePageBlocks(scan, low, maxBlocks, onPage){
  let top = low, added = 0;
  for (let it = 0; it < 24 && added < maxBlocks && low; it++){
    let gb; try { gb = await rpc("getBlocks", { lowHash: low, includeBlocks: true, includeTransactions: true }, BRIDGE_PAGE_TIMEOUT_MS); } catch (e) { if (it === 0) throw e; break; }
    const blocks = gb.blocks || []; if (!blocks.length) break;
    let topBlue = -1, topHash = null, fresh = 0;
    for (const b of blocks){
      const h = b.header && b.header.hash; if (!h) continue;
      if (!scan.blocks.has(h)){ const s = bridgeSummarize(b); scan.blocks.set(h, s); added++; fresh++; if (s.prefix && !scan.prefix) scan.prefix = s.prefix; }
      const s = scan.blocks.get(h);
      if (s.isChain && s.blue > topBlue){ topBlue = s.blue; topHash = h; }
    }
    if (onPage) onPage(scan, added);
    if (!topHash || topHash === low) break;   // no forward progress
    if (!fresh && it > 0) break;              // only blocks we already hold: the deep pass has met the near window
    low = topHash; top = topHash;
  }
  return top;
}
async function bridgeRefreshAcceptance(scan){
  // The node may answer a long span in chunks (the added list stops short of the sink): keep asking
  // from the last chain block returned, so no accepted lock is missed and mislabelled "not accepted".
  try {
    const acc = new Map(); let start = scan.anchor;
    for (let it = 0; it < 12 && start; it++){
      const vc = await rpc("getVirtualChainFromBlock", { startHash: start, includeAcceptedTransactionIds: true }, 60000);
      for (const e of (vc && vc.acceptedTransactionIds) || []) for (const id of (e.acceptedTransactionIds || [])) acc.set(String(id).toLowerCase(), e.acceptingBlockHash);
      const added = (vc && vc.addedChainBlockHashes) || [], last = added[added.length - 1];
      if (!last || last === start || String(last).toLowerCase() === String(scan.sink).toLowerCase()) break;
      start = last;
    }
    scan.accepted = acc;
  } catch {}
}
async function bridgeCollect(onProgress, onPage){
  const dag = await rpc("getBlockDagInfo");
  const net = dag.network || null, sink = dag.sink;
  let scan = bridgeScan;
  if (scan && scan.net === net && scan.sink === sink){ scan.virtualDaa = Number(dag.virtualDaaScore || 0); return scan; }
  if (scan && scan.net === net && scan.topChain){
    try {   // incremental: everything above the last chain block we hold
      scan.topChain = await bridgePageBlocks(scan, scan.topChain, BRIDGE_MAX_BLOCKS, onPage);
      scan.sink = sink; scan.virtualDaa = Number(dag.virtualDaaScore || 0);
      if (scan.blocks.size > BRIDGE_CACHE_BLOCKS){
        const drop = [...scan.blocks.values()].sort((a, b) => a.daa - b.daa).slice(0, scan.blocks.size - BRIDGE_CACHE_BLOCKS);
        for (const d of drop) scan.blocks.delete(d.hash);
      }
      await bridgeRefreshAcceptance(scan);
      return scan;
    } catch { scan = null; }   // e.g. the cached top block left the selected chain → rescan from a fresh anchor
  }
  scan = { net, sink, anchor: null, deeper: null, virtualDaa: Number(dag.virtualDaaScore || 0), blocks: new Map(), accepted: new Map(), rest: new Map(), prefix: null, topChain: null };
  const anchors = await bridgeAnchorHashes(dag, onProgress);
  scan.anchor = anchors.near; scan.deeper = anchors.far !== anchors.near ? anchors.far : null;
  bridgeScan = scan;
  if (onProgress) onProgress("Reading the newest L1 blocks…");
  scan.topChain = await bridgePageBlocks(scan, scan.anchor, BRIDGE_MAX_BLOCKS, onPage);
  await bridgeRefreshAcceptance(scan);
  return scan;
}
// The second, deeper pass of a fresh scan: extend the window down to the far anchor. Returns true
// when it added anything worth a repaint.
async function bridgeDeepen(scan){
  const far = scan.deeper; if (!far) return false;
  scan.deeper = null;
  const before = scan.blocks.size;
  try { await bridgePageBlocks(scan, far, BRIDGE_MAX_BLOCKS, null); scan.anchor = far; await bridgeRefreshAcceptance(scan); }
  catch { scan.deeper = far; }   // a timed-out page is retried by the next poll rather than silently dropped
  return scan.blocks.size !== before;
}
// Ask the explorer index about the locks the window could not decide (bounded, in parallel).
async function bridgeConsultIndex(scan, rows){
  const ask = rows.filter(r => r.status === "pending" && !r.orphan).map(r => r.txid).slice(0, 12);
  if (!ask.length) return false;
  await Promise.all(ask.map(async id => { scan.rest.set(id, await apiGet(`/transactions/${id}`)); }));
  return true;
}
function bridgeStatusCell(r, scan){
  const daaNow = scan.virtualDaa;
  const refund = r.timeoutDaa == null ? (r.orphan ? "" : `<div class="dim">never refundable</div>`)
               : `<div class="dim">refund from DAA ${num(r.timeoutDaa)} · now ${num(daaNow)}</div>`;
  switch (r.status){
    case "credited": return `<span class="pill chain">credited</span><div class="dim">claim in ${linkBlock(r.claim.block)} <span title="${esc(dt(r.claim.ts))}">${ago(r.claim.ts)}</span>${r.claim.tipSompi ? ` · tip ${coin(r.claim.tipSompi)} ${SYMBOL}` : ""}${r.orphan ? " · lock older than the scan window" : ""}</div>`;
    case "waiting":  return `<span class="pill warn">waiting for claim</span>${refund}${r.acceptingBlock ? `<div class="dim">lock accepted in ${linkBlock(r.acceptingBlock)}</div>` : ""}`;
    case "expired":  return `<span class="pill warn">claim window closed</span><div class="dim">refundable by the depositor since DAA ${num(r.timeoutDaa)} · now ${num(daaNow)}</div>`;
    case "refunded": return `<span class="pill">refunded</span><div class="dim">spent by ${linkTx(r.spend.txid)} in ${linkBlock(r.spend.block)}</div>`;
    case "not-accepted": return `<span class="pill red">not accepted</span>${r.note ? `<div class="dim">${esc(r.note)}</div>` : ""}`;
    default: return `<span class="pill blue">pending acceptance</span><div class="dim">not yet merged by a chain block</div>`;
  }
}
function paintBridge(gen, rows){
  if (gen !== routeGen) return;
  const box = document.getElementById("bridgeSec"), head = document.getElementById("bridgeSecHead");
  if (!box || !bridgeScan) return;
  const scan = bridgeScan, n = bridgeCounts(rows);
  const blocks = scan.blocks.size, chain = [...scan.blocks.values()].filter(b => b.isChain).length;
  const parts = [`${num(n.total)} deposit${n.total === 1 ? "" : "s"}`];
  if (n.waiting) parts.push(`${num(n.waiting)} waiting for claim`);
  if (n.credited) parts.push(`${num(n.credited)} credited`);
  if (n.notAccepted) parts.push(`${num(n.notAccepted)} not accepted`);
  if (n.pending) parts.push(`${num(n.pending)} pending`);
  if (n.refunded) parts.push(`${num(n.refunded)} refunded`);
  if (head) head.innerHTML = `Bridge <span class="dim" style="font-size:13px">(UTXO→EVM deposits · ${parts.join(" · ")} · ${num(blocks)} L1 blocks scanned, ${num(chain)} chain${scan.deeper ? " · reading older blocks…" : ""})</span>`;
  if (!rows.length){
    box.innerHTML = `<div class="note">No bridge deposits in the last ${num(blocks)} L1 blocks. A deposit is a transaction with an <span class="mono">EVM_DEPOSIT_LOCK</span> output (built by the wallet's deposit command); it shows here as soon as a block carries it, then moves to <span class="pill chain">credited</span> when a chain block's payload claims it.</div>`;
    return;
  }
  box.innerHTML = `<table class="tbl"><thead><tr><th>When</th><th>L1 tx</th><th class="num">Amount (${SYMBOL})</th><th>To (EVM)</th><th>From (depositor)</th><th>Status</th></tr></thead><tbody>${
    rows.map(r => {
      const dep = r.refundHash ? p2pkhMldsaAddress("76c440" + r.refundHash + "88a6", scan.prefix) : null;
      return `<tr><td class="nowrap"><span title="${esc(dt(r.ts))}">${ago(r.ts)}</span><div class="dim">${esc(dt(r.ts))}</div><div class="dim">DAA ${num(r.daa)} · ${linkBlock(r.block)}</div></td>
        <td>${linkTx(r.txid)} <span class="dim">#${num(r.index)}</span></td>
        <td class="num coin">${coin(r.value)}</td>
        <td><span class="mono" title="${esc(r.evmAddress)}">${esc(short(r.evmAddress, 8))}</span></td>
        <td>${dep ? linkAddrShort(dep) : '<span class="dim">—</span>'}</td>
        <td>${bridgeStatusCell(r, scan)}</td></tr>`;
    }).join("")}</tbody></table>
    <div class="note" style="margin-top:8px">A lock is <b>credited</b> when a <b>chain</b> block's payload carries its DepositClaim (the EVM account receives (amount − tip) × 10¹⁰ wei); a lock the chain never accepted cannot be claimed; past its refund DAA the depositor's key can spend it back.</div>`;
}
async function fillBridgeSection(gen){
  if (bridgeBusy) return; bridgeBusy = true;
  const say = msg => { const b = document.getElementById("bridgeSec"); if (b && gen === routeGen && !(bridgeScan && bridgeScan.topChain)) b.innerHTML = `<div class="spin">${esc(msg)}</div>`; };
  const derive = () => bridgeDeriveRows([...bridgeScan.blocks.values()], bridgeScan.accepted, bridgeScan.rest, bridgeScan.virtualDaa, Date.now());
  try {
    if (!bridgeScan) say("Locating the L1 scan anchor…");
    const scan = await bridgeCollect(say, (s, added) => { if (gen === routeGen && s === bridgeScan) { const b = document.getElementById("bridgeSec"); if (b && !s.topChain) b.innerHTML = `<div class="spin">Reading the L1 window… ${num(s.blocks.size)} blocks</div>`; } });
    if (scan !== bridgeScan) return;
    let rows = derive();
    paintBridge(gen, rows);
    if (await bridgeConsultIndex(scan, rows)){ rows = derive(); paintBridge(gen, rows); }
    if (evmTxScan) paintEvmTxs(gen);   // the EVM table can now link its blocks to L1 chain blocks
    if (scan.deeper){                  // the rest of the window streams in behind the first paint
      await bridgeDeepen(scan);
      if (scan !== bridgeScan) return;
      rows = derive(); paintBridge(gen, rows);
      if (await bridgeConsultIndex(scan, rows)){ rows = derive(); paintBridge(gen, rows); }
      if (evmTxScan) paintEvmTxs(gen);
    }
  } catch (e) {
    const b = document.getElementById("bridgeSec");
    if (b && gen === routeGen && !bridgeScan) b.innerHTML = `<div class="note">Bridge scan failed (${esc(e.message || e)}).</div>`;
  } finally { bridgeBusy = false; }
}
function evmLaneExtraSections(){
  return `<h2 class="sec" id="evmTxSecHead">EVM transactions <span class="dim" style="font-size:13px">(Ethereum JSON-RPC · newest first)</span></h2>
    <div id="evmTxSec"><div class="spin">Loading…</div></div>
    <h2 class="sec" id="bridgeSecHead">Bridge <span class="dim" style="font-size:13px">(UTXO→EVM deposits and their claims)</span></h2>
    <div id="bridgeSec"><div class="spin">Loading…</div></div>`;
}

/* ---------------- shared: collect a window of recent DAG blocks --------------- */
// Walk the selected chain back `walkBack` blocks from the sink to get an anchor, then page
// getBlocks(lowHash) forward collecting up to `maxBlocks` distinct blocks (newest-inclusive).
// Used by the BlockDAG / Transactions / Miners pages. Same node-direct source as everything else.
async function collectRecentBlocks({ walkBack = 50, maxBlocks = 150, includeTx = false } = {}){
  let dag; try { dag = await rpc("getBlockDagInfo"); } catch { return { blocks: [], sink: null, dag: null }; }
  let anchor = dag.sink;
  for (let i=0;i<walkBack && anchor;i++){
    let r; try { r = await rpc("getBlock", { hash: anchor, includeTransactions:false }); } catch { break; }
    const sp = r && r.block && r.block.verboseData && r.block.verboseData.selectedParentHash;
    if (!sp) break; anchor = sp;
  }
  const seen = new Map(); let low = anchor;
  for (let it=0; it<16 && seen.size < maxBlocks && low; it++){
    let gb; try { gb = await rpc("getBlocks", { lowHash: low, includeBlocks:true, includeTransactions:includeTx }); } catch { break; }
    const blocks = gb.blocks || []; if (!blocks.length) break;
    let topBlue=-1, topHash=null;
    for (const b of blocks){ const h=b.header&&b.header.hash; if (h && !seen.has(h)) seen.set(h,b);
      const bl=Number((b.header&&b.header.blueScore)||0); if (bl>topBlue){ topBlue=bl; topHash=h; } }
    if (!topHash || topHash===low) break;   // no forward progress
    low = topHash;
  }
  return { blocks: [...seen.values()], sink: dag.sink, dag };
}
// A block "carries EVM" when its (v2) payload decodes to ≥1 EVM tx or bridge deposit-claim.
function blockHasEvm(b){
  const hd = b.header || {};
  if (!hd.evmPayloadHash && !hd.evmCommitmentRoot) return false;   // pre-EVM (v1) block
  const dec = decodeEvmPayload(b.evmPayload);
  return (dec.txs && dec.txs.length>0) || (dec.claims && dec.claims.length>0);
}
function isCoinbaseTx(t){ return (t.inputs||[]).length === 0; }
// largest-value output (miner subsidy for coinbase; main payee for a transfer) — addr + value.
function largestOut(t){
  let mx=null; for (const o of (t.outputs||[])){ const v=Number(o.value||0); if (!mx || v>Number(mx.value||0)) mx=o; }
  return { addr:(mx && mx.verboseData && mx.verboseData.scriptPublicKeyAddress) || null, value: mx?Number(mx.value||0):0 };
}
function largestOutAddr(t){ return largestOut(t).addr; }
function fmtDur(sec){ sec=Math.max(0,Math.floor(sec)); const m=Math.floor(sec/60), s=sec%60;
  if (m>=60){ const h=Math.floor(m/60); return `${h}h ${m%60}m`; } return m>0?`${m}m ${s}s`:`${s}s`; }

/* ------------------------- PALW · algo-4 lane -------------------------- */
// ADR-0039 / ADR-0040 / ADR-0045. Every figure on this page is read from the node — there is no
// off-chain source: `getPalwState` (the global probe, plus the per-epoch PCPB production context),
// the PALW header fields carried by every block, and the PALW overlay subnetwork band 0x30-0x38 in
// block bodies. Where the chain cannot answer (no algo-4 block minted yet, a snapshot outside the
// retained window) the page says so instead of showing a plausible zero.

// PALW overlay tx kinds — the first byte of the 20-byte subnetwork id (consensus/core/src/subnets.rs).
// NOTE 0x34 is Revocation, NOT slashing (ADR-0040 SLASH-01 renamed the dangling mislabel).
/* ---------------- LLM submissions — what was submitted, who verified it, who approved it ----------------
   Replaces the old algo-4 BlockDAG page (LegacyTn11 lane; its getPalwState probe, batch header
   fields and beacon subnetworks do not exist on the ConsensusV2 network this explorer fronts).
   Everything below is read straight off the chain: the attempt envelope every algo-6 header
   carries (magic "PAV2" + borsh), and the lifecycle objects that ride 0x4b-subnetwork
   transactions (PanelBound / ReceiptLicensed / ProducerDefaulted / Court*). Decoders mirror
   consensus/core/src/palw_attempt_v2.rs and palw_state_v2.rs field-for-field. The page renders
   the record; approval itself is the chain's 3-of-5 signed receipt quorum. */
// Two different numbers, never interchangeable.
// PANEL_DRAW / PANEL_QUORUM: a claim already admitted draws 5 bonded seats and needs 3 Valid
// receipts (ReceiptLicensed). Below 3 holders a licensed claim cannot form and the block defaults.
// requiredReadySeats (registry): a class is minable only after that many seats have proved
// possession on chain (SeatReadinessProved). The globals are seat_count 5 + spare 2 = 7 for
// every live class. Prefetching / Held / Registered admit nothing — even if five operators
// have the file. The roster below is deployment; readySeatsNow is the chain's count.
const PANEL_DRAW = 5;
const PANEL_QUORUM = 3;
// **The verifier-seat roster** — which live panel seats HOLD which class artifact. Deployment
// overlay, not the admission count. Bonds are the on-chain identity — the same `<txid>:N` the
// receipt rows show. Ready seats DERIVE from getPalwModelRegistry (`readySeatsNow` /
// SeatReadinessProved), never from this table.
// testnet-12's OWN premine txid (premine_txid_for(testnet-12), salted per network since 2026-09-24;
// the shared sentinel `6d697361…` is gone from this chain). Read by the deploy kit's probe
// (contrib/t12-deploy-kit/probe-identity-local.sh → PREMINE_TXID); re-check it against fleet.env at the
// shipping commit before staging this file (DEPLOY.md §9).
const PANEL_BOND_TX = "5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669";
// testnet-12 roster (the R-core+ regenesis, public launch 2026-09-25): eight genesis bonds on
// PANEL_BOND_TX:0..7, every one a verifier seat that holds the dense 8k artifact
// (contrib/t12-deploy-kit/PLAN.md §1 and the NODES tables of install-ibm/113/5104.sh). Producers:
// 8k = bond 0 (ibm node0), floor = bonds 1 (ibm node1) and 6 (.113). Card 7 was re-keyed for
// testnet-12 (its old key was on no host). No seat holds the 2M artifact: 2M is CLOSED at launch
// (ADR-0152 §8.3 item 7 / O-11: every 2M attempt and FP claim is refused until 2M's flag day).
// Deployment truth, updated with the fleet.
const PANEL_SEATS = [
  { ix:0, host:"seat 0 · ibm node0 (8k producer)",           holds:["PALW-BASE-0","QWEN25-A16-8K"] },
  { ix:1, host:"seat 1 · ibm node1 (floor producer)",        holds:["PALW-BASE-0","QWEN25-A16-8K"] },
  { ix:2, host:"seat 2 · 5.104.81.23",                       holds:["PALW-BASE-0","QWEN25-A16-8K"] },
  { ix:3, host:"seat 3 · 5.104.81.23",                       holds:["PALW-BASE-0","QWEN25-A16-8K"] },
  { ix:4, host:"seat 4 · 5.104.81.23",                       holds:["PALW-BASE-0","QWEN25-A16-8K"] },
  { ix:5, host:"seat 5 · 5.104.81.23",                       holds:["PALW-BASE-0","QWEN25-A16-8K"] },
  { ix:6, host:"seat 6 · .113 explorer node (floor producer)", holds:["PALW-BASE-0","QWEN25-A16-8K"] },
  { ix:7, host:"seat 7 · 5.104.81.23 (re-keyed for t12)",    holds:["PALW-BASE-0","QWEN25-A16-8K"] },
];
function verifierCount(name){ return PANEL_SEATS.filter(s => s.holds.includes(name)).length; }
function registryAdmitsClaims(state){
  const t = String(state || "");
  return /^Probation/.test(t) || /^ActiveLimited/.test(t) || t === "Active";
}
function llmRegistryIndex(reg){
  const root = (reg && !reg.__unsupported && !reg.__error) ? (reg.registry || reg) : null;
  const byId = Object.create(null);
  for (const row of (root && root.classes) || []) {
    const id = String(row.classId || "").toLowerCase();
    if (id) byId[id] = row;
  }
  const proved = Object.create(null);
  for (const r of (root && root.readiness) || []) {
    const ix = r.bondIndex;
    const cid = String(r.classId || "").toLowerCase();
    if (ix == null || !cid) continue;
    proved[ix + ":" + cid] = r;
  }
  return { root, byId, proved };
}
const LLM_CLASSES = [
  { id:"f1c5635c6e47e96e7af864789c94523335dc56584af297cb8cc19021c228b897bee1a50145597e45f8ca2727349bf4aa352a98cc05274b7f059a176642f623c8",
    name:"PALW-BASE-0", model:"deterministic integer floor — no model file", tag:"floor" },
  { id:"ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01",
    name:"QWEN25-A16-8K", model:"Qwen/Qwen2.5-1.5B/graph-v7@8192 · W8A16 static PTQ, 8k held context (genesis row; Prefetching → Probation once 7 seats prove readiness after the grace)", tag:"llm" },
  { id:"74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a",
    name:"QWEN25-A16-2M", model:"Qwen/Qwen2.5-1.5B/graph-v7@2097152 · W8A16 static PTQ, 2M held context (genesis row; closed at launch by rule — no claims until a flag day installs its measured row)", tag:"llm" },
  // testnet-12 registers no hybrid row at genesis (the held map's GDN convolution gather is wrong for
  // Qwen3.6-35B-A3B's 16/32 key/value heads). A permissionless registration appears here by its
  // class id from getPalwModelRegistry; name it in this table when it registers.
];
const LLM_CLASS_BY_ID = Object.fromEntries(LLM_CLASSES.map(c=>[c.id, c]));
function llmClassName(id){ const c=LLM_CLASS_BY_ID[id]; return c ? c.name : (id ? short(id,6) : "—"); }

let llmSubs = [];            // newest-first decoded submissions
let llmEvents = [];          // newest-first decoded lifecycle events
let llmSeenBlocks = new Set();
let llmTotalSubs = 0;        // every decoded submission ever seen (the arrays below are capped)
let llmFilter = "llm";       // "llm" (model classes only — the page's point) | "all" | "floor"
function llmIsFloor(classId){ const c = LLM_CLASS_BY_ID[classId]; return !!c && c.tag === "floor"; }
// The committed input/output of each LLM job, decoded server-side by palw-jobs-export from the
// SAME derivations every panel seat performs (anchor → prompt; retained material → generated
// tokens), and published as /llm-jobs.json. Keyed by block hash.
let llmJobs = new Map();
let llmJobsTs = 0;
async function refreshLlmJobs(){
  if (Date.now() - llmJobsTs < 30_000) return;
  llmJobsTs = Date.now();
  try {
    const r = await fetch("/llm-jobs.json", { cache: "no-store" });
    if (!r.ok) return;
    const d = await r.json();
    const m = new Map();
    for (const row of (d.rows||[])) m.set(row.block, row);
    llmJobs = m;
    const fm = new Map(), fb = new Map();
    for (const row of (d.fp_rows||[])){
      fm.set(row.claim, row);
      if (row.block){ const k = String(row.block).toLowerCase(); if (!fb.has(k)) fb.set(k, []); fb.get(k).push(row); }
    }
    llmFpJobs = fm;
    llmFpByBlock = fb;
  } catch {}
}
let llmSeenTx = new Set();
let llmSeenClaims = new Set();   // free-prompt claims already rowed (a claim rides one tx; the sweep may re-read blocks)
let llmFpJobs = new Map();       // claim -> the feed's free-prompt row (text where the fleet holds the retention)
let llmFpByBlock = new Map();    // block hash -> the free-prompt rows whose carrier tx that block included
let llmCursor = null;        // getBlocks paging cursor (grows forward from genesis)
let llmBusy = false;

function llmBytes(v){
  if (Array.isArray(v)) return Uint8Array.from(v);
  const s = String(v||""); const n = s.length >> 1;
  const out = new Uint8Array(n);
  for (let i=0;i<n;i++) out[i] = parseInt(s.substr(i*2,2),16) || 0;
  return out;
}
class LlmRd {
  constructor(b){ this.b=b; this.i=0; }
  take(n){ if (this.i+n > this.b.length) throw new Error("short"); const o=this.b.subarray(this.i,this.i+n); this.i+=n; return o; }
  u8(){ return this.take(1)[0]; }
  u16(){ const b=this.take(2); return b[0] | b[1]<<8; }
  u32(){ const b=this.take(4); return (b[0] | b[1]<<8 | b[2]<<16 | b[3]<<24) >>> 0; }
  u64(){ const b=this.take(8); let v=0; for (let i=7;i>=0;i--) v = v*256 + b[i]; return v; }
  h64(){ let s=""; for (const x of this.take(64)) s += x.toString(16).padStart(2,"0"); return s; }
  vec(){ return this.take(this.u32()); }
}
// PalwAttemptEnvelopeV2 wire: "PAV2" + borsh; unsigned-attempt field order is the deployed struct's.
function llmDecodeAttempt(raw){
  try {
    const b = llmBytes(raw);
    if (!(b[0]===0x50 && b[1]===0x41 && b[2]===0x56 && b[3]===0x32)) return null;   // "PAV2"
    const r = new LlmRd(b.subarray(4)); const a = {};
    a.version = r.u16(); r.h64(); r.h64();                    // network_domain, challenge
    a.classId = r.h64(); a.bondTx = r.h64(); a.bondIx = r.u32();
    r.vec();                                                  // executor_pubkey
    a.operator = r.h64(); a.artifactRoot = r.h64(); a.traceRoot = r.h64(); a.outputRoot = r.h64();
    a.pwu = r.u64(); a.manifestRoot = r.h64(); a.chunks = r.u32(); a.retention = r.u64();
    a.execRoot = r.h64();
    return a;
  } catch { return null; }
}
function llmOutpoint(r){ return { tx:r.h64(), ix:r.u32() }; }
function llmReceipt(r){
  const claim = r.h64(); const tag = r.u8();
  let verdict = "Valid", detail = null;
  if (tag === 1){ verdict = "Unavailable"; detail = { chunk:r.u32(), at:r.u64() }; }
  else if (tag === 2){ verdict = "Incapable"; }
  else if (tag !== 0) throw new Error("verdict#"+tag);
  const seat = llmOutpoint(r); const signedDaa = r.u64(); r.vec();   // signature
  return { claim, verdict, detail, seat, signedDaa };
}
// PalwLifecycleTxPayloadV2 { version:u16, object: PalwConsensusObjectV2 } — variant tags are the
// enum's declaration order in palw_state_v2.rs.
function llmDecodeLifecycle(payload){
  try {
    const r = new LlmRd(llmBytes(payload));
    const version = r.u16(); const v = r.u8();
    const out = { version, variant: v };
    // Variant tags are `PalwConsensusObjectV2`'s declaration order in palw_state_v2.rs. This table
    // was one behind from BondCapabilityDeclared on (every tag ≥ 1 rendered as its predecessor,
    // FreePromptCommitted as "object#12") until 2026-09-04; keep it in the enum's order.
    switch (v){
      case 0:  out.kind = "BondRegistered"; break;
      case 1:  out.kind = "BondCapabilityDeclared"; break;
      case 2:  out.kind = "BondRetireRequested"; break;
      case 3:  out.kind = "ClassRegistered"; out.classId = r.h64(); break;
      case 4:  out.kind = "ClassFrozen"; out.classId = r.h64(); break;
      case 5: {
        out.kind = "PanelBound"; out.claim = r.h64(); out.anchor = r.h64();
        const n = r.u32(); out.seats = [];
        for (let i=0;i<n && i<16;i++) out.seats.push({ bond: llmOutpoint(r), operator: r.h64() });
        break;
      }
      case 6: case 11: {
        out.kind = v===6 ? "ReceiptLicensed" : "ProducerDefaulted";
        out.claim = r.h64();
        const n = r.u32(); out.receipts = [];
        for (let i=0;i<n && i<16;i++) out.receipts.push(llmReceipt(r));
        break;
      }
      case 7:  out.kind = "CourtOpened"; out.session = r.h64(); out.claim = r.h64(); break;
      case 8:  out.kind = "CourtClosed"; out.session = r.h64(); break;
      case 9:  out.kind = "CourtDisclosed"; out.session = r.h64(); break;
      case 10: out.kind = "CourtVerdictPosted"; out.session = r.h64(); break;
      case 12: {
        // FreePromptCommitted { claim, class_id, bond: { outpoint, .. }, executor_pubkey, work_leaves, ... }
        out.kind = "FreePromptCommitted"; out.claim = r.h64(); out.classId = r.h64();
        const bond = llmOutpoint(r); out.bondTx = bond.tx; out.bondIx = bond.ix;
        r.vec();                                   // executor_pubkey
        out.pwu = r.u64();                         // work_leaves: the pwu the claim asks to be paid for
        break;
      }
      case 13: out.kind = "FamilyCertified"; break;
      case 14: out.kind = "ClassLaneCertified"; out.classId = r.h64(); out.lane = r.u8()===1 ? "free-prompt" : "attempt"; break;
      case 15: out.kind = "ObjectChunk"; out.group = r.h64(); out.index = r.u8(); out.count = r.u8(); break;
      case 16: {
        // DerivedArtifactV1 { object: { version, network_domain, claim_id, output_root, grammar_id, transformer_id, kind, dsl_hash, artifact_hash, artifact_bytes, .. }, signature }
        out.kind = "DerivedArtifactV1"; r.u16(); r.h64(); out.claim = r.h64(); r.h64(); r.h64();
        out.transformer = r.h64(); out.artifactKind = r.u16(); r.h64(); out.artifactHash = r.h64(); out.artifactBytes = r.u64();
        break;
      }
      case 17: out.kind = "DefaultAccused"; break;
      case 18: out.kind = "MaterialDisclosed"; break;
      case 19: out.kind = "CourtCloseDeclared"; break;
      case 20: out.kind = "CourtCloseChunk"; break;
      case 21: out.kind = "CourtAttnRootClaimed"; break;
      case 22: out.kind = "CourtAttnDissected"; break;
      case 23: out.kind = "CourtAttnChildChosen"; break;
      default: out.kind = "object#"+v;
    }
    return out;
  } catch { return null; }
}

function renderLlm(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › LLM Jobs</div>
    <h1 class="page">LLM Jobs <span class="dim" style="font-size:13px;font-weight:400">(submitted inference · who verified it · who approved it)</span></h1>
    <div class="note">A model block on this network carries a <b>verified LLM inference claim</b> (PoW algo-6).
      The claim and the block are separate identities: a class can win multiple blocks during its epoch budget,
      and when the chain/feed reports the same claim on more than one block this page keeps that relationship visible.
      Heartbeat blocks have neither a model nor a claim. Every claim is judged, not trusted: a panel of five bonded
      seats is drawn on-chain, each seat re-derives the committed execution and files a <b>signed receipt</b>, and a
      claim is approved only when <b>3 of 5</b> receipts agree (<code>ReceiptLicensed</code>). This page renders that
      record straight from the chain — the judgement itself is the network's, never this site's.</div>
    <h2 class="sec">Execution classes</h2>
    <div id="llmWeight"></div>
    <div id="llmClasses" class="cards"><div class="loading">Loading class facts…</div></div>
    <div id="llmClaimLinks"></div>
    <h2 class="sec">Verifier seats — who can verify what</h2>
    <div class="note"><b>Two counts, not one.</b> A claim already admitted draws <b>5</b> of these bonded
      seats and needs <b>3 matching receipts</b> (<code>ReceiptLicensed</code>) — that is the panel, never
      the admission bar. A class is minable only after the registry sees <b><code>requiredReadySeats</code></b>
      possession proofs on chain via <code>SeatReadinessProved</code>. That number is <b>derived per class</b>
      (ADR-0135): at least <b>7</b> (5 panel seats + 2 spare), and more for a class whose canonical job is
      heavy at the target utilization. On testnet-12 the 8k row starts in <b>Prefetching</b> and moves to
      <b>Probation</b> once 7 seats of distinct operators have proved readiness after the registry's grace;
      the 2M row is <b>closed at launch</b> by rule and takes no claim.
      Prefetching admits nothing, even if five operators have the file. The ticks below that say
      <i>holds</i> are the operator's deployment (the floor is derived — every seat verifies it by
      construction); a seat is <b>Ready</b> only when the chain counts it, which is the same
      <code>${esc(short(PANEL_BOND_TX,4))}:N</code> identity the approvals table shows.</div>
    <div id="llmSeatMatrix" class="tblscroll"></div>
    <h2 class="sec">The lifecycle a claim walks</h2>
    <div class="palw-flow" style="margin-bottom:6px">
      <div class="palw-stage ok"><span class="n">1</span><div class="t">Submitted</div><div class="d">A producer runs the class's model and mines the block with the execution committed in its header (roots over trace, output and execution).</div></div>
      <div class="palw-stage ok"><span class="n">2</span><div class="t">Panel bound</div><div class="d">Five bonded seats are drawn on-chain (<code>PanelBound</code>) — the producer cannot pick its judges.</div></div>
      <div class="palw-stage ok"><span class="n">3</span><div class="t">Verified</div><div class="d">Each seat re-derives the job from the served material and signs a receipt: <b>Valid</b>, <b>Unavailable</b> or <b>Incapable</b>.</div></div>
      <div class="palw-stage ok"><span class="n">4</span><div class="t">Approved</div><div class="d"><code>ReceiptLicensed</code> carries the 3-of-5 quorum of signed receipts; a withholding producer gets <code>ProducerDefaulted</code> and is slashed.</div></div>
      <div class="palw-stage wait"><span class="n">5</span><div class="t">Final</div><div class="d">A licensed claim can still be disputed in court; unchallenged through its challenge window (120 DAA on testnet-12, about 4 hours), it becomes <b>Final</b> and its weight counts.</div></div>
    </div>
    <div class="sec-row"><h2 class="sec">Submitted LLM work</h2><span class="sec-more dim" id="llmSubCount"></span></div>
    <div class="note" style="margin-top:4px">The <b>input</b> of an attempt-lane job is not chosen by anyone: it is a
      prompt derived deterministically from the block's own position, the class and the producer's bond (the anchor) —
      that is what makes it a lottery ticket nobody can pre-compute, and why it reads as random tokens. The
      <b>output</b> is what the model actually generated from it — a <b>single token</b>: on testnet-12 an
      attempt's job is its class's canonical prefill with one generated token (ADR-0117, one forward pass),
      the smallest unit that proves the model ran over the whole prompt, since up to five seats re-execute
      every claim and the material carries a full logits row per generated token. This lane certifies
      inference; long-form generation belongs to the free-prompt lane. Every panel seat re-derives both
      halves before signing.</div>
    <div class="note" style="margin-top:4px"><b>What this page shows, and what the chain publishes — two different
      things.</b> A claim puts COMMITMENTS on chain for its ANSWER — the roots over its trace, its output and its
      execution — and those bytes stay with the executor, reaching the five drawn seats over an authenticated pull.
      <b>The prompt depends on its mode, and testnet-12 arms both.</b> Under mode 1 (PublicDa) the commitment
      transaction carries the prompt's token ids <b>whole</b>: public and permanent, readable by anyone with a node.
      Under mode 2 (PanelDa) it carries only a digest, and the ids reach the five drawn seats over an authenticated
      pull. This page renders neither, which is a courtesy and not a protection — <b>do not submit a mode-1 prompt
      you would not publish</b>. Even under mode 2 a data-availability court close carries the ids, so a disputed
      prompt becomes public. Private unless disputed, never confidential. An attempt-lane input is shown outright because
      nobody chose it — anyone recomputes it from the block's anchor. An ANSWER becomes public exactly when somebody
      <b>demands</b> it — a data-availability accusation the executor answers on chain (ADR-0062) — and the
      demand costs the accuser if the answer comes. Those disclosures appear here marked
      <span class="pill blue">disclosed on demand</span>, and nothing else does. Until this release the page
      read the executor's own retention and printed what it found, which put the text in the one place the
      network's rules do not.</div>
    <div class="llm-chips" id="llmChips">
      <button data-f="llm" class="on">LLM models</button><button data-f="all">All classes</button><button data-f="floor">Floor only</button>
    </div>
    <div id="llmSubs"><div class="spin">Sweeping the chain…</div></div>
    <div class="sec-row"><h2 class="sec">Verification &amp; approvals</h2><span class="sec-more dim" id="llmEvCount"></span></div>
    <div id="llmEvents"><div class="spin">Sweeping lifecycle carriers…</div></div>
    <h2 class="sec">Check it yourself</h2>
    <div class="note">Approval on this chain is reproducible: an artifact is pinned by <b>root</b>, and an honest
      re-execution matches <b>bit-for-bit</b> — no tolerances, no judgement calls. Anyone can re-derive what a
      panel approved. The re-verification market at
      <a href="https://llm.misakascan.com/" target="_blank" rel="noopener">llm.misakascan.com</a> walks the deposit-backed path.</div>
    <pre class="cmd"># QWEN25-A16-8K · rebuild the exact artifact testnet-12 registered, and check it against the chain's pin
#    (the chain pins the class id and the root, not a filename)
cargo build --release --bin qwen25-convert --bin palw-class
./target/release/qwen25-convert Qwen2.5-1.5B-Instruct/ --a16 --n-ctx 8192 --out qwen25-1.5b-a16-8k.palwart
#    input model.safetensors SHA-256 dd924a11b4c220f3…; expect 1,799,359,436 bytes
./target/release/palw-class manifest --network testnet-12 qwen25-1.5b-a16-8k.palwart          # writes the .palwmanifest sidecar
./target/release/palw-class manifest --network testnet-12 --check qwen25-1.5b-a16-8k.palwart  # exit 1 on a mismatch
#    expect class ebf44d0aa09ff7d1…, inventory root 88096dc177826d88…

# the full path — conversion, memory, bond, panel duty:
#    docs/testnet12-join-mining.md §6</pre>`;
  const chips = document.getElementById("llmChips");
  if (chips) chips.addEventListener("click", e => {
    const b = e.target.closest("button"); if (!b) return;
    llmFilter = b.dataset.f;
    chips.querySelectorAll("button").forEach(x=>x.classList.toggle("on", x===b));
    paintLlm();
  });
  refreshLlm();
  armPoll(()=>{ const s=curSeg(); return s==="llm"||s==="blockdag"||s==="dag"||s==="palw"; }, refreshLlm, 5000);
  onBlockAdded(() => refreshLlm());    // push: sweep immediately on every new block
}

async function refreshLlm(){
  if (llmBusy) return; llmBusy = true;
  try { await refreshLlmInner(); }
  catch(e){ const el=document.getElementById("llmSubs"); if (el && !llmSubs.length) el.innerHTML = `<div class="err">Sweep failed: ${esc(e.message)}</div>`; }
  finally { llmBusy = false; }
}
async function refreshLlmInner(){
  // Class facts — the same RPC the operators read (budget, produced, pwu per class).
  refreshLlmJobs();   // fire-and-forget; the next paint picks it up
  const [facts, regRaw] = await Promise.all([
    Promise.all(LLM_CLASSES.map(c =>
      rpc("getPalwProducerFacts", { classId:c.id, bondTransactionId:"", bondIndex:0, withBond:false }).catch(()=>null))),
    palwRead("getPalwModelRegistry"),
  ]);
  const llmReg = llmRegistryIndex(regRaw);

  // ---- The two class ratios a miner actually needs, kept SEPARATE because they disagree on
  // purpose. ISSUANCE follows block counts: every block earns the same subsidy carve whatever
  // its class (ADR-0042 Decision 10 — PALW pay is a carve of the fixed emission, never an
  // addition; an attempt-lane chain block's reward is escrowed until its claim is Final), and
  // the epoch budgets are "blocks, never pwu" (derive_epoch_budgets_v2 over the on-chain share
  // table). CONSENSUS follows pwu: block_pwu_v1 over the class's live difficulty is what a
  // claim WEIGHS — security, reorg depth, slash exposure — not what it pays. So the floor mines
  // most blocks at the same per-block pay, while the LLM classes carry nearly all the weight.
  const wColor = { "PALW-BASE-0":"#675f87", "QWEN36":"#a855f7", "QWEN25-A16-2M":"#67e8f9", "QWEN25-A16-8K":"#34d399" };
  const wSpare = ["#fbbf24","#34d399","#fb7185","#f472b6"];
  const wRows = LLM_CLASSES.map((c,i)=>{
    const f = facts[i];
    const pwu = f ? (Number(f.pwu)||0) : 0;
    const budget = f ? (Number(f.epochBudgetBlocks)||0) : 0;
    const made = f ? (Number(f.epochProducedBlocks)||0) : 0;
    return { c, pwu, budget, designed: pwu*budget, realized: pwu*made,
             color: wColor[c.name] || wSpare[i % wSpare.length] };
  });
  const floorPwu = (wRows.find(r=>r.c.tag==="floor")||{}).pwu || 0;
  wRows.forEach(r=>{ r.mult = (r.c.tag!=="floor" && floorPwu>0 && r.pwu>0) ? Math.round(r.pwu/floorPwu) : null; });
  const bSum = wRows.reduce((s,r)=>s+r.budget,0);
  const wSum = wRows.reduce((s,r)=>s+r.designed,0);
  const rSum = wRows.reduce((s,r)=>s+r.realized,0);
  const llmB = bSum ? wRows.filter(r=>r.c.tag!=="floor").reduce((s,r)=>s+r.budget,0)/bSum*100 : 0;
  const llmW = wSum ? wRows.filter(r=>r.c.tag!=="floor").reduce((s,r)=>s+r.designed,0)/wSum*100 : 0;
  const fmtP = p => p<=0 ? "0" : p<1 ? p.toFixed(2) : p.toFixed(1);
  const wSeg = (r,val,sum,what,unit) => { const p = sum ? val/sum*100 : 0;
    return p<=0 ? "" : `<i style="width:${p}%;background:${r.color}" title="${esc(r.c.name)} — ${fmtP(p)}% of ${what} (${num(val)} ${unit})"></i>`; };
  const wEl = document.getElementById("llmWeight");
  if (wEl) wEl.innerHTML = !(bSum || wSum) ? "" : `
    <div class="wbar-wrap">
      <div class="wbar-head">
        <span class="wbar-title">Class ratios — issuance vs chain weight</span>
        <span class="wbar-big">LLM classes: <b class="amt-in">${fmtP(llmB)}%</b> <span class="dim" style="font-size:12px;font-weight:400">of block issuance</span> · <b class="amt-in">${fmtP(llmW)}%</b> <span class="dim" style="font-size:12px;font-weight:400">of chain weight</span></span>
      </div>
      <div class="wbar-row"><span class="wbar-lbl" title="the epoch's per-class block budgets from the on-chain share table — every block earns the SAME subsidy carve, so MSK issuance follows these block counts; as LLM production grows the retarget walks the floor's share down toward its liveness minimum (20‰ = 2% on testnet-12)">issuance (block budget)</span><div class="wbar">${bSum ? wRows.map(r=>wSeg(r,r.budget,bSum,"the epoch's block budget","blocks")).join("") : '<i class="none">no budgets visible</i>'}</div></div>
      <div class="wbar-row"><span class="wbar-lbl" title="pwu per block × epoch block budget — what the claims WEIGH in consensus (chain security, reorg depth, slash exposure), not what they pay">chain weight (pwu)</span><div class="wbar">${wSum ? wRows.map(r=>wSeg(r,r.designed,wSum,"the epoch weight budget","pwu")).join("") : '<i class="none">no weight visible</i>'}</div></div>
      <div class="wbar-row"><span class="wbar-lbl" title="pwu × blocks actually produced in the running epoch — resets at the epoch boundary">weight produced now</span><div class="wbar">${rSum ? wRows.map(r=>wSeg(r,r.realized,rSum,"the weight produced so far","pwu")).join("") : '<i class="none">no blocks yet this epoch</i>'}</div></div>
      <div class="wbar-legend">${wRows.map(r=>`<span><span class="swatch" style="background:${r.color}"></span>${esc(r.c.name)} <span class="dim">issuance</span> <b>${fmtP(bSum?r.budget/bSum*100:0)}%</b> · <span class="dim">weight</span> <b>${fmtP(wSum?r.designed/wSum*100:0)}%</b> <span class="dim">· ${num(r.pwu)} pwu/block${r.mult?` · ×${num(r.mult)} floor`:""}${r.c.tag==="floor"?" · liveness backbone":""}</span></span>`).join("")}</div>
      <div class="wbar-note">Every block earns the <b>same subsidy carve</b> whatever its class — PALW pay is a carve of the fixed
        emission, never an addition to it (an attempt-lane chain block's reward sits in <b>escrow until its claim is Final</b>;
        a voided claim burns it). So <b>issuance follows block counts</b>: the on-chain share table budgets each class's blocks
        per epoch, and as LLM production grows the per-class retarget walks the floor's share down toward its <b>liveness
        minimum</b> (20‰ = 2% on testnet-12). <b>Consensus follows pwu</b>: a claim weighs its class's verified work units, so chain security is carried
        almost entirely by the LLM classes even while the model-free floor — the liveness backbone — mines most blocks. An
        LLM block budget pays the same per block as the floor's, to any producer whose class the registry admits.</div>
    </div>`;

  const fc = document.getElementById("llmClasses");
  if (fc) fc.innerHTML = LLM_CLASSES.map((c,i)=>{
    const f = facts[i];
    const prod = f ? `${num(f.epochProducedBlocks)} / ${num(f.epochBudgetBlocks)}` : "—";
    const pwu  = f ? num(f.pwu) : "—";
    const ok   = f && f.available;
    // **Can a miner actually earn in this class right now?** Four gates, in the order they
    // bite: the chain must hold the class (available); the registry must admit claims
    // (Probation / ActiveLimited / Active — Prefetching and Held do not); `readySeatsNow`
    // must meet `requiredReadySeats` (class-derived — 7 for light tiers, higher for held 2M;
    // not the 5-seat panel draw); enough panel seats must
    // still hold the artifact to form a 3-of-5 licence; and the per-class epoch must have
    // opened a block budget (a class registered mid-epoch shows 0/0 until the next 1000-DAA
    // retarget boundary). Operator "holds" is not Ready.
    const nVer = verifierCount(c.name);
    const row = llmReg.byId[String(c.id).toLowerCase()] || null;
    const ready = row && row.readySeatsNow != null ? Number(row.readySeatsNow) : null;
    const need  = row && row.requiredReadySeats != null ? Number(row.requiredReadySeats) : null;
    const st    = row ? String(row.state || "") : "";
    // The floor is derived — every seat verifies it by construction, so it never files
    // SeatReadinessProved and readySeatsNow stays 0. requiredReadySeats applies to model classes.
    const admits = c.tag === "floor" ? true : (row ? registryAdmitsClaims(st) : false);
    const readyOk = c.tag === "floor" ? true : (need == null ? false : (ready != null && ready >= need));
    const quorumOk  = nVer >= PANEL_QUORUM || c.tag === "floor";
    const budgetOpen = !f || Number(f.epochBudgetBlocks) > 0;
    const minable = ok && admits && readyOk && quorumOk;
    const readyLbl = (ready == null || need == null) ? "—" : `${num(ready)} / ${num(need)}`;
    let status;
    if (!ok)            status = '<span class="dim">unavailable on chain</span>';
    else if (c.tag !== "floor" && !row) status = '<span class="pill blue" title="getPalwModelRegistry did not return this class — admission unread">readiness unread</span>';
    else if (!admits || !readyOk) status = `<span class="pill red" title="${esc((row && row.reason) || "this class does not admit claims until requiredReadySeats ("+need+") seats have proved possession on chain; the panel is 5 drawn / 3-of-5 after that")}">OFF — ${esc(st || "not admitted")} · ready ${readyLbl}</span>`;
    else if (!quorumOk) status = `<span class="pill red" title="a claim needs ${PANEL_QUORUM} of its ${PANEL_DRAW} drawn seats to verify it; only ${nVer} seat${nVer===1?"":"s"} hold this artifact — blocks mined in this class would default and slash">OFF — panel holders ${nVer}/${PANEL_QUORUM}</span>`;
    else if (!budgetOpen) status = `<span class="pill blue" title="registered mid-epoch: the block budget opens at the next per-class retarget boundary (every 1000 DAA); registry admission is open">⛏ ready — budget opens next epoch</span>`;
    else                status = `<span class="pill chain" title="registry admits claims; ${num(ready)} / ${num(need)} seats have proved possession; a well-formed claim draws ${PANEL_DRAW} and licenses at ${PANEL_QUORUM}-of-${PANEL_DRAW}">⛏ minable — ready ${readyLbl}</span>`;
    return `<div class="card${minable?"":" off"}"><div class="k">${esc(c.name)} ${c.tag==="floor"?'<span class="pill blue">floor</span>':'<span class="pill chain">LLM</span>'}</div>
      <div class="v sm">${esc(c.model)}</div>
      <div class="sub">${status}</div>
      <div class="sub">epoch: <b>${prod}</b> blocks · claims ${pwu} pwu/block</div>
      <div class="sub">issuance <b>${fmtP(bSum?wRows[i].budget/bSum*100:0)}%</b> of epoch blocks · weight <b>${fmtP(wSum?wRows[i].designed/wSum*100:0)}%</b>${wRows[i].mult?` · ×${num(wRows[i].mult)} floor pwu`:""}</div>
      <div class="sub mono" title="${esc(c.id)}">class ${esc(short(c.id,8))}</div></div>`;
  }).join("");
  const sm = document.getElementById("llmSeatMatrix");
  if (sm) sm.innerHTML = `<table class="tbl"><thead><tr>
      <th>Panel seat (bond)</th>${LLM_CLASSES.map(c=>`<th title="${esc(c.model)}">${esc(c.name)}</th>`).join("")}
    </tr></thead><tbody>${PANEL_SEATS.map(seat=>{
      const bond = `${short(PANEL_BOND_TX,4)}:${seat.ix}`;
      return `<tr><td><span class="mono" title="${esc(PANEL_BOND_TX)}:${seat.ix}">${esc(bond)}</span> <span class="dim">${esc(seat.host)}</span></td>${
        LLM_CLASSES.map(c=>{
          const holds = seat.holds.includes(c.name);
          const proof = llmReg.proved[seat.ix + ":" + String(c.id).toLowerCase()];
          const provedOk = proof && proof.fresh !== false && !proof.notReadyReason;
          if (provedOk) return `<td><span class="amt-in" title="SeatReadinessProved on chain at DAA ${esc(String(proof.provedDaa == null ? "—" : proof.provedDaa))} — this seat counts toward requiredReadySeats">✓ ready</span></td>`;
          if (proof && proof.notReadyReason) return `<td><span class="bad" title="${esc(proof.notReadyReason)}">not ready</span></td>`;
          if (c.tag === "floor") return `<td><span class="dim" title="every seat verifies the floor by construction">floor</span></td>`;
          if (holds) return `<td><span class="dim" title="the operator's notes say this seat has the file, but the chain has no fresh SeatReadinessProved — it does not count toward the 7 and is not on any panel for this class">not proved</span></td>`;
          return `<td><span class="dim" title="no artifact on this seat — drawn onto a panel for this class it files Incapable, which counts toward neither side">—</span></td>`;
        }).join("")}</tr>`;
    }).join("")}
    <tr class="sumrow"><td class="dim" title="the operator's deployment notes — not a chain fact, and not a running panel">operator notes: has the file</td>${LLM_CLASSES.map(c=>{
      if (c.tag === "floor") return `<td class="dim">—</td>`;
      const n = verifierCount(c.name);
      return `<td class="dim" title="deployment notes, not proofs: only the row below counts">${n} noted</td>`;
    }).join("")}</tr>
    <tr class="sumrow"><td class="dim">chain ready (need requiredReadySeats)</td>${LLM_CLASSES.map(c=>{
      if (c.tag === "floor") return `<td class="dim" title="every node verifies the floor; it is not readiness-gated">not gated</td>`;
      const row = llmReg.byId[String(c.id).toLowerCase()] || null;
      const ready = row && row.readySeatsNow != null ? Number(row.readySeatsNow) : null;
      const need  = row && row.requiredReadySeats != null ? Number(row.requiredReadySeats) : 7;
      if (ready == null) return `<td class="dim">— / ${num(need)}</td>`;
      const ok = ready >= need;
      return `<td>${ok ? `<b class="amt-in">${num(ready)}</b>` : `<b class="bad">${num(ready)}</b>`} / ${num(need)}</td>`;
    }).join("")}</tr>
    </tbody></table>`;

  // Chain sweep — RECENT WINDOW ONLY. The first cursor used to be the pruning point ("young
  // enough to read whole"), and the chain outgrew that within a day: getBlocks with transactions
  // is heavy, twelve serial pages hit the rpc timeout partway up the history, and the sweep
  // re-started from the same early segment every poll — the page showed day-old floor blocks and
  // never reached today's LLM ones. History now comes from the server-side feed
  // (llm-jobs.json, which already decodes every LLM claim); the live sweep only covers the last
  // ~24 chain blocks and their mergesets, so it always completes inside one poll.
  for (const [claim, row] of llmFpJobs){
    if (llmSeenClaims.has(claim)) continue;
    llmSeenClaims.add(claim);
    const cls = LLM_CLASSES.find(c => c.name === row.class);
    llmTotalSubs++;
    llmSubs.push({ block: row.block || "", ts: Number(row.ts||0), daa: Number(row.daa||0) || null, chain: null,
                   classId: cls ? cls.id : "", pwu: row.work_leaves || null, bondTx: "", bondIx: 0, execRoot: "", claim, fp: true, feedOnly: true });
  }
  for (const [blockHash, row] of llmJobs){
    if (llmSeenBlocks.has(blockHash)) continue;
    llmSeenBlocks.add(blockHash);
    const cls = LLM_CLASSES.find(c => c.name === row.class);
    llmTotalSubs++;
    llmSubs.push({ block: blockHash, ts: Number(row.ts||0), daa: Number(row.daa||0) || null,
                   chain: null, classId: cls ? cls.id : "", pwu: null,
                   bondTx: "", bondIx: 0, execRoot: "", feedOnly: true });
  }
  if (!llmCursor){
    const dag = await rpc("getBlockDagInfo");
    let hash = dag.sink, hops = 0;
    // Walk selected parents back ~24 chain blocks; each hop is one cheap header fetch.
    while (hops < 24){
      let b; try { b = await rpc("getBlock", { hash, includeTransactions:false }, 8000); } catch { break; }
      const parent = (b.block||b).header && ((b.block||b).header.selectedParentHash || ((b.block||b).header.parents||[[]])[0][0]);
      if (!parent) break;
      hash = parent; hops++;
    }
    llmCursor = hash;
  }
  const chainSeen = [];   // chain blocks met this refresh, in arrival order — the overlap anchor pool
  for (let page=0; page<4; page++){
    let gb; try { gb = await rpc("getBlocks", { lowHash: llmCursor, includeBlocks:true, includeTransactions:true }, 15000); } catch { break; }
    const blocks = gb.blocks || [];
    let fresh = 0;
    for (const blk of blocks){
      const hd = blk.header || {}, vd = blk.verboseData || {};
      const bh = hd.hash || vd.hash;
      if (!bh || llmSeenBlocks.has(bh)) continue;
      llmSeenBlocks.add(bh); fresh++;
      if (hd.palwCommitment){
        const a = llmDecodeAttempt(hd.palwCommitment);
        if (a) { llmTotalSubs++; llmSubs.unshift({ block:bh, ts:Number(hd.timestamp||0), daa:Number(hd.daaScore||0),
                                 chain: vd.isChainBlock !== false, classId:a.classId, pwu:a.pwu,
                                 bondTx:a.bondTx, bondIx:a.bondIx, execRoot:a.execRoot }); }
      }
      for (const t of (blk.transactions||[])){
        const s = String(t.subnetworkId||"").toLowerCase();
        if (!(s.startsWith("4b") && /^0*$/.test(s.slice(2)))) continue;
        const txid = t.verboseData && t.verboseData.transactionId;
        if (!txid || llmSeenTx.has(txid)) continue;
        llmSeenTx.add(txid);
        const d = llmDecodeLifecycle(t.payload);
        if (d) llmEvents.unshift({ ts:Number(hd.timestamp||0), block:bh, txid, ...d });
        // A free-prompt claim is submitted work exactly as an attempt block is — it rides a
        // transaction instead of a header, so it is a row of the same table, typed.
        if (d && d.kind === "FreePromptCommitted" && !llmSeenClaims.has(d.claim)){
          llmSeenClaims.add(d.claim); llmTotalSubs++;
          llmSubs.unshift({ block:bh, ts:Number(hd.timestamp||0), daa:Number(hd.daaScore||0), chain: vd.isChainBlock !== false,
                            classId:d.classId, pwu:d.pwu, bondTx:d.bondTx, bondIx:d.bondIx, execRoot:"", claim:d.claim, fp:true });
        }
      }
      // The cursor must stay ON the selected chain: getBlocks(lowHash) walks forward from its
      // argument, and a red/side block (every slow-class block, i.e. exactly the rows this page
      // exists for) has no forward — anchoring there froze the sweep at the first QWEN block.
      if (vd.isChainBlock !== false){ llmCursor = bh; chainSeen.push(bh); }
    }
    if (!fresh || blocks.length <= 1) break;
  }
  // And then STEP THE CURSOR BACK: a side block (every slow-class one) is parallel to the chain
  // tip, so it never sits in the tip's own future cone — a cursor parked at the tip is blind to
  // exactly the rows this page exists for. Re-anchoring ~20 chain blocks behind keeps a rolling
  // overlap window; llmSeenBlocks makes the re-reads free.
  if (chainSeen.length) llmCursor = chainSeen[Math.max(0, chainSeen.length - 20)];
  llmSubs.sort((a,b)=>b.ts-a.ts);
  { // cap the floor's flood without ever dropping a model-class submission
    const models = llmSubs.filter(r=>!llmIsFloor(r.classId));
    const floor  = llmSubs.filter(r=> llmIsFloor(r.classId)).slice(0,400);
    llmSubs = models.concat(floor).sort((a,b)=>b.ts-a.ts);
  }
  llmEvents.sort((a,b)=>b.ts-a.ts); llmEvents = llmEvents.slice(0,400);
  paintLlm();
}

// **Who made this block, and who checked it** — answered from the block's OWN header wherever it
// can be, because the server-side feed is a convenience and its silence was being read as the
// chain's.
//
// The old block page said "the feed has not decoded this block's job yet" for EVERY PALW-era lane
// id. On a heartbeat block (algo-8) that sentence is permanently false: the heartbeat lane is a
// plain hash lane (ADR-0066 Decision 1) with no class, no inference and no panel — there is no job
// to decode, and there never will be. A reader was left waiting for something that does not exist.
//
// The attempt lane's answer, meanwhile, was sitting in the header the whole time: `palwCommitment`
// is a `PalwAttemptEnvelopeV2` and `llmDecodeAttempt` already parses it. Class, producer bond, work
// units and all four roots come out of it with no feed and no extra request.

/// The seat behind a bond outpoint, named where the deployment knows it.
function seatName(tx, ix){
  if (String(tx).toLowerCase() !== PANEL_BOND_TX) return null;
  const s = PANEL_SEATS.find(s => s.ix === Number(ix));
  return s ? s.host : null;
}
function bondCell(tx, ix){
  const name = seatName(tx, ix);
  const mono = `<span class="mono" title="bond ${esc(String(tx))}:${esc(String(ix))}">${esc(short(String(tx),6))}:${esc(String(ix))}</span>`;
  return name ? `${esc(name)} <span class="dim">· ${mono}</span>` : mono;
}

/// What the chain's phase word actually means, in the reader's terms. The phase is the node's own
/// verdict on whether the work was judged — never this page's arithmetic.
const PALW_PHASE = {
  provisional:     { label:"Provisional",      judged:false, meaning:"submitted; no panel has judged it yet" },
  panel_bound:     { label:"Panel bound",      judged:false, meaning:"five bonded seats have been drawn \u2014 judgement is in flight" },
  receipt_licensed:{ label:"Approved",         judged:true,  meaning:"3 of 5 drawn seats signed matching receipts; still challengeable in court" },
  final:           { label:"Final",            judged:true,  meaning:"licensed and unchallenged through the window \u2014 no longer disputable" },
  voided:          { label:"Voided",           judged:false, meaning:"the chain decided this work did not happen as claimed" },
};
function palwPhase(raw){
  const k = String(raw||"").toLowerCase();
  return PALW_PHASE[k] || { label: raw || "\u2014", judged:false, meaning:"" };
}

/// The production half: everything the header itself states.
function blockProductionSection(hd, feedRow){
  const algo = Number(hd.powAlgoId);
  const raw = hd.palwCommitment;
  const att = (raw && raw.length) ? llmDecodeAttempt(raw) : null;

  // Heartbeat: the honest answer is that no model was involved, stated once and plainly.
  if (algo === 8){
    return `<h2 class="sec">How this block was produced</h2>
      <div class="note"><b>No model produced this block.</b> It was mined on the <b>heartbeat lane</b>
      (algo-8, ADR-0066 Decision 1) — a plain self-verifying hash lane that keeps the chain moving when no
      model block is ready. It carries no execution class, no inference, no committed roots and no panel,
      so there is nothing here for seats to verify. Its header's <code>palwCommitment</code> is empty, which
      is what a heartbeat block looks like when it is correct. Model-produced blocks are on the
      <b>attempt lane</b> (algo-6) and free-prompt work rides a transaction; both are listed on the
      <a href="#/llm">LLM Jobs</a> page.</div>`;
  }
  if (!att){
    return `<h2 class="sec">How this block was produced</h2>
      <div class="note">This block's header carries no PALW attempt envelope, so no model, class or producer
      can be named from it. Lane ${powLaneCell(hd.powAlgoId)}.</div>`;
  }

  const cls = LLM_CLASS_BY_ID[att.classId];
  const floor = llmIsFloor(att.classId);
  const model = floor
    ? `<b>PALW-BASE-0</b> <span class="dim">— the deterministic integer floor. No model file: every seat verifies it by construction, which is why it is the liveness backbone.</span>`
    : (cls ? `<b>${esc(cls.name)}</b> <span class="dim">— ${esc(cls.model)}</span>`
           : `<span class="mono">${esc(short(att.classId,8))}</span> <span class="dim">— a class this site does not have a name for; the id is the chain's.</span>`);

  const claim = feedRow && feedRow.claim ? feedRow.claim : null;
  const rows = [
    ["Lane", powLaneCell(hd.powAlgoId) + ` <span class="dim">— each verified draw can win a separate block; the class budget is shown above</span>`],
    [floor ? "Base" : "Model", model],
    ["Class id", `<span class="hash" title="${esc(att.classId)}">${esc(short(att.classId,10))}</span>`],
    ["Artifact root", `<span class="hash" title="${esc(att.artifactRoot)}">${esc(short(att.artifactRoot,10))}</span> <span class="dim">— the exact weights this execution ran against</span>`],
    ["Produced by", bondCell(att.bondTx, att.bondIx)],
    ...(claim ? [["Claim", `<span class="hash" title="${esc(claim)}">${esc(short(claim,10))}</span> <span class="dim">— this block's claim identity</span>`]] : []),
    ["Work claimed", `${num(att.pwu)} <span class="dim">pwu</span>`],
    ["Trace root", `<span class="hash" title="${esc(att.traceRoot)}">${esc(short(att.traceRoot,10))}</span>`],
    ["Output root", `<span class="hash" title="${esc(att.outputRoot)}">${esc(short(att.outputRoot,10))}</span>`],
    ["Execution root", `<span class="hash" title="${esc(att.execRoot)}">${esc(short(att.execRoot,10))}</span>`],
  ];
  return `<h2 class="sec">How this block was produced <span class="dim" style="font-size:13px">— read from this block's own header</span></h2>
    <div class="kv">${rows.map(r=>`<div class="row"><div class="key">${r[0]}</div><div class="val">${r[1]}</div></div>`).join("")}</div>
    <h2 class="sec">Who verified it</h2>
    <div id="blkVerify" data-claim="${esc(claim||"")}"><div class="spin">Asking the chain…</div></div>`;
}

/// The verification half, filled in after paint: the phase is the node's own state (one request,
/// authoritative), the seat identities come from the licensing carrier (a bounded forward sweep,
/// best-effort — and it says so when it finds nothing rather than implying nobody signed).
async function fillBlockVerification(blockHash){
  const box = document.getElementById("blkVerify");
  if (!box) return;
  const claim = box.getAttribute("data-claim");
  if (!claim){
    box.innerHTML = `<div class="note">This block's claim id is not in the decoded feed yet, so its panel cannot be
      looked up here. The verification record for every claim is on the <a href="#/llm">LLM Jobs</a> page under
      <b>Verification &amp; approvals</b>; a claim is approved when 3 of its 5 drawn seats file matching receipts.</div>`;
    return;
  }
  let r = null;
  try { r = await rpc("getPalwFreePromptClaim", { claimId: claim }, 12000); } catch {}
  if (!r || !r.found){
    box.innerHTML = `<div class="note">The node this page reads does not hold claim
      <span class="hash">${esc(short(claim,10))}</span> in its state — it may have been pruned from the claim map,
      or this vantage is behind. The claim id is the chain's; the panel record for it is on the
      <a href="#/llm">LLM Jobs</a> page.</div>`;
    return;
  }
  const ph = palwPhase(r.phase);
  const meaning = ph.meaning;
  const pill = ph.judged ? `<span class="pill chain">${esc(ph.label)} \u2713</span>`
             : String(r.phase).toLowerCase() === "voided" ? `<span class="pill red">${esc(ph.label)} \u2717</span>`
             : `<span class="pill blue">${esc(ph.label)}</span>`;
  const head = [
    ["Claim", `<span class="hash" title="${esc(claim)}">${esc(short(claim,10))}</span>`],
    ["Phase", `${pill} ${meaning?`<span class="dim">— ${esc(meaning)}</span>`:""}${r.phaseDaa?` <span class="dim">· at DAA ${num(r.phaseDaa)}</span>`:""}`],
    ["Accepted", r.acceptedBlock ? `${linkBlock(r.acceptedBlock)} <span class="dim">· DAA ${num(r.acceptedDaa)}</span>` : '<span class="dim">—</span>'],
    ["Producer bond", r.executorBond ? `<span class="mono">${esc(r.executorBond)}</span>` : '<span class="dim">—</span>'],
  ];
  box.innerHTML = `<div class="kv">${head.map(x=>`<div class="row"><div class="key">${x[0]}</div><div class="val">${x[1]}</div></div>`).join("")}</div>
    <div id="blkSeats"><div class="spin">Looking for the licensing carrier…</div></div>`;

  // **The seats, without pretending to a chain scan.**
  //
  // `ReceiptLicensed` rides a 0x4b transaction in some LATER block, and "later" is not near: the
  // licence waits for the claim's receipt window and `Final` waits its challenge window past that
  // (120 DAA on testnet-12), so the carrier for a finalized claim sits well beyond its acceptance. The forward
  // walk this function used to do reached none of it and reported "0 pages", which reads as
  // "nobody signed" — the one thing it must never say by accident.
  //
  // So it uses what is already free (the LLM Jobs page's sweep fills `llmEvents` for the session)
  // and otherwise says where the list is and why it is not here. The PHASE above is the
  // load-bearing answer either way, and it is the node's own state rather than this page's
  // arithmetic: `receipt_licensed` means three of the five drawn seats signed matching receipts.
  const seatBox = document.getElementById("blkSeats");
  if (!seatBox) return;
  const swept = llmEvents.find(e => e.kind === "ReceiptLicensed" && e.claim === claim);
  const receipts = swept ? (swept.receipts || []) : null;
  if (!receipts){
    seatBox.innerHTML = `<div class="note">${ph.judged
        ? `Three of the five drawn seats signed matching receipts — that is what <b>${esc(ph.label)}</b> is, and it is
           the node's own state that says so, not this page. Which bonds those were rides the licensing carrier, a
           transaction in a later block, so this page does not walk out to fetch it: the seats are
           listed on the <a href="#/llm">LLM Jobs</a> page under <b>Verification &amp; approvals</b>.`
        : `No seat has filed a receipt yet, and that is what this phase means: ${esc(meaning)}. Five bonded seats are
           drawn on chain — the producer cannot pick its judges — and the licence is carried once three of them agree.`}</div>`;
    return;
  }
  const tally = receipts.reduce((m,x)=>{ m[x.verdict]=(m[x.verdict]||0)+1; return m; },{});
  seatBox.innerHTML = `<div class="note" style="margin-bottom:6px">${receipts.length} signed receipt${receipts.length===1?"":"s"} —
    ${Object.entries(tally).map(([k,n])=>`${n}× ${esc(k)}`).join(", ")}. Each seat re-derived this execution from the
    material the producer served and signed its own verdict; the producer could not choose its judges.</div>
    <table class="tbl"><thead><tr><th>Panel seat</th><th>Verdict</th><th class="num">Signed at DAA</th></tr></thead><tbody>${
      receipts.map(x=>`<tr>
        <td>${bondCell(x.seat.tx, x.seat.ix)}</td>
        <td>${x.verdict==="Valid"?'<span class="pill chain">Valid ✓</span>'
             :x.verdict==="Unavailable"?'<span class="pill red">Unavailable</span>'
             :'<span class="pill">'+esc(x.verdict)+'</span>'}</td>
        <td class="num">${num(x.signedDaa)}</td></tr>`).join("")
    }</tbody></table>`;
}
function llmSeatList(seats){
  return seats.map(s=>`<span class="mono" title="bond ${esc(s.bond.tx)}:${s.bond.ix}">${esc(short(s.bond.tx,4))}:${s.bond.ix}</span>`).join(", ");
}
// The block header names the execution; the claim id arrives from the decoded job feed (or from
// the free-prompt transaction). Keep the lookup in one place for all claim/block projections.
function llmClaimForRow(row){
  if (row.claim) return row.claim;
  const job = row.fp ? llmFpJobs.get(row.claim) : llmJobs.get(row.block);
  return job && job.claim ? job.claim : null;
}
function paintLlm(){
  const modelCount = llmSubs.filter(r=>!llmIsFloor(r.classId)).length;
  const shown = llmFilter==="all" ? llmSubs
              : llmFilter==="floor" ? llmSubs.filter(r=> llmIsFloor(r.classId))
              : llmSubs.filter(r=>!llmIsFloor(r.classId));
  const sc = document.getElementById("llmSubCount");
  if (sc) sc.textContent = `${num(llmTotalSubs)} on chain · ${num(modelCount)} from LLM model classes`;
  const claimGroups = new Map();
  for (const row of llmSubs){
    const claim = llmClaimForRow(row);
    if (!claim) continue;
    let group = claimGroups.get(claim);
    if (!group){ group = { claim, rows: [] }; claimGroups.set(claim, group); }
    if (!group.rows.some(x => x.block === row.block)) group.rows.push(row);
  }
  const rel = document.getElementById("llmClaimLinks");
  if (rel){
    const groups = Array.from(claimGroups.values()).sort((a,b)=>Number(b.rows[0]?.ts||0)-Number(a.rows[0]?.ts||0));
    rel.innerHTML = `<h2 class="sec">Claim ↔ blocks</h2>
      <div class="note">Claims are the inference identity; blocks are the accepted or merged block records that carry it.
        A class may win multiple blocks in its epoch budget. This table groups every decoded block by claim, so a
        repeated claim is visible as <b>one claim → multiple blocks</b>. Rows whose claim feed has not arrived yet
        remain visible in the submissions table with their block identity.</div>` +
      (groups.length ? `<div class="tblscroll"><table class="tbl"><thead><tr><th>Claim</th><th>Class</th><th class="num">Blocks</th><th>Block records</th></tr></thead><tbody>${groups.slice(0,60).map(g=>{
        const first = g.rows[0];
        const cls = LLM_CLASS_BY_ID[first.classId] ? llmClassName(first.classId) : short(first.classId,6);
        return `<tr><td class="hash" title="${esc(g.claim)}">${esc(short(g.claim,10))}</td><td>${esc(cls)}</td><td class="num"><b>${g.rows.length}</b></td><td>${g.rows.map(x=>linkBlock(x.block)).join(" · ")}</td></tr>`;
      }).join("")}</tbody></table></div>`
      : `<div class="note">No claim↔block links have been decoded in the current window yet. The block rows remain visible while the job feed catches up.</div>`);
  }
  const sw = document.getElementById("llmSubs");
  if (sw){
    sw.innerHTML = !shown.length
      ? (llmFilter==="llm"
          ? `<div class="note">No LLM-model submissions in the decoded window yet — the floor produces most blocks
             (one per ~120 s needs no model), while a model-class block must win the class lottery on a real
             inference, so its blocks are rarer by design. Every one that lands appears here; switch to
             <b>All classes</b> for the full stream.</div>`
          : `<div class="spin">No submissions decoded yet…</div>`) :
    `<div class="tblscroll"><table class="tbl"><thead><tr>
      <th>Age</th><th>Block</th><th>Claim</th><th>Class</th><th class="num">pwu claimed</th><th>Input</th><th>Output</th><th>Type</th>
    </tr></thead><tbody>${shown.slice(0,30).map(r=>{
      const job = r.fp ? llmFpJobs.get(r.claim) : llmJobs.get(r.block);
      const claim = llmClaimForRow(r);
      const meta = `producer bond ${r.bondTx}:${r.bondIx}\nexecution root ${r.execRoot}` + (claim?`\nclaim ${claim}`:"");
      let inCell, outCell;
      if (r.fp){
        // **A free prompt is the author's, and the answer is the executor's to serve** — neither is
        // published here. The claim's own identity is the commitment a reader holds onto; the
        // derived artifacts below it ARE on chain (ADR-0078) and stay.
        const derived = job && job.derived && job.derived.length
          ? ` <span class="dim">→ derived: ${job.derived.map(d=>esc(`${d.kind_name||("kind "+d.kind)} ${d.artifact_bytes} B`)).join(", ")}</span>` : "";
        inCell  = llmDisclosedCell(job && job.disclosed)
          || llmPromptOnChain(job && job.prompt_tokens ? ` <span class="dim">· ${num(job.prompt_tokens)} tokens</span>` : "");
        outCell = llmSealed("output", "", (r.pwu?` <span class="dim">· ${num(r.pwu)} leaves</span>`:"") + derived);
      } else if (llmIsFloor(r.classId)){
        inCell  = '<span class="dim">deterministic integer job (no tokens)</span>';
        outCell = '<span class="dim">—</span>';
      } else if (job){
        // The attempt lane's INPUT stays: it is not anybody's words but a draw the block itself
        // fixes, and any reader recomputes it from the anchor. Its OUTPUT does not: that lives in
        // the executor's retention like every other answer.
        const inT  = job.prompt_text != null ? job.prompt_text : (job.prompt_ids||[]).join(" ");
        inCell  = llmInCell(inT, job.prompt_ids, true);
        outCell = llmDisclosedCell(job.disclosed) || llmOutCell(null, null);
      } else {
        inCell  = '<span class="dim">decoding…</span>';
        outCell = '<span class="dim">decoding…</span>';
      }
      return `<tr title="${esc(meta)}"><td class="dim nowrap" title="${esc(dt(r.ts))}">${ago(r.ts)}</td>
        <td class="nowrap">${linkBlock(r.block)}</td>
        <td>${claim?`<span class="hash" title="${esc(claim)}">${esc(short(claim,8))}</span>`:'<span class="dim">pending decode</span>'}</td>
        <td class="nowrap">${LLM_CLASS_BY_ID[r.classId] ? esc(llmClassName(r.classId)) : `<span class="mono">${esc(short(r.classId,6))}</span>`}</td>
        <td class="num">${r.pwu==null?'<span class="dim">—</span>':num(r.pwu)}</td>
        <td>${inCell}</td>
        <td>${outCell}</td>
        <td class="nowrap">${r.fp?'<span class="pill blue" title="free-prompt lane (ADR-0044): a prompt somebody typed, committed by its executor as a claim">free-prompt</span>':(llmIsFloor(r.classId)?'<span class="pill" title="the deterministic integer floor: no model, no tokens">floor</span>':'<span class="pill" title="attempt lane: the prompt is derived from the block anchor — a lottery ticket nobody chose">attempt</span>')} ${r.chain==null?'':r.chain?'<span class="pill chain">chain</span>':'<span class="pill red">merged</span>'}</td></tr>`;}).join("")}
    </tbody></table></div>`;
  }
  const ec = document.getElementById("llmEvCount");
  if (ec) ec.textContent = `${llmEvents.length} lifecycle objects`;
  const ew = document.getElementById("llmEvents");
  if (ew){
    const row = e => {
      let pill = '<span class="pill std">'+esc(e.kind)+'</span>', what = "";
      if (e.kind === "PanelBound"){
        pill = '<span class="pill blue">Panel bound</span>';
        what = `5-seat panel drawn for claim — seats ${llmSeatList(e.seats||[])}`;
      } else if (e.kind === "ReceiptLicensed"){
        pill = '<span class="pill chain">Approved ✓</span>';
        const v = (e.receipts||[]).reduce((m,r)=>{ m[r.verdict]=(m[r.verdict]||0)+1; return m; },{});
        what = `${(e.receipts||[]).length} signed receipts (${Object.entries(v).map(([k,n])=>n+"× "+k).join(", ")}) — quorum stood; verified by ${llmSeatList((e.receipts||[]).map(r=>({bond:r.seat})))}`;
      } else if (e.kind === "ProducerDefaulted"){
        pill = '<span class="pill red">Defaulted ✗</span>';
        what = `panel could not obtain the committed data — claim voided, stake slashed`;
      } else if (e.kind === "CourtOpened"){
        pill = '<span class="pill" style="color:var(--warn);border-color:#fbbf2444">Court opened</span>';
        what = `a licensed claim is disputed — session ${short(e.session||"",6)}`;
      } else if (e.kind === "BondRegistered"){
        what = `a new producer/seat bond joined the registry`;
      } else if (e.kind === "ClassRegistered"){
        what = `execution class registered: ${esc(llmClassName(e.classId))}`;
      } else if (e.kind === "FreePromptCommitted"){
        pill = '<span class="pill blue">Free prompt</span>';
        what = `a free-prompt inference committed on ${esc(llmClassName(e.classId))} — ${e.pwu?num(e.pwu)+" leaves":""} (text stays with its author)`;
      } else if (e.kind === "ClassLaneCertified"){
        pill = '<span class="pill chain">Lane certified</span>';
        what = `${esc(llmClassName(e.classId))} bound to the ${esc(e.lane)} lane (ADR-0075)`;
      } else if (e.kind === "FamilyCertified"){
        pill = '<span class="pill chain">Family certified</span>';
        what = `a family's drill evidence was re-graded by the court and recorded (ADR-0075)`;
      } else if (e.kind === "DerivedArtifactV1"){
        pill = '<span class="pill blue">Derived</span>';
        what = `a derived artifact (${e.artifactBytes} B, kind ${e.artifactKind}) committed for the claim (ADR-0078)`;
      } else if (e.kind === "ObjectChunk"){
        what = `part ${e.index+1} of ${e.count} of a chunked object (group ${short(e.group||"",6)})`;
      } else if (e.kind === "BondCapabilityDeclared"){
        what = `a bond declared the classes it can verify (ADR-0065)`;
      }
      return `<tr><td class="dim" title="${esc(dt(e.ts))}">${ago(e.ts)}</td><td>${pill}</td>
        <td class="mono" title="${esc(e.claim||e.session||"")}">${(e.claim||e.session)?esc(short(e.claim||e.session,8)):'<span class="dim">—</span>'}</td>
        <td style="font-size:12.5px">${what||'<span class="dim">—</span>'}</td>
        <td>${linkBlock(e.block)}</td></tr>`;
    };
    ew.innerHTML = !llmEvents.length ? `<div class="note">No lifecycle objects in the recent window yet — the
      panel seats may still be collecting receipts (a <code>ReceiptLicensed</code> carrier lands once a 3-of-5
      quorum stands; on testnet-12 the first floor licences were expected about 40–50 minutes after launch).
      This page sweeps only the most recent blocks. Panel <i>bindings</i> are derived by every node from state
      and never ride a transaction, so they are deliberately absent here.</div>` :
    `<div class="tblscroll"><table class="tbl"><thead><tr>
      <th>Age</th><th>Event</th><th>Claim</th><th>What the chain recorded</th><th>Block</th>
    </tr></thead><tbody>${llmEvents.slice(0,30).map(row).join("")}</tbody></table></div>`;
  }
}

/* ----------------------- LATEST TRANSACTIONS (full page) -------------- */
let txPageBusy=false;
async function renderTransactions(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer=null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Transactions</div>
    <h1 class="page">Latest transactions <span class="dim" style="font-size:14px">(node-direct · excludes coinbase issuance)</span></h1>
    <div id="txPageWrap"><div class="spin">Loading transactions…</div></div>`;
  await refreshTxPage();
  armPoll(()=>{ const s=curSeg(); return s==="transactions"||s==="txs"; }, refreshTxPage, 5000);
  onBlockAdded(() => refreshTxPage());
}
async function refreshTxPage(){
  if (txPageBusy) return; txPageBusy=true;
  try {
    const { blocks } = await collectRecentBlocks({ walkBack:45, maxBlocks:90, includeTx:true });
    const wrap = document.getElementById("txPageWrap"); if (!wrap) return;
    blocks.sort((a,b)=>Number((b.header&&b.header.daaScore)||0)-Number((a.header&&a.header.daaScore)||0));
    const seen=new Set(); const rows=[];
    for (const b of blocks){ const bh=b.header&&b.header.hash, bts=Number((b.header&&b.header.timestamp)||0);
      for (const t of (b.transactions||[])){ if (isCoinbaseTx(t)) continue;
        const id=t.verboseData&&t.verboseData.transactionId; if (!id||seen.has(id)) continue; seen.add(id);
        const outs=t.outputs||[]; const val=outs.reduce((a,o)=>a+Number(o.value||0),0);
        rows.push({ id, kind:txKind(t), addr:largestOutAddr(t), val, ins:(t.inputs||[]).length, outs:outs.length, block:bh, ts:bts });
        if (rows.length>=120) break; }
      if (rows.length>=120) break; }
    if (!rows.length){ wrap.innerHTML = `<div class="note">No standard transactions in the recent window — only coinbase issuance right now. They appear here as accounts transact.</div>`; return; }
    wrap.innerHTML = `<div class="dim" style="margin-bottom:8px">${num(rows.length)} most-recent non-coinbase transactions.</div>
      <table class="tbl"><thead><tr><th>Transaction id</th><th>Type</th><th>To <span class="dim">(largest output)</span></th>
        <th class="num">Amount</th><th>In block</th><th class="right">Age</th></tr></thead><tbody>${
      rows.map(t=>`<tr><td>${linkTx(t.id)}</td><td>${kindPill(t.kind)}</td>
        <td>${t.addr?linkAddrShort(t.addr):'<span class="muted">non-standard</span>'}</td>
        <td class="num coin">${coin(t.val)} ${SYMBOL}</td>
        <td>${linkBlock(t.block)}</td>
        <td class="right dim" title="${esc(dt(t.ts))}">${t.ts?ago(t.ts):"—"}</td></tr>`).join("")
    }</tbody></table>`;
  } catch {} finally { txPageBusy=false; }
}

/* ------------------------------- MINERS ------------------------------- */
let minersBusy=false, minersPending=false;
async function renderMiners(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer=null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Miners</div>
    <h1 class="page">Miners <span class="dim" style="font-size:14px">(recent coinbase payout ranking)</span></h1>
    <div class="cards" id="minerCards"><div class="loading">Loading miner stats…</div></div>
    <h2 class="sec">Ranking</h2>
    <div id="minerWrap"><div class="spin">Scanning recent blocks…</div></div>`;
  await refreshMiners();
  armPoll(()=>curSeg()==="miners", refreshMiners, 20000);
  onBlockAdded(() => refreshMiners());
}
// Coalescing guard: the miner scan is heavy (deep walk) and the poll is slow (15s), so if a call
// arrives while a scan is in flight, don't DROP it (that would leave "Loading…" until the next
// 15s tick when returning to the page mid-scan) — flag a trailing re-run that renders to the
// current DOM once the in-flight scan finishes.
async function refreshMiners(){
  if (minersBusy){ minersPending=true; return; }
  minersBusy=true;
  try { do { minersPending=false; try { await minersScanRender(); } catch {} } while (minersPending && curSeg()==="miners"); }
  finally { minersBusy=false; }
}
async function minersScanRender(){
  {
    const { blocks, dag } = await collectRecentBlocks({ walkBack:70, maxBlocks:150, includeTx:true });
    const cardsEl=document.getElementById("minerCards"), wrap=document.getElementById("minerWrap");
    if (!cardsEl || !wrap) return;
    // **No hashrate column on a PALW lane.** Blocks here are won by running a pinned model, so
    // "share × network hashrate" was share of a quantity nothing on this network produces. The
    // honest per-miner figure is the share itself, and the honest network figure is the cadence.
    const palwLane = isPalwLane(overlayStats.powAlgoId);
    let netHsEst = null;
    if (!palwLane) {
      try {
        const e = await rpc("estimateNetworkHashesPerSecond", { windowSize: 1000 });
        const v = Number(e && e.networkHashesPerSecond);
        if (Number.isFinite(v)) netHsEst = v;
      } catch {}
    }
    const tally=new Map(); let minTs=Infinity,maxTs=0,totalCb=0;
    for (const b of blocks){ const bts=Number((b.header&&b.header.timestamp)||0); if (bts){ if(bts<minTs)minTs=bts; if(bts>maxTs)maxTs=bts; }
      for (const t of (b.transactions||[])){ if (!isCoinbaseTx(t)) continue;
        const lo=largestOut(t); if (!lo.addr) continue; totalCb++;
        const e=tally.get(lo.addr)||{addr:lo.addr,blocks:0,reward:0}; e.blocks++;
        e.reward += lo.value; tally.set(lo.addr,e); } }   // miner's share = largest output, NOT the full coinbase (validator/reserve/bounty are separate outputs)
    const list=[...tally.values()].sort((a,b)=>b.blocks-a.blocks);
    const netHs = netHsEst != null ? netHsEst : (dag ? Number(dag.difficulty||0)*2 : 0);
    const spanSec = (maxTs>minTs) ? (maxTs-minTs)/1000 : 0;
    cardsEl.innerHTML = [
      ["Miners seen", num(list.length), "distinct payout addrs"],
      ["Blocks sampled", num(totalCb), spanSec?("over "+fmtDur(spanSec)):"recent window"],
      palwLane
        ? ["Consensus", "PALW", powLaneLabel(overlayStats.powAlgoId).text]
        : ["Network hashrate", fmtHashrate(netHs), netHsEst!=null ? "node estimate" : "≈ difficulty ×2 (fallback)"],
      ["Avg DAG block time", spanSec&&totalCb? (spanSec/totalCb).toLocaleString("en-US",{maximumFractionDigits:2})+" s":"—", "wall-clock / DAG block"],
    ].map(c=>`<div class="card"><div class="k">${c[0]}</div><div class="v sm">${c[1]}</div><div class="sub">${c[2]}</div></div>`).join("");
    if (!list.length){ wrap.innerHTML = `<div class="note">No coinbase payouts found in the sampled window.</div>`; return; }
    wrap.innerHTML = `<table class="tbl"><thead><tr><th class="num">#</th><th>Miner (payout address)</th>
        <th class="num">Blocks</th><th>Share</th>${palwLane?"":'<th class="num">Est. hashrate</th>'}<th class="num">Reward (window)</th></tr></thead><tbody>${
      list.map((m,i)=>{ const share=totalCb?m.blocks/totalCb:0;
        return `<tr><td class="num">${i+1}</td>
          <td>${linkAddrShort(m.addr)}</td>
          <td class="num">${num(m.blocks)}</td>
          <td><div class="sharebar"><i style="width:${(share*100).toFixed(1)}%"></i></div><span class="dim" style="font-size:11px">${(share*100).toFixed(1)}%</span></td>
          ${palwLane?"":`<td class="num">${fmtHashrate(share*netHs)}</td>`}
          <td class="num coin">${coin(m.reward)} ${SYMBOL}</td></tr>`; }).join("")
    }</tbody></table>
    <div class="note" style="margin-top:8px">Ranking by coinbase blocks won in the recent on-chain window (node-direct). ${palwLane
      ? "Blocks on this lane are won by running a pinned model, not by hashing, so there is no hashrate to estimate \u2014 share of blocks is the whole measure."
      : "Estimated hashrate = share \u00d7 network hashrate (node-estimated)."} Pool labels are not applied (raw payout addresses).</div>`;
  }
}

/* -------------------------------- PEERS -------------------------------- */
// The one question this page keeps being asked is "my node is not in the list" — from operators
// who searched for `their.ip:26311` and found nothing. It can never appear that way: a row is a
// live TCP connection, and the number after the IP is the SOURCE port the peer's OS picked for
// that connection, not the port the peer listens on. An operator who dialled out from 26311
// shows up as `their.ip:54180`. So: the IP is the identity here (one row per IP), the source
// ports are demoted to a connection detail, and a lookup box answers the question outright for
// a pasted address — including when the paste carries the `:26311` that caused the confusion.
const PEER_VANTAGES = [
  { path: WS_PATH_SEED, name: "seed (mesh hub)" },
  { path: WS_PATH_HUB,  name: "hub" },
  { path: WS_PATH,      name: "this explorer's node" },
];

// Sort key for v4 dotted quads; anything else (v6) sorts after, by string.
function ipSortKey(ip){
  const m = /^(\d+)\.(\d+)\.(\d+)\.(\d+)$/.exec(ip);
  if (!m) return [1, ip];
  return [0, ((+m[1]<<24)>>>0) + (+m[2]<<16) + (+m[3]<<8) + (+m[4])];
}
function isLoopbackIp(ip){ return ip === "127.0.0.1" || ip === "::1" || /^127\./.test(ip); }

// The distinct REMOTE addresses behind a `getConnectedPeerInfo` answer — the closest thing to a
// node count a single node can honestly report. Loopback is dropped because a node on this same
// host is this vantage's own machine, not another participant, and it is already counted under
// its public address when it has one. Shared so the home tile and the Peers page cannot drift
// into counting different things again.
// **The nodes the handshake refuses are in no peer list** (claude-census-v1, 2026-09-10).
//
// The Nodes tile counts the peers the explorer's node is CONNECTED to. A node the handshake turns
// away never gets there, so when a build rollout cuts part of the network off (the previous
// network, 2026-09-09: the fence at DAA 2,400) the tile reads as the network shrinking and says nothing to
// the operators who were cut off. They are in that node's log, one line per attempt, with the
// reason; a timer on this host (misakascan-peer-census.py) turns the last hour of those lines into
// /peer-census.json. Refused is not the same question as connected, so it is shown next to the
// count, never added into it.
let peerCensus = null, peerCensusTs = 0;
async function refreshPeerCensus(force){
  if (!force && Date.now() - peerCensusTs < 60000) return peerCensus;
  peerCensusTs = Date.now();
  try {
    const r = await fetch("/peer-census.json", { cache: "no-store" });
    if (r.ok) peerCensus = await r.json();
  } catch {}
  return peerCensus;
}
function censusRefused(kinds){
  const list = (peerCensus && Array.isArray(peerCensus.refused)) ? peerCensus.refused : [];
  return kinds ? list.filter(r => kinds.includes(r.reason)) : list;
}
function censusReason(r){
  switch (r.reason) {
    case "outdated_build":
      return `outdated build — this chain has crossed the fence at DAA ${num(r.missing_fence)} and the node's build does not carry it`
        + (r.peer_daa != null ? `; the node itself is at DAA ${num(r.peer_daa)}, on its own arm` : "");
    case "different_schedule": return "a different fence schedule";
    case "refused_us": return "the node refused this one" + (r.peer_daa != null ? ` (it is at DAA ${num(r.peer_daa)})` : "");
    case "different_ruleset": return `another ruleset (fingerprint ${esc(r.remote_fingerprint || "?")}…)`;
    case "different_genesis": return `another chain (genesis ${esc(r.remote_genesis || "?")}…)`;
    default: return esc(r.reason || "refused");
  }
}
function censusRemedy(r){
  const rem = peerCensus && peerCensus.remedy;
  if (r.reason === "different_genesis")
    return "It is not on this chain at all: stop it, move its datadir aside (keep the keys) and resync.";
  if (rem && rem.text)
    return rem.url ? `<a href="${esc(rem.url)}" target="_blank" rel="noopener">${esc(rem.text)}</a>` : esc(rem.text);
  return "Its operator has to rebuild. Until then it keeps extending its own arm, and nothing the rest of the network does reaches it.";
}
// claude-bridge-fresh-v2: the node's own bridge gate, restated for the tile. Since main a5f1bdf7,
// dns_finality_fresh_for_bridge() is: DNS confirmed AND (sink blue score − the confirmed anchor's
// blue score) ≤ dns_bridge_max_anchor_distance_blue_score() — the anchor's healthy distance below
// the tip (testnet-12's 120 s DNS preset: lag 2 + backoff 1 + epoch 2 × (1 + 3 inclusion epochs) + (2 − 1) = 12)
// plus bridge_finality_max_staleness_daa_score (2) = 14, which is what Params::from(testnet-12) gives at
// 0e8ec984e (dns_bridge_max_anchor_distance_blue_score). The rule before it (virtual DAA − anchor DAA ≤ 2)
// could never hold: the newest confirmable anchor sits at least 3 blue (10–12 DAA) below the tip.
const BRIDGE_MAX_ANCHOR_DISTANCE_BLUE = 14;
let bridgeAnchor = { hash: null, blue: null };
async function refreshBridgeAnchorBlue(dns){
  const hash = dns && dns.lastDnsConfirmedAnchor ? String(dns.lastDnsConfirmedAnchor).toLowerCase() : null;
  if (!hash) { bridgeAnchor = { hash: null, blue: null }; return; }
  if (hash === bridgeAnchor.hash && bridgeAnchor.blue != null) return;   // one header read per anchor
  try {
    const b = await rpc("getBlock", { hash, includeTransactions: false });
    const bs = b && b.block && b.block.header ? b.block.header.blueScore : null;
    bridgeAnchor = { hash, blue: bs != null ? Number(bs) : null };
  } catch { bridgeAnchor = { hash, blue: null }; }
}
function bridgeFreshNote(dns, sbs){
  const anchorDaa = Number(dns.lastDnsConfirmedAnchorDaaScore) || 0;
  if (!dns.dnsConfirmed)
    return ` · <b>EVM bridge paused</b> — DNS not confirmed` + (anchorDaa ? ` (last anchor DAA ${num(anchorDaa)})` : "");
  const sink = sbs && sbs.blueScore != null ? Number(sbs.blueScore) : null;
  const anchor = bridgeAnchor.blue;
  if (sink == null || anchor == null) return " · EVM bridge: unknown (anchor header not read)";
  const behind = sink >= anchor ? sink - anchor : 0;
  if (behind <= BRIDGE_MAX_ANCHOR_DISTANCE_BLUE) return " · EVM bridge open";
  return ` · <b>EVM bridge paused</b> — DNS anchor ${num(behind)} blue below the tip; the gate allows ${BRIDGE_MAX_ANCHOR_DISTANCE_BLUE}`;
}
function censusTileNote(){
  const list = censusRefused();
  if (!list.length) return "";
  const labels = [
    ["outdated_build", "outdated build"],
    ["different_schedule", "different schedule"],
    ["refused_us", "peer refused"],
    ["different_ruleset", "different ruleset"],
    ["different_genesis", "different genesis"],
  ];
  const detail = labels.map(([kind, label]) => {
    const n = list.filter(r => r.reason === kind).length;
    return n ? `${num(n)} ${label}` : null;
  }).filter(Boolean);
  return ` · <b>+${num(list.length)} refused</b> (${detail.join(", ")})`;
}

function distinctPeerIps(peerInfo){
  const seen = new Set();
  for (const p of (peerInfo || [])){
    const a = p && p.address;
    const ip = String((a && (a.ip || a)) || (p && p.ip) || "");
    if (ip && !isLoopbackIp(ip)) seen.add(ip);
  }
  return [...seen];
}
// p2pId is the node identity. IP is only a display/census grouping key: one host may run
// multiple nodes, and one node may have several simultaneous sockets from different addresses.
function peerNodeKey(p){
  const id = p && (p.id || p.peerId);
  if (id) return `id:${id}`;
  const a = p && p.address;
  const ip = String((a && (a.ip || a)) || (p && p.ip) || "");
  const port = a && a.port != null ? a.port : (p && p.port != null ? p.port : "");
  return ip ? `addr:${ip}:${port}` : "";
}
function distinctPeerNodes(peerInfo){
  const seen = new Set();
  for (const p of (peerInfo || [])){
    const key = peerNodeKey(p);
    if (key) seen.add(key);
  }
  return [...seen];
}
// user_agent arrives as "/kaspad:1.1.0/kaspad:1.1.0/" — operators care about the version only.
function peerVersion(ua){ const m = /(\d+\.\d+\.\d+)/.exec(ua || ""); return m ? m[1] : null; }

async function renderPeers(){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › Peers</div>
    <h1 class="page">Connected peers</h1><div class="loading">Loading…</div>`;

  // Read every vantage the site holds rather than one: an operator asking "am I connected?"
  // wants the union, and any single node's peer list is only a lower bound on the mesh.
  const reads = await Promise.allSettled(PEER_VANTAGES.map(async v => {
    const [peers, info] = await Promise.all([
      pathRpc(v.path, "getConnectedPeerInfo"),
      pathRpc(v.path, "getInfo").catch(() => ({})),
    ]);
    return { ...v, list: peers.peerInfo || [], p2pId: info.p2pId || null,
             synced: info.isSynced == null ? null : !!info.isSynced };
  }));
  if (curSeg() !== "peers") return;                 // navigated away while loading

  const ok     = reads.filter(r => r.status === "fulfilled").map(r => r.value);
  const failed = PEER_VANTAGES.filter((v, i) => reads[i].status !== "fulfilled").map(v => v.name);
  if (!ok.length) return showErrFor(__g, "No node vantage answered — every RPC endpoint is unreachable.");

  // Two or three of these paths can be tunnelled to the SAME node. Calling that three vantages
  // would overstate how much of the mesh this list covers, so collapse them by the node's own
  // p2p id and report the honest count.
  const byNode = new Map();
  for (const v of ok){
    const key = v.p2pId || v.path;
    if (byNode.has(key)) { byNode.get(key).names.push(v.name); continue; }
    byNode.set(key, { names: [v.name], p2pId: v.p2pId, synced: v.synced, list: v.list });
  }
  const vantages = [...byNode.values()];

  // Union the connections by peer identity: one peer seen on several sockets or vantages is one node.
  const observedNodeIds = new Set();
  for (const vt of vantages){
    if (vt.p2pId) observedNodeIds.add(`id:${vt.p2pId}`);
  }
  const conns = new Map();
  for (const vt of vantages){
    for (const p of vt.list){
      const a = p.address || {};
      if (a.ip == null) continue;
      const id = peerNodeKey(p) || `addr:${a.ip}:${a.port}`;
      observedNodeIds.add(id);
      if (!conns.has(id)) conns.set(id, p);
    }
  }

  // One row per IP. Multiple connections from one IP are normal (co-located nodes behind one
  // address, or a reconnect whose old socket has not timed out yet), so they fold into the row.
  const ipMap = new Map();
  for (const p of conns.values()){
    const a = p.address || {}, ip = a.ip;
    let g = ipMap.get(ip);
    if (!g) ipMap.set(ip, g = { ip, ports:[], versions:new Set(), protos:new Set(),
                                pings:[], out:0, in:0, ibd:false, upMs:0 });
    g.ports.push(a.port);
    const ver = peerVersion(p.user_agent != null ? p.user_agent : p.userAgent);
    if (ver) g.versions.add(ver);
    const proto = p.advertised_protocol_version != null ? p.advertised_protocol_version : p.advertisedProtocolVersion;
    if (proto != null) g.protos.add(proto);
    const ping = p.last_ping_duration != null ? p.last_ping_duration : p.lastPingDuration;
    if (ping != null) g.pings.push(Number(ping));
    if (p.is_outbound != null ? p.is_outbound : p.isOutbound) g.out++; else g.in++;
    if (p.is_ibd_peer != null ? p.is_ibd_peer : p.isIbdPeer) g.ibd = true;
    // `time_connected` is the elapsed lifetime of the connection in ms, not a wall-clock stamp.
    const up = Number(p.time_connected != null ? p.time_connected : p.timeConnected) || 0;
    if (up > g.upMs) g.upMs = up;
  }
  const groups = [...ipMap.values()].sort((x, y) => {
    const lx = isLoopbackIp(x.ip) ? 1 : 0, ly = isLoopbackIp(y.ip) ? 1 : 0;
    if (lx !== ly) return lx - ly;                                  // loopback last
    const kx = ipSortKey(x.ip), ky = ipSortKey(y.ip);
    return kx[0] - ky[0] || (kx[0] ? String(kx[1]).localeCompare(String(ky[1])) : kx[1] - ky[1]);
  });

  const publicIps = groups.filter(g => !isLoopbackIp(g.ip)).length;
  const observedNodes = observedNodeIds.size;
  const vantageNames = vantages.map(v => v.names.join(" / ")).join(", ");
  const anySynced = vantages.some(v => v.synced === true);
  const allKnown  = vantages.every(v => v.synced !== null);
  const syncPill  = !allKnown ? `<span class="pill">sync unknown</span>`
                  : anySynced ? `<span class="pill chain">synced</span>`
                              : `<span class="pill red">not synced</span>`;

  const rows = groups.map(g => {
    const loop  = isLoopbackIp(g.ip);
    const ports = g.ports.slice().sort((a,b)=>a-b);
    const dir   = g.out && g.in ? "both" : g.out ? "outbound" : "inbound";
    const ping  = g.pings.length ? Math.min(...g.pings) : null;
    return `<tr data-ip="${esc(g.ip)}">
      <td class="hash"><b>${esc(g.ip)}</b>${loop?` <span class="pill">local</span>`:""}
        <div class="dim" style="font-size:11px;margin-top:3px">source port${ports.length>1?"s":""} ${esc(ports.join(", "))}${
          loop?" · a node on this same host":""}</div></td>
      <td class="num">${num(g.ports.length)}</td>
      <td class="num">${g.versions.size ? esc([...g.versions].join(", ")) : "—"}</td>
      <td class="num">${g.protos.size ? esc([...g.protos].join(", ")) : "—"}</td>
      <td class="num">${ping == null ? "—" : num(ping)}</td>
      <td>${dir}</td>
      <td class="num">${g.upMs ? esc(fmtDur(g.upMs/1000)) : "—"}</td>
      <td>${g.ibd ? '<span class="pill blue">syncing from</span>' : '<span class="dim">—</span>'}</td></tr>`;
  }).join("");

  viewFor(__g).innerHTML = `
    <div class="crumbs"><a href="#/">Home</a> › Peers</div>
    <h1 class="page">Connected peers — ${num(observedNodes)} distinct node${observedNodes===1?"":"s"}<span class="dim" style="font-size:14px">${
      groups.length > publicIps ? ` · ${num(groups.length - publicIps)} on this host` : ""
    } · ${num(publicIps)} remote address${publicIps===1?"":"es"} · ${num(conns.size)} live connection${conns.size===1?"":"s"}</span></h1>
    <div class="note" style="margin-top:-4px">The node count uses each peer's <b>p2pId</b> and includes
      the queried vantage once. <b>Remote addresses</b> are only an IP view: multiple nodes may share
      one host, while one node may hold several sockets. <b>Live connections</b> counts those sockets.
      The count is a lower bound because it includes only nodes observed by the available vantages.</div>

    <div class="note"><b>Looking for your own node? Search by IP — ignore the port.</b>
      Every row here is a live TCP connection, and the port shown under an address is the
      <i>source</i> port the peer's operating system picked for that connection (e.g.
      <span class="mono">54180</span>). It is <b>not</b> the port your node listens on
      (<span class="mono">26311</span>). A node that dialled out to the network will therefore
      <b>never</b> appear as <span class="mono">your.ip:26311</span> — only as your IP with some
      high-numbered port. If your IP is in this table, your node is connected.</div>

    <form class="mtp-lookup" id="peerLookup" autocomplete="off">
      <input id="peerIp" type="text" spellcheck="false"
             placeholder="Is my node connected? — paste your node's IP, e.g. 164.68.119.212" />
      <button type="submit">Check</button>
    </form>
    <div id="peerVerdict"></div>

    ${groups.length ? `<div class="tblscroll"><table class="tbl">
      <thead><tr><th>IP address</th><th class="num">Conns</th><th class="num">Version</th>
        <th class="num">Protocol</th><th class="num">Ping (ms)</th><th>Direction</th>
        <th class="num">Connected for</th><th>IBD source</th></tr></thead>
      <tbody id="peerRows">${rows}</tbody></table></div>` : `<div class="note">No peers connected.</div>`}

    <div class="note">This is the peer view of <b>${esc(vantageNames)}</b> ${syncPill} —
      ${vantages.length === 1
        ? `a single node, so the list is a <b>lower bound</b>: it holds only the peers connected to that one node, and the node itself is not in its own list.`
        : `${num(vantages.length)} distinct nodes, unioned by peer id.`}${
      failed.length ? ` <span class="dim">(${esc(failed.join(", "))} did not answer.)</span>` : ""}
      <b>Direction</b> is from that node's side: <i>inbound</i> means the peer dialled in — the
      normal case, since most operators are behind NAT and dial out to the seed.
      <b>IBD source</b> marks the single peer it is downloading history from; a node syncs from
      <i>one</i> peer at a time, so at most one row is ever marked and the dashes are normal.
      <b>Ping</b> is the last round-trip in milliseconds — tens of seconds means that connection
      has stalled.${anySynced ? "" : ` <b>The vantage's own sync state does not describe the chain</b>; the chain's state is on the <a href="#/llm">LLM Jobs</a> page.`}</div>`;

  // ---- refused at handshake (claude-census-v1) ----------------------------------------------
  await refreshPeerCensus(true);
  if (curSeg() !== "peers") return;
  const connectedIps = new Set(groups.map(g => g.ip));
  const refused = censusRefused().filter(r => !connectedIps.has(r.ip));
  const refusedByIp = new Map(refused.map(r => [r.ip, r]));
  if (peerCensus) {
    const sec = document.createElement("div");
    sec.id = "refusedPeers";
    const win = num(peerCensus.window_minutes || 60);
    sec.innerHTML = `<h2 class="sec" style="margin-top:22px">Refused at handshake — ${num(refused.length)} address${refused.length===1?"":"es"} in the last ${win} min</h2>
      <div class="note">These nodes reached ${esc(peerCensus.vantage || "the explorer's node")} and were turned away before
        they could join, so they are <b>not</b> in the count above. Read off that node's own log
        (updated ${esc(peerCensus.generated_at || "?")}); each attempt is one line with the reason.</div>
      ${refused.length ? `<div class="tblscroll"><table class="tbl"><thead><tr><th>IP address</th><th>Why it was refused</th>
        <th class="num">Attempts</th><th>Last attempt (UTC)</th></tr></thead><tbody id="refusedRows">${
        refused.map(r => `<tr data-ip="${esc(r.ip)}"><td class="hash"><b>${esc(r.ip)}</b></td><td>${censusReason(r)}</td>
          <td class="num">${num(r.attempts)}</td><td class="nowrap">${esc(String(r.last_seen || "").replace("T", " ").replace("Z", ""))}</td></tr>`).join("")
      }</tbody></table></div>
      <div class="note">${censusRemedy(refused.find(r => r.reason === "outdated_build") || refused[0])}</div>`
      : `<div class="note">Nobody was refused in that window.</div>`}`;
    viewFor(__g).appendChild(sec);
  }

  // ---- "is my node connected?" ------------------------------------------------------------
  const byIp = new Map(groups.map(g => [g.ip, g]));

  function peerCheck(scroll){
    const out = document.getElementById("peerVerdict");
    const raw = (document.getElementById("peerIp").value || "").trim();
    for (const tr of document.querySelectorAll("#peerRows tr")) {
      tr.style.background = ""; tr.style.boxShadow = "";
    }
    if (!raw) { out.innerHTML = ""; return; }
    // Operators paste what their config says, which is `ip:26311`. Accept it, drop the port —
    // and say that the port was dropped, because that is the misunderstanding being fixed.
    const m = /^(\d{1,3}(?:\.\d{1,3}){3})(?::(\d+))?$/.exec(raw);
    const ip = m ? m[1] : raw;
    const typedPort = m && m[2] ? m[2] : null;
    const portNote = typedPort
      ? `<div style="margin-top:6px">You typed <span class="mono">:${esc(typedPort)}</span> — matched on the IP alone, on purpose. The ports in the table are per-connection source ports, never a listening port.</div>`
      : "";
    const g = byIp.get(ip);
    if (g) {
      const ports = g.ports.slice().sort((a,b)=>a-b).join(", ");
      out.innerHTML = `<div class="note" style="border-left-color:var(--good)">
        <span class="pill chain">connected</span> <b class="mono">${esc(ip)}</b> is a peer of this vantage right now.
        ${num(g.ports.length)} connection${g.ports.length===1?"":"s"} (source port${g.ports.length===1?"":"s"}
        <span class="mono">${esc(ports)}</span>)${g.versions.size?`, kaspad <span class="mono">${esc([...g.versions].join(", "))}</span>`:""}${
        g.upMs?`, up ${esc(fmtDur(g.upMs/1000))}`:""}. The handshake succeeded, which means your
        genesis and params fingerprint match this network — a node on a different chain is
        dropped before it ever reaches this list.${portNote}</div>`;
      const tr = document.querySelector(`#peerRows tr[data-ip="${CSS.escape(ip)}"]`);
      if (tr) {
        tr.style.background = "#a855f71f";
        tr.style.boxShadow  = "inset 3px 0 0 var(--acc)";
        if (scroll) tr.scrollIntoView({ behavior:"smooth", block:"center" });
      }
    } else if (refusedByIp.has(ip)) {
      const r = refusedByIp.get(ip);
      out.innerHTML = `<div class="note" style="border-left-color:var(--bad)">
        <span class="pill red">refused at handshake</span> <b class="mono">${esc(ip)}</b> reached
        ${esc(peerCensus.vantage || "the explorer's node")} ${num(r.attempts)} time${r.attempts===1?"":"s"} in the last
        ${num(peerCensus.window_minutes || 60)} min and was turned away: ${censusReason(r)}.
        ${censusRemedy(r)}${portNote}</div>`;
      const tr = document.querySelector(`#refusedRows tr[data-ip="${CSS.escape(ip)}"]`);
      if (tr) {
        tr.style.background = "#ef44441f";
        tr.style.boxShadow  = "inset 3px 0 0 var(--bad)";
        if (scroll) tr.scrollIntoView({ behavior:"smooth", block:"center" });
      }
    } else {
      out.innerHTML = `<div class="note" style="border-left-color:var(--bad)">
        <span class="pill red">not in this list</span> <b class="mono">${esc(ip)}</b> is not a peer of
        ${vantages.length === 1 ? "this vantage" : "any vantage the explorer holds"} at this moment.
        That is not proof your node is down — this list is one node's view. Check, in order:
        <ol style="margin:8px 0 0 18px;padding:0">
          <li>Your node connected to a <i>different</i> peer. The mesh is not a star; try again in a minute, or check your own log for <span class="mono">P2P Connected to</span>.</li>
          <li>Your node is still starting or mid-IBD and has not completed a handshake yet.</li>
          <li>Your build's genesis / params fingerprint does not match this network — a mismatched node is disconnected at handshake and never appears here. Compare against the banner at the top of the site.</li>
          <li>You are behind NAT with no outbound reachability at all (rare — outbound almost always works).</li>
        </ol>${portNote}</div>`;
    }
  }
  const form = document.getElementById("peerLookup");
  if (form) {
    form.addEventListener("submit", e => { e.preventDefault(); peerCheck(true); });
    document.getElementById("peerIp").addEventListener("input", () => peerCheck(false));
  }
}

/* ----------------------------- MTP POINTS ------------------------------
   MISAKA Testnet Points — a read-only mirror of misaka-mtp-service (nginx
   /mtp/ → 127.0.0.1:8790). Every number on this page is copied verbatim out of
   an ML-DSA-87-signed epoch ledger; nothing is recomputed in the browser, so
   the trust anchor is `misaka mtp verify-epoch` on the same signed file — the
   page always shows how to run it. */
const MTP_BASE = "/mtp/v1";
// claude-mtp-v6 (2026-09-10): rules v6 weight C5 (LLM work) heaviest — final claims, seat receipts, model
// registrations — and the leaderboard total includes C5 (claude-mtp-v6-board).
const MTP_CATS = [
  ["c1", "C1 · Node operation",          "mined blocks, node uptime, validator attestation"],
  ["c2", "C2 · Bug reports",             "severity-tiered; first reporter takes it"],
  ["c3", "C3 · Verification / feedback", "transactions sent, campaigns, test drills"],
  ["c4", "C4 · Infrastructure",          "seeders, mirrors, tooling"],
  ["c5", "C5 · LLM work",                "LLM mining, seat verification, model registration — weighted heaviest"],
];
// Ledger values are milli-points (1 point = 1000 milli-points).
function mtpPts(m){ return (m==null) ? "—" : (Number(m)/1000).toLocaleString("en-US",{maximumFractionDigits:3}); }
// Like apiGet, but it throws: a 404 ("no such id") must not look like an outage.
async function mtpGet(path){
  const r = await fetch(path, { headers:{ "Accept":"application/json" }, cache:"no-store" });
  if (!r.ok){
    let msg = `HTTP ${r.status}`;
    try { const j = await r.json(); if (j && j.error) msg = j.error; } catch {}
    const e = new Error(msg); e.status = r.status; throw e;
  }
  return r.json();
}
function mtpLinkId(id){ return `<a class="hash" href="#/mtp/${encodeURIComponent(id)}">${esc(id)}</a>`; }
// The service routes on the RAW request path (no percent-decoding), so the `:` in
// `addr:misakatest:…` must stay literal — it is a legal path character. Everything else is escaped.
function mtpIdPath(id){ return encodeURIComponent(id).replace(/%3A/gi, ":"); }
function mtpCatLegend(){
  return `<div class="cards">${MTP_CATS.map(([k,name,sub]) =>
    `<div class="card"><div class="k">${esc(k.toUpperCase())}</div><div class="v sm">${esc(name.replace(/^C\d · /,""))}</div><div class="sub">${esc(sub)}</div></div>`).join("")}</div>
    <div class="note">Categories are fixed by the signed rules document (its <span class="hash">rules_hash</span> is pinned into every ledger).
      <b>C5 (LLM work) is weighted heaviest</b> since rules v6: 10 points for each claim you produce that becomes final (30 for a free-prompt
      answer), 2 for each Valid seat receipt on one, 1,000 for registering a model — against 1 point a mined block in C1 — and C5 is half of the allocation pool.
      C5 points are <b>measured and signed but provisional</b> — the token value they carry is not decided yet, so C5 settles nothing today.</div>`;
}
// How a participant gets an id: they already have one. Points accrue to `addr:<their address>`,
// derived from the address itself — there is no registration step, and the HTTP surface stays
// read-only because there is nothing left for it to accept.
function mtpRegisterSection(){
  return `
    <div class="note"><b>Which network is scored.</b> The published ledgers score <b>testnet-11</b>, the previous network
      (each signed ledger names its network; the leaderboard's <b>Scored network</b> card shows it). testnet-12 activity is
      <b>not scored yet</b>: whether and when MTP moves to testnet-12 has not been decided. The rules below are how the
      ledger scores.</div>
    <div class="kv">
      <div class="row"><div class="key">1 · make an address</div><div class="val">
        <span class="hash">misaka key gen --out mtp.seed</span>
        <div class="dim" style="font-size:12px;margin-top:4px">It prints your <span class="hash">misakatest:…</span> address.
          That address <b>is</b> your points id — <span class="hash">addr:misakatest:…</span>. Back the key up; it cannot be recovered.</div></div></div>
      <div class="row"><div class="key">2 · there is no step 2</div><div class="val">
        No invitation, no signature, no handle, nothing to submit. Use the address on the scored network and the next
        epoch run credits it.
        <div class="dim" style="font-size:12px;margin-top:4px">Every point still cites the block hash or transaction id it came from, so anyone can re-check it against the chain.</div></div></div>
      <div class="row"><div class="key">mine (C1)</div><div class="val">
        Mine on the scored network with your address as the payout address. Each accepted block whose coinbase pays you
        is one point, up to <b>200 points per epoch</b>.
        <div class="dim" style="font-size:12px;margin-top:4px">A block that ends up red pays no coinbase, so it earns no point either.</div></div></div>
      <div class="row"><div class="key">LLM mining (C5)</div><div class="val">
        Run a PALW producer (<span class="hash">--palw-produce</span>) with a bond whose payout address is yours. Every claim you
        produce that becomes <b>final</b> — licensed by three bonded seats and unchallenged through its window — is <b>10 points</b>; an answer to a free prompt is <b>30</b>.
        <div class="dim" style="font-size:12px;margin-top:4px">Counted in the epoch the claim became final. Up to 10,000 points an epoch, together with seat receipts.</div></div></div>
      <div class="row"><div class="key">LLM verification (C5)</div><div class="val">
        Hold a class artifact on a bonded panel seat (on testnet-12 builds the seat duties are always on for a bonded node; no flag).
        Every <b>Valid</b> receipt you sign on a claim that becomes final is <b>2 points</b>.</div></div>
      <div class="row"><div class="key">register a model (C5)</div><div class="val">
        Register a new model class through your running node (<span class="hash">misaka model add</span> — never a second
        <span class="hash">kaspad</span> with the same bond: on testnet-12 two processes on one bond can get it slashed):
        <b>1,000 points</b> to the registering bond's payout address, once per class.
        <div class="dim" style="font-size:12px;margin-top:4px">Only a registration the chain accepted counts; genesis classes, and a dormant class registered again, do not.</div></div></div>
      <div class="row"><div class="key">transact (C3)</div><div class="val">
        1 point per <b>100</b> accepted transactions your address funds, up to <b>100 points per epoch</b>.</div></div>
      <div class="row"><div class="key">run a node (C1)</div><div class="val">
        Restart your node with <span class="hash">--uacomment=mtp:&lt;your address&gt;</span> and the operator's vantage hosts attribute its uptime to you.
        <div class="dim" style="font-size:12px;margin-top:4px">A node still in IBD does not count — the sample has to see you at chain sync.</div></div></div>
      <div class="row"><div class="key">validate (C1)</div><div class="val">
        Bond and attest. Attestation resolves through the bond's owner address to the same id.</div></div>
      <div class="row"><div class="key">C2 / C4</div><div class="val">
        Bug reports and infrastructure work are awarded by hand after review — those need a human call, so they are not automatic.</div></div>
    </div>
    <div class="note"><b>An address is not a person.</b> Two addresses are two participants here, even if one human holds both, and nothing links an
      address to any account elsewhere. That is the deliberate trade for having no enrolment: the per-epoch caps above, and the allocation rules, are
      where sybil resistance lives — not in a registration check that used to silently drop the points of anyone who earned on chain without signing up first.</div>`;
}
// The operator key + the exact commands that re-derive this page from scratch.
function mtpVerifySection(op, epoch){
  const n = (epoch == null) ? 1 : epoch;
  const origin = location.origin;
  const key = op && op.operator_pubkey_mldsa87_hex;
  const pins = (op && op.pins) || [];
  const cmd = [
    `# 1. the signed ledger and the facts it was computed from`,
    `curl -s ${origin}${MTP_BASE}/epoch/${n}       -o epoch-${n}.jsonl`,
    `curl -s ${origin}${MTP_BASE}/epoch/${n}/facts -o epoch-${n}-facts.json`,
    ``,
    `# 2. the operator key (pin it out of band — the pins are shown above)`,
    `curl -s ${origin}${MTP_BASE}/operator | jq -r .operator_pubkey_mldsa87_hex > operator.pub`,
    ``,
    `# 3. check the signature AND recompute every score byte-for-byte`,
    `misaka mtp verify-epoch epoch-${n}.jsonl --pubkey-file operator.pub --facts epoch-${n}-facts.json`,
  ].join("\n");
  return `
    <div class="kv">
      <div class="row"><div class="key">Operator key</div><div class="val">${
        key ? `<span class="hash">${esc(short(key, 32))}</span> <span class="dim">· ML-DSA-87 · ${num(op.pubkey_len_bytes)} bytes</span>
               <details style="margin-top:6px"><summary class="dim" style="cursor:pointer">show full hex</summary><div class="hash" style="font-size:11px;margin-top:6px">${esc(key)}</div></details>`
             : `<span class="dim">unavailable</span>`}</div></div>
      <div class="row"><div class="key">Key pins</div><div class="val">${
        pins.length ? pins.map(p=>`<div class="hash" style="font-size:12px">${esc(p)}</div>`).join("") : `<span class="dim">—</span>`}</div></div>
      <div class="row"><div class="key">API</div><div class="val">
        <a href="${MTP_BASE}/points" target="_blank" rel="noopener">/mtp/v1/points</a> ·
        <a href="${MTP_BASE}/operator" target="_blank" rel="noopener">/mtp/v1/operator</a> ·
        <span class="dim">/mtp/v1/points/&lt;id&gt; · /mtp/v1/epoch/&lt;n&gt;[/facts|/all]</span></div></div>
    </div>
    <pre class="cmd">${esc(cmd)}</pre>
    <div class="note"><span class="hash">misaka mtp</span> is not part of the testnet-12 release build (<span class="hash">0e8ec984e</span>);
      its query client speaks plain HTTP/1.1 with no TLS, so
      <span class="hash">misaka mtp points</span> / <span class="hash">leaderboard</span> only work against an <span class="hash">http://</span> instance —
      read this endpoint with <span class="hash">curl</span> as above. That costs nothing in trust: <span class="hash">verify-epoch</span> is offline,
      and it is the step that actually proves the numbers.</div>`;
}

async function renderMtp(id){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  if (id) return renderMtpId(id);
  viewFor(__g).innerHTML = `<div class="crumbs"><a href="#/">Home</a> › MTP Points</div>
    <h1 class="page">MISAKA Testnet Points <span class="dim" style="font-size:14px">(MTP · signed epoch ledger)</span></h1>
    <div class="note">Contributions are scored once per epoch and published as an ML-DSA-87-signed ledger.
      This page mirrors that ledger verbatim — every row below can be re-verified offline with the commands at the bottom.
      Testnet points are not a token and carry no monetary value.</div>
    <div class="cards" id="mtpCards"><div class="loading" style="grid-column:1/-1">Loading points ledger…</div></div>
    <form class="mtp-lookup" id="mtpLookup" autocomplete="off">
      <input id="mtpIdInput" type="text" placeholder="Check an address — e.g. misakatest:q…" />
      <button type="submit">Check points</button>
    </form>
    <div class="sec-row"><h2 class="sec">Leaderboard</h2><a class="sec-more" href="${MTP_BASE}/points" target="_blank" rel="noopener">raw JSON →</a></div>
    <div id="mtpBoard"><div class="spin">Loading leaderboard…</div></div>
    <h2 class="sec">Categories</h2>
    <div id="mtpCatsWrap">${mtpCatLegend()}</div>
    <h2 class="sec">Start earning</h2>
    <div id="mtpRegister">${mtpRegisterSection()}</div>
    <h2 class="sec">Verify it yourself</h2>
    <div id="mtpVerify"><div class="spin">Loading operator key…</div></div>`;

  const form = document.getElementById("mtpLookup");
  if (form) form.addEventListener("submit", (e) => {
    e.preventDefault();
    let q = document.getElementById("mtpIdInput").value.trim();
    if (!q) return;
    // A pasted address is the common case: `misakatest:q…` is the same participant as
    // `addr:misakatest:q…`, so accept it and normalise to the ledger id form.
    if (/^misaka(dev|test|sim)?:/i.test(q)) q = "addr:" + q;
    location.hash = "#/mtp/" + encodeURIComponent(q);
  });

  const [boardRes, opRes] = await Promise.allSettled([ mtpGet(`${MTP_BASE}/points`), mtpGet(`${MTP_BASE}/operator`) ]);
  if (curSeg() !== "mtp" && curSeg() !== "points") return;   // navigated away while loading

  const cards = document.getElementById("mtpCards"), board = document.getElementById("mtpBoard");
  if (boardRes.status !== "fulfilled"){
    if (cards) cards.innerHTML = "";
    if (board) board.innerHTML = `<div class="err">Points service unreachable — ${esc(boardRes.reason.message)}</div>`;
  } else {
    const b = boardRes.value, entries = b.entries || [];
    const issued = entries.reduce((s,e)=> s + Number((e.cumulative&&e.cumulative.total)||0), 0);
    if (cards) cards.innerHTML = [
      ["Scored network", esc(b.network || "—"), "from the signed ledgers"],
      ["Participants",   num(b.participants),   "ids with at least one scored epoch"],
      ["Epochs counted", num(b.epochs_counted), "latest issue of each"],
      ["Latest epoch",   b.latest_epoch != null ? num(b.latest_epoch) : "—", "most recent published ledger"],
      ["Points issued",  mtpPts(issued),        "C1–C5 across all ids"],
    ].map(c=>`<div class="card"><div class="k">${c[0]}</div><div class="v sm">${c[1]}</div><div class="sub">${c[2]}</div></div>`).join("");
    if (board) board.innerHTML = !entries.length
      ? `<div class="note">No epoch has been published yet — the board fills in as soon as the first signed ledger lands.</div>`
      : `<table class="tbl"><thead><tr><th class="num">#</th><th>Ledger id</th>
           <th class="num">C1</th><th class="num">C2</th><th class="num">C3</th><th class="num">C4</th><th class="num">C5</th>
           <th class="num">Total</th></tr></thead><tbody>${
          entries.map(e => { const c = e.cumulative || {};
            return `<tr><td class="num">${num(e.rank)}</td><td>${mtpLinkId(e.id)}</td>
              <td class="num">${mtpPts(c.c1)}</td><td class="num">${mtpPts(c.c2)}</td>
              <td class="num">${mtpPts(c.c3)}</td><td class="num">${mtpPts(c.c4)}</td><td class="num">${mtpPts(c.c5)}</td>
              <td class="num coin">${mtpPts(c.total)}</td></tr>`; }).join("")
        }</tbody></table>
        <div class="note">The board sums all five categories — C5 (LLM work) included, and weighted heaviest since rules v6.
          Ranking is by total, ties broken by id, and a reissued epoch replaces its earlier issue instead of adding to it.</div>`;
  }
  const ver = document.getElementById("mtpVerify");
  if (ver) ver.innerHTML = mtpVerifySection(
    opRes.status === "fulfilled" ? opRes.value : null,
    boardRes.status === "fulfilled" ? boardRes.value.latest_epoch : null);
}

async function renderMtpId(id){
  const __g = routeGen;   // claude-route-guard-v1: the generation this render belongs to
  const crumbs = `<div class="crumbs"><a href="#/">Home</a> › <a href="#/mtp">MTP Points</a> › ${esc(id)}</div>`;
  viewFor(__g).innerHTML = `${crumbs}<h1 class="page">${esc(id)}</h1><div class="loading">Looking up points…</div>`;
  let v, op = null;
  try { v = await mtpGet(`${MTP_BASE}/points/${mtpIdPath(id)}`); }
  catch(e){
    return viewFor(__g).innerHTML = `${crumbs}<h1 class="page">${esc(id)}</h1>` + (e.status === 404
      ? `<div class="note">No published epoch scores this id yet. Points appear here once the epoch covering the
           activity is published — there is nothing to sign up for, so if the address has mined, transacted or served on
           the scored network, it will appear at the next epoch run. testnet-12 activity is not scored yet.</div>
         <h2 class="sec">Start earning</h2>${mtpRegisterSection()}
         <div style="margin-top:14px"><a class="sec-more" href="#/mtp">← back to the leaderboard</a></div>`
      : `<div class="err">Points service unreachable — ${esc(e.message)}</div>`);
  }
  try { op = await mtpGet(`${MTP_BASE}/operator`); } catch {}
  if (curSeg() !== "mtp" && curSeg() !== "points") return;   // navigated away while loading

  const c = v.cumulative || {}, epochs = v.epochs || [];
  const cards = [["Total points", mtpPts(c.total), "C1–C5 cumulative"]]
    .concat(MTP_CATS.map(([k,name,]) => [name, mtpPts(c[k]), k === "c5" ? "provisional — settles no tokens" : "cumulative"]));
  viewFor(__g).innerHTML = `${crumbs}
    <h1 class="page">${esc(v.id)} <span class="dim" style="font-size:14px">· ${num(epochs.length)} scored epoch${epochs.length===1?"":"s"}${
      v.latest_epoch!=null?` · latest epoch ${num(v.latest_epoch)}`:""}</span></h1>
    <div class="cards">${cards.map(c2=>`<div class="card"><div class="k">${esc(c2[0])}</div><div class="v sm">${c2[1]}</div><div class="sub">${esc(c2[2])}</div></div>`).join("")}</div>
    <h2 class="sec">Per-epoch breakdown</h2>
    ${!epochs.length ? `<div class="note">No scored epochs.</div>` : `
    <div class="tblscroll">
    <table class="tbl"><thead><tr><th class="num">Epoch</th><th>Network</th>
      <th class="num">C1</th><th class="num">C2</th><th class="num">C3</th><th class="num">C4</th><th class="num">C5</th>
      <th class="num">Total</th><th>Signed ledger</th></tr></thead><tbody>${
      epochs.slice().sort((a,b)=>b.epoch-a.epoch).map(e => {
        const tot = Number(e.c1||0)+Number(e.c2||0)+Number(e.c3||0)+Number(e.c4||0)+Number(e.c5||0);
        return `<tr><td class="num">${num(e.epoch)}${e.superseded?` <span class="pill blue" title="an earlier issue of this epoch was superseded">issue ${num(e.issue)}</span>`:""}</td>
          <td>${esc(e.network||"—")}</td>
          <td class="num">${mtpPts(e.c1)}</td><td class="num">${mtpPts(e.c2)}</td><td class="num">${mtpPts(e.c3)}</td>
          <td class="num">${mtpPts(e.c4)}</td><td class="num">${mtpPts(e.c5)}</td>
          <td class="num coin">${mtpPts(tot)}</td>
          <td><a href="${MTP_BASE}/epoch/${encodeURIComponent(e.epoch)}" target="_blank" rel="noopener">${esc(e.file||("epoch-"+e.epoch))}</a>
              <div class="dim" style="font-size:11px"><a href="${MTP_BASE}/epoch/${encodeURIComponent(e.epoch)}/facts" target="_blank" rel="noopener">facts</a> ·
                  <a href="${MTP_BASE}/epoch/${encodeURIComponent(e.epoch)}/all" target="_blank" rel="noopener">all issues</a></div></td></tr>
        <tr><td colspan="9" style="padding-top:0">
          <div class="dim" style="font-size:11.5px">rules ${esc(short(e.rules_hash,16))} · inputs ${esc(short(e.inputs_hash,16))}</div>
          ${mtpEvidence(e.evidence)}</td></tr>`;
      }).join("")}</tbody></table></div>`}
    <h2 class="sec">Verify it yourself</h2>
    ${mtpVerifySection(op, v.latest_epoch)}`;
}
// Evidence links are the ledger's own audit trail: uptime samples, attestation
// tx ids, accepted PALW leaves. Long lists are capped — and the cap is stated.
function mtpEvidence(ev){
  const list = ev || [];
  if (!list.length) return `<div class="dim" style="font-size:11.5px">no evidence links</div>`;
  const CAP = 200, shown = list.slice(0, CAP);
  return `<details style="margin-top:4px"><summary class="dim" style="cursor:pointer;font-size:11.5px">${num(list.length)} evidence link${list.length===1?"":"s"}</summary>
    <div style="margin-top:6px">${shown.map(x=>`<div class="hash" style="font-size:11px;color:var(--mut)">${esc(x)}</div>`).join("")}
    ${list.length>CAP?`<div class="dim" style="font-size:11px;margin-top:6px">showing the first ${num(CAP)} of ${num(list.length)} — the full list is in the signed ledger.</div>`:""}</div></details>`;
}

/* -------------------------------- misc --------------------------------- */
function showErr(msg, ctx){
  view().innerHTML = `<div class="crumbs"><a href="#/">Home</a></div>
    <div class="err">${msg}${ctx?`<div class="hash" style="margin-top:8px;font-size:12px">${esc(ctx)}</div>`:""}</div>`;
}

/* ------------------- auto-update for long-open tabs -------------------
   Deploys overwrite /app.js, so nginx's ETag (derived from mtime+size)
   changes on every deploy. Snapshot it at load, re-check periodically and
   on tab focus; if it changed, a new version is live → reload. With the
   no-store cache policy this makes even tabs/PWAs left open self-update. */
const VERSION_CHECK_MS = 60000;
let appTag = null;       // baseline ETag/Last-Modified of /app.js for this session
let reloading = false;
async function fetchAppTag(){
  const r = await fetch("/app.js", { method: "HEAD", cache: "no-store" });
  return r.headers.get("etag") || r.headers.get("last-modified");
}
async function checkVersion(){
  if (reloading) return;
  let tag;
  try { tag = await fetchAppTag(); } catch { return; }   // offline/transient — retry later
  if (!tag) return;
  if (appTag === null) { appTag = tag; return; }          // first sample = baseline
  if (tag === appTag) return;
  reloading = true;
  const b = document.createElement("div");
  b.textContent = "新しいバージョンを検出 — 更新しています…";
  b.style.cssText = "position:fixed;left:50%;bottom:18px;transform:translateX(-50%);z-index:9999;"
    + "background:#16122a;color:#e7e3f7;border:1px solid #a855f7;padding:10px 16px;border-radius:10px;"
    + "font-size:14px;box-shadow:0 6px 20px rgba(0,0,0,.5)";
  document.body.appendChild(b);
  setTimeout(() => location.reload(), 1500);
}
checkVersion();                                            // snapshot baseline at load
setInterval(checkVersion, VERSION_CHECK_MS);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") checkVersion();
});

/* live UI: briefly flash freshly-arrived block rows so new blocks are visible at a glance */
(function injectLiveStyles(){
  if (document.getElementById("msk-live-style")) return;
  const s = document.createElement("style");
  s.id = "msk-live-style";
  s.textContent =
    "@keyframes mskFlash{0%{background:rgba(168,85,247,.30)}100%{background:transparent}}"
    + "tr.rowNew>td{animation:mskFlash 1.2s ease-out}"
    + "@media(prefers-reduced-motion:reduce){tr.rowNew>td{animation:none}}"
    // kaspa-pq overlay: tx-kind pills + overlay cards + top nav
    + ".pill.bond{background:#1e3a2f;color:#7ee0b0;border:1px solid #2f6f53}"
    + ".pill.att{background:#1c2b4a;color:#8fb6ff;border:1px solid #34548f}"
    + ".pill.slash{background:#3a1e22;color:#ff9aa6;border:1px solid #7f3540}"
    + ".pill.std{background:#23202e;color:#b9b3c9;border:1px solid #3a3550}"
    + ".pill.evm{background:#2a1e3a;color:#c79bff;border:1px solid #5b3f8f}"
    + ".pill.warn{color:var(--warn);border-color:#fbbf2444;background:#fbbf240f}"
    + ".card .v.ov{color:#c9a4ff}";
  document.head.appendChild(s);
})();

/* Persistent header nav so the overlay/peers views are reachable from anywhere. */
(function injectNav(){
  const bar = document.querySelector(".topbar"); if (!bar) return;
  if (document.getElementById("mskNav")) return;
  const nav = document.createElement("nav");
  nav.id = "mskNav"; nav.className = "topnav";
  nav.innerHTML = `<a href="#/">Home</a><a href="#/registry">Models</a><a href="#/lane">Lane</a><a href="#/llm">LLM Jobs</a><a href="#/transactions">Transactions</a><a href="#/miners">Miners</a><a href="#/mtp">MTP Points</a><a href="#/overlay">Validators</a><a href="#/finality">Finality</a><a href="#/evm">EVM</a><a href="#/faucet">Faucet</a><a href="#/peers">Peers</a>`;
  const brand = bar.querySelector(".brand");
  if (brand && brand.nextSibling) bar.insertBefore(nav, brand.nextSibling); else bar.appendChild(nav);
  const mark = () => {
    const h = location.hash.replace(/^#\/?/, "").split("/")[0];
    nav.querySelectorAll("a").forEach(a => {
      const t = a.getAttribute("href").replace(/^#\/?/, "");
      a.classList.toggle("on", t === h || (t === "" && h === "") || (t === "evm" && h === "evmtx") || (t === "llm" && (h === "blockdag" || h === "dag" || h === "palw")) || (t === "registry" && (h === "classes" || h === "line" || h === "model")) || (t === "lane" && h === "rounds"));
    });
  };
  window.addEventListener("hashchange", mark); mark();
})();

connect();
route();
