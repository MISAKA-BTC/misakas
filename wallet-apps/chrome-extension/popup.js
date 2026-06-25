// MISAKA Wallet — popup (UI + WASM signing). The post-quantum signing core is the
// kaspa-wasm SDK; key material exists only in this popup's volatile memory while
// unlocked. The encrypted vault lives in chrome.storage.local; the unlocked seed (if
// "stay unlocked" is active) lives in chrome.storage.session (in-memory, extension-only,
// auto-cleared on browser close and by the background auto-lock alarm).

import init, {
  Mnemonic,
  KaspaPqKeyPair,
  Address,
  RpcClient,
  Encoding,
  createTransaction,
  signTransactionMlDsa87,
  calculateTransactionMass,
  kaspaToSompi,
  sompiToKaspaString,
  ScriptPublicKey,
  payToAddressScript,
} from "./kaspa/kaspa.js";

import { createVault, openVault } from "./src/vault.js";
import { NETWORKS, DEFAULT_NETWORK, netConfig } from "./src/networks.js";
import { deriveEvm, isEvmAddress, signEvmTx, withdrawCalldata } from "./vendor/evm-bundle.js";

const VAULT_KEY = "misaka_vault";
const META_KEY = "misaka_meta"; // { net, lockMinutes, rpcOverride }
const SESSION_KEY = "unlocked"; // { net, seed, exp }

// mass / standardness (ML-DSA-87 sigs are large) — mirror the reference wallet.
const FEE_BUFFER = 5_000n, FEE_CAP = 20_000_000n, DUST = 20_000n;
const TRANSIENT_FACTOR = 4n, MASS_SAFETY = 430_000n, IN_OVERHEAD = 52n, TX_OVERHEAD = 320n;
const HARD_MAX_INPUTS = 60, SIG_BYTES_FALLBACK = 7300;

// EVM lane (ADR-0020): standard Ethereum domain, bridged to the UTXO lane.
const EVM_CHAIN_ID = 5067211; // 0x4D534B "MSK"
const MISAKA_WITHDRAW = "0x000000000000000000000000000000000000F002"; // F002 EVM→UTXO exit precompile
const EVM_NATIVE_SCALE = 10_000_000_000n; // wei per sompi (1e10): 1 MSK = 1e8 sompi = 1e18 wei
const EVM_MIN_GAS_PRICE = 2_000_000_000n; // 2 gwei (≥ the 1 gwei base-fee floor)
const EVM_NATIVE_GAS = 21_000n, EVM_WITHDRAW_GAS = 100_000n;

const W = { net: DEFAULT_NETWORK, cfg: netConfig(DEFAULT_NETWORK), seed: null, kp: null, address: null, evm: null, rpc: null, connected: false, utxos: [], allUtxos: [], balance: 0n, lockedBond: 0n, bonds: new Set(), evmBalance: 0n, lockMinutes: 5, rpcOverride: "", sigBytes: 0, _flow: {} };

// kaspa-pq bond display fix: a StakeBond's locked output-0 is a NORMAL retained UTXO at the owner's
// own address (the stake-lock is enforced only by the consensus spend-gate), so it shows up in
// getUtxosByAddresses and would otherwise (a) inflate the displayed spendable balance and (b) be
// picked first by coin selection (it is the largest UTXO) → a tx that the consensus gate rejects.
// The validator CLI prints each bond's outpoint as "<txid>:0"; the user records them in Settings and
// the wallet excludes them from spendable balance + coin selection, showing them as "locked in bond".
// `opKey` derives the "<txid>:<index>" key from a UTXO entry across the SerdeJson entry shapes.
function opKey(e) {
  try {
    const op = (e && e.outpoint) || e || {};
    const tid = op.transactionId || op.transaction_id || op.txId || op.transactionHash;
    const idx = op.index ?? op.transactionIndex ?? 0;
    if (!tid) return null;
    return String(tid).toLowerCase() + ":" + String(idx);
  } catch { return null; }
}
// Normalize a user-entered bond ref ("txid:0", or bare "txid" ⇒ index 0) to the opKey form.
function normBondRef(s) {
  const t = String(s || "").trim().toLowerCase();
  if (!t) return null;
  const m = t.match(/^([0-9a-f]{16,128}):(\d+)$/) || t.match(/^([0-9a-f]{16,128})$/);
  if (!m) return null;
  return m[1] + ":" + (m[2] ?? "0");
}

const $ = (s, r = document) => r.querySelector(s);
const view = () => document.getElementById("view");
const tpl = (id) => document.getElementById(id).content.cloneNode(true);
// signTransactionMlDsa87 → passArray8ToWasm0 expects 32 RAW bytes (a hex string
// would be copied as 64 wrong bytes → "requires 32 bytes of randomness").
const randomness32 = () => crypto.getRandomValues(new Uint8Array(32));
const amtOf = (e) => { try { return BigInt(e.amount); } catch { return 0n; } };
const fmt = (sompi) => { try { return sompiToKaspaString(sompi); } catch { return (Number(sompi) / 1e8).toString(); } };

/* ----------------------------- storage -------------------------------- */
const getLocal = (k) => new Promise((r) => chrome.storage.local.get([k], (o) => r(o[k])));
const setLocal = (o) => new Promise((r) => chrome.storage.local.set(o, r));
const delLocal = (k) => new Promise((r) => chrome.storage.local.remove(k, r));
const getSession = (k) => new Promise((r) => chrome.storage.session.get([k], (o) => r(o[k])));
const setSession = (o) => new Promise((r) => chrome.storage.session.set(o, r));
const delSession = (k) => new Promise((r) => chrome.storage.session.remove(k, r));

async function loadMeta() {
  const m = (await getLocal(META_KEY)) || {};
  W.net = m.net || DEFAULT_NETWORK;
  W.cfg = netConfig(W.net);
  W.lockMinutes = m.lockMinutes || 5;
  W.rpcOverride = m.rpcOverride || "";
  // Outpoints are globally unique, so a flat list is correct (a bond ref only ever matches a UTXO at
  // its own address/network). Tolerate a missing/legacy field (no bonds recorded).
  W.bonds = new Set((Array.isArray(m.bonds) ? m.bonds : []).map(normBondRef).filter(Boolean));
}
async function saveMeta() {
  await setLocal({ [META_KEY]: { net: W.net, lockMinutes: W.lockMinutes, rpcOverride: W.rpcOverride, bonds: [...W.bonds] } });
}

/* ----------------------------- auto-lock ------------------------------ */
async function armLock() {
  const exp = Date.now() + W.lockMinutes * 60_000;
  if (W.seed) await setSession({ [SESSION_KEY]: { net: W.net, seed: W.seed, exp } });
  chrome.runtime.sendMessage({ target: "bg", cmd: "arm-lock", minutes: W.lockMinutes });
}
async function lock() {
  W.seed = W.kp = W.address = null;
  W.utxos = []; W.allUtxos = []; W.balance = 0n; W.lockedBond = 0n;
  try { if (W.rpc) await W.rpc.disconnect?.(); } catch {}
  W.rpc = null; W.connected = false;
  await delSession(SESSION_KEY);
  chrome.runtime.sendMessage({ target: "bg", cmd: "lock-now" });
  render();
}

/* ------------------------------- crypto/keys -------------------------- */
function deriveKeypair(seed) {
  const kp = KaspaPqKeyPair.fromMnemonic(seed, "", W.net, 0, 0, 0);
  const addr = kp.address(W.cfg.prefix).toString();
  return { kp, addr };
}
async function activate(seed) {
  W.seed = seed;
  const { kp, addr } = deriveKeypair(seed);
  W.kp = kp; W.address = addr; W.sigBytes = 0;
  // Derive the EVM (secp256k1) identity from the SAME 24-word phrase — standard
  // BIP44 m/44'/60'/0'/0/0, so W.evm.address equals the MetaMask 0x address.
  try { W.evm = deriveEvm(seed, 0, 0); } catch (e) { W.evm = null; }
  await armLock();
  render(); // home
  connectRpc().then(refresh);
}

/* --------------------------------- rpc -------------------------------- */
function rpcUrl() { return W.rpcOverride || W.cfg.rpc; }
async function connectRpc() {
  if (W.rpc && W.connected) return true;
  const url = rpcUrl();
  if (!url) { setConn(false); return false; }
  try {
    W.rpc = new RpcClient({ url, encoding: Encoding.SerdeJson, networkId: W.net });
    await W.rpc.connect();
    await W.rpc.getServerInfo();
    setConn(true);
    return true;
  } catch {
    setConn(false);
    return false;
  }
}
function setConn(up) { W.connected = up; const c = $("#conn"); if (c) c.classList.toggle("up", up); }

/* ------------------------------ activity ------------------------------ */
// Minimal local record of sends (last 50 per network); safe no-op on failure.
async function pushActivity(item) {
  try {
    const key = "misaka_activity_" + W.net;
    const cur = (await getLocal(key)) || [];
    cur.unshift(item);
    await setLocal({ [key]: cur.slice(0, 50) });
  } catch {}
}

/* ------------------------------- EVM RPC ------------------------------ */
// Ethereum JSON-RPC (ADR-0020 adapter) — a separate HTTP endpoint from the UTXO
// wRPC; the node enables CORS. Throws on transport / RPC error.
async function ethRpc(method, params = []) {
  const url = W.cfg.ethRpc;
  if (!url) throw new Error("EVM RPC endpoint is not configured for this network");
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 8000);
  try {
    const resp = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
      signal: ctrl.signal,
    });
    if (!resp.ok) throw new Error(`EVM RPC HTTP ${resp.status}`);
    const j = await resp.json();
    if (j && j.error) throw new Error(j.error.message || `EVM RPC error ${j.error.code}`);
    return j.result;
  } finally { clearTimeout(timer); }
}
async function evmGasPrice() {
  try { const g = BigInt(await ethRpc("eth_gasPrice")) * 2n; return g > EVM_MIN_GAS_PRICE ? g : EVM_MIN_GAS_PRICE; }
  catch { return EVM_MIN_GAS_PRICE; }
}
async function evmNonce() { return BigInt(await ethRpc("eth_getTransactionCount", [W.evm.address, "latest"])); }
// EVM balance in sompi (wei ÷ 1e10). Best-effort: 0 when the endpoint is down.
async function refreshEvm() {
  if (!W.evm || !W.cfg.ethRpc) { W.evmBalance = 0n; return; }
  try { W.evmBalance = BigInt(await ethRpc("eth_getBalance", [W.evm.address, "latest"])) / EVM_NATIVE_SCALE; }
  catch { W.evmBalance = 0n; }
  updateBalanceDisplay();
}
// Home/Send show COMBINED holdings (UTXO + EVM) as one figure — the lanes are
// 1:1 and the single-balance UI is unchanged.
function updateBalanceDisplay() {
  const total = (W.balance || 0n) + (W.evmBalance || 0n);
  const b = $("#bal"); if (b) b.textContent = fmt(total);
  const av = $("#snd-avail"); if (av) av.textContent = fmt(total) + " " + W.cfg.symbol;
  // "Locked in bond" line — only shown when the user has recorded bond outpoints that are present.
  const lk = $("#locked-row"), lkv = $("#locked-bal");
  if (lk && lkv) {
    if (W.lockedBond && W.lockedBond > 0n) { lkv.textContent = fmt(W.lockedBond) + " " + W.cfg.symbol; lk.style.display = ""; }
    else { lk.style.display = "none"; }
  }
}

let _refreshing = false;
async function refresh() {
  if (!W.address || _refreshing) return;
  _refreshing = true;
  try {
    if (!W.connected && !(await connectRpc())) return;
    const res = await W.rpc.getUtxosByAddresses([W.address]);
    const all = (res && res.entries) || [];
    // Keep recorded bond outpoints OUT of W.utxos so coin selection never tries to spend a locked
    // bond (the consensus spend-gate would reject the carrying block); show their total separately.
    const spend = [], locked = [];
    for (const e of all) { (W.bonds.size && W.bonds.has(opKey(e)) ? locked : spend).push(e); }
    W.allUtxos = all;
    W.utxos = spend;
    W.balance = spend.reduce((s, e) => s + amtOf(e), 0n);
    W.lockedBond = locked.reduce((s, e) => s + amtOf(e), 0n);
    const n = $("#utxo-n"); if (n) n.textContent = String(all.length);
    updateBalanceDisplay();
    refreshEvm(); // EVM balance (best-effort; refreshes the combined figure when it lands)
  } catch { setConn(false); } finally { _refreshing = false; }
}

/* ----------------------------- mass / send ---------------------------- */
function transientMassOf(signed) {
  let size = 94n;
  try { for (const i of signed.inputs) size += IN_OVERHEAD + BigInt(((i.signatureScript || "").length / 2) | 0); } catch {}
  try { size += BigInt(signed.outputs.length) * 100n; } catch {}
  return size * TRANSIENT_FACTOR;
}
function feeFor(signed) {
  let m = 0n; try { m = BigInt(calculateTransactionMass(W.net, signed)); } catch {}
  const t = transientMassOf(signed);
  let f = (m > t ? m : t) + FEE_BUFFER;
  return f > FEE_CAP ? FEE_CAP : f;
}
async function ensureMeasured() {
  if (W.sigBytes > 0 || !W.utxos.length) return;
  const u = [...W.utxos].sort((a, b) => (amtOf(a) < amtOf(b) ? 1 : -1))[0];
  try {
    const tx = createTransaction([u], [{ address: W.address, amount: amtOf(u) > 2000n ? amtOf(u) - 1000n : amtOf(u) }], 0n, undefined, 1);
    const s = signTransactionMlDsa87(tx, W.kp, randomness32());
    W.sigBytes = ((s.inputs[0].signatureScript || "").length / 2) | 0;
  } catch {}
}
function maxInputs() {
  const per = IN_OVERHEAD + BigInt(W.sigBytes || SIG_BYTES_FALLBACK);
  let n = Number((MASS_SAFETY / TRANSIENT_FACTOR - TX_OVERHEAD) / per);
  if (!isFinite(n) || n < 1) n = 1;
  return Math.min(n, HARD_MAX_INPUTS);
}
/* ---- UTXO → EVM bridge (ADR-0020 deposit-lock) ---- */
function u64leHex(n) {
  let x = BigInt(n), h = "";
  for (let i = 0; i < 8; i++) { h += Number(x & 0xffn).toString(16).padStart(2, "0"); x >>= 8n; }
  return h;
}
// Lock UTXO funds in an EVM_DEPOSIT_LOCK output addressed to a 0x recipient.
// Producers auto-claim the lock and credit the EVM account; if never claimed the
// sender can refund after `timeout`. Script (v0, 108 B): OpNop(0x61) OpData36(0x24)
// evm20 timeout8LE claimTip8LE OpDrop(0x75) refund69(sender P2PKH).
async function sendToEvm(toAddr, amountStr) {
  const amount = kaspaToSompi(amountStr);
  if (amount === undefined || amount <= 0n) throw new Error("Invalid amount");
  await refresh();
  if (!W.utxos.length) throw new Error("No balance");
  await ensureMeasured();
  const maxIn = maxInputs();
  const evmHex = toAddr.replace(/^0x/i, "").toLowerCase();
  let timeout = 18446744073709551615n; // u64::MAX fallback (always claimable)
  try { const dag = await W.rpc.getBlockDagInfo(); const d = BigInt(dag.virtualDaaScore || dag.virtual_daa_score || 0); if (d > 0n) timeout = d + 100_000_000n; } catch {}
  const claimTip = 0n; // full amount credited to EVM; producers auto-claim
  const sorted = [...W.utxos].sort((a, b) => (amtOf(a) < amtOf(b) ? 1 : -1));
  const picked = []; let total = 0n;
  for (const u of sorted) {
    picked.push(u); total += amtOf(u);
    if (picked.length > maxIn) throw new Error("Too many UTXOs for one bridge tx — send a smaller amount, or consolidate first (send to yourself).");
    const probe = signTransactionMlDsa87(createTransaction(picked, [{ address: W.address, amount }], 0n, undefined, 1), W.kp, randomness32());
    const fee = feeFor(probe);
    if (total >= amount + fee) {
      const change = total - amount - fee;
      const outs = [{ address: W.address, amount }];
      if (change >= DUST) outs.push({ address: W.address, amount: change });
      const tx = createTransaction(picked, outs, 0n, undefined, 1);
      const refundHex = tx.outputs[0].scriptPublicKey.script; // sender's 69-B ML-DSA P2PKH
      const lockHex = "61" + "24" + evmHex + u64leHex(timeout) + u64leHex(claimTip) + "75" + refundHex;
      const newOuts = [{ value: tx.outputs[0].value, scriptPublicKey: new ScriptPublicKey(0, lockHex) }];
      if (tx.outputs.length > 1) newOuts.push({ value: tx.outputs[1].value, scriptPublicKey: tx.outputs[1].scriptPublicKey });
      tx.outputs = newOuts;
      const signed = signTransactionMlDsa87(tx, W.kp, randomness32());
      const resp = await W.rpc.submitTransaction({ transaction: signed, allowOrphan: false });
      const txid = (resp && (resp.transactionId || resp.transaction_id)) || signed.id;
      await pushActivity({ type: "send", txid, to: toAddr, amount: amountStr, sym: W.cfg.symbol, net: W.net, addr: W.address, ts: Date.now() });
      return txid;
    }
  }
  throw new Error("Insufficient balance for amount + fee");
}
// Native UTXO → UTXO send (chained consolidation: grow the input set until it
// covers amount + the measured PQ fee).
async function sendUtxo(toAddr, amountStr) {
  const amount = kaspaToSompi(amountStr);
  if (amount === undefined || amount <= 0n) throw new Error("Invalid amount");
  await refresh();
  await ensureMeasured();
  const cap = maxInputs();
  const sorted = [...W.utxos].sort((a, b) => (amtOf(a) < amtOf(b) ? 1 : -1)); // largest first
  const picked = []; let total = 0n;
  for (const u of sorted) {
    picked.push(u); total += amtOf(u);
    if (picked.length > cap) throw new Error("Too many UTXOs for one transaction — consolidate first (send max to yourself).");
    // build a probe to get the real fee for this input count
    const probe = signTransactionMlDsa87(createTransaction(picked, [{ address: toAddr, amount }], 0n, undefined, 1), W.kp, randomness32());
    const fee = feeFor(probe);
    if (total >= amount + fee) {
      const change = total - amount - fee;
      const outputs = [{ address: toAddr, amount }];
      if (change >= DUST) outputs.push({ address: W.address, amount: change });
      const signed = signTransactionMlDsa87(createTransaction(picked, outputs, 0n, undefined, 1), W.kp, randomness32());
      const resp = await W.rpc.submitTransaction({ transaction: signed, allowOrphan: false });
      return (resp && (resp.transactionId || resp.transaction_id)) || signed.id;
    }
  }
  throw new Error("Insufficient balance for amount + fee");
}

/* ---- EVM → EVM native send (pays from the EVM lane) ---- */
async function sendEvmNative(toAddr, amountStr) {
  if (!W.evm) throw new Error("EVM account unavailable");
  const amount = kaspaToSompi(amountStr);
  if (amount === undefined || amount <= 0n) throw new Error("Invalid amount");
  const valueWei = amount * EVM_NATIVE_SCALE;
  const gasPrice = await evmGasPrice();
  await refreshEvm();
  if (W.evmBalance * EVM_NATIVE_SCALE < valueWei + EVM_NATIVE_GAS * gasPrice) throw new Error("Insufficient EVM balance for amount + gas");
  const nonce = await evmNonce();
  const raw = signEvmTx(W.evm.privKey, { to: toAddr, value: valueWei, nonce, gasLimit: EVM_NATIVE_GAS, gasPrice, chainId: EVM_CHAIN_ID, data: "0x" });
  const txid = await ethRpc("eth_sendRawTransaction", [raw]);
  await pushActivity({ type: "evm-send", txid, to: toAddr, amount: amountStr, sym: W.cfg.symbol, net: W.net, addr: W.evm.address, ts: Date.now() });
  return txid;
}

/* ---- EVM → UTXO bridge (F002 withdraw → synthetic UTXO at the misaka: dest) ---- */
async function sendWithdraw(toAddr, amountStr) {
  if (!W.evm) throw new Error("EVM account unavailable");
  const amount = kaspaToSompi(amountStr);
  if (amount === undefined || amount <= 0n) throw new Error("Invalid amount");
  const valueWei = amount * EVM_NATIVE_SCALE; // exact 1e10 multiple ⇒ exact sompi (F002 requires it)
  // Destination UTXO ScriptPublicKey (ML-DSA P2PKH, version 0 — the only class F002 accepts).
  const script = payToAddressScript(new Address(toAddr)).script;
  const calldata = withdrawCalldata(0, script);
  const gasPrice = await evmGasPrice();
  await refreshEvm();
  if (W.evmBalance * EVM_NATIVE_SCALE < valueWei + EVM_WITHDRAW_GAS * gasPrice) throw new Error("Insufficient EVM balance for amount + gas");
  const nonce = await evmNonce();
  const raw = signEvmTx(W.evm.privKey, { to: MISAKA_WITHDRAW, value: valueWei, nonce, gasLimit: EVM_WITHDRAW_GAS, gasPrice, chainId: EVM_CHAIN_ID, data: calldata });
  const txid = await ethRpc("eth_sendRawTransaction", [raw]);
  await pushActivity({ type: "withdraw", txid, to: toAddr, amount: amountStr, sym: W.cfg.symbol, net: W.net, addr: W.evm.address, ts: Date.now() });
  return txid;
}

// Router: ONE Send form, destination-driven auto-bridge (UI unchanged).
//   → 0x recipient  : pay from EVM if it covers (native), else bridge from UTXO (deposit-lock).
//   → misaka: recip : pay from UTXO if it covers (native), else bridge from EVM (F002 withdraw).
async function send(toAddr, amountStr) {
  const amount = kaspaToSompi(amountStr);
  if (amount === undefined || amount <= 0n) throw new Error("Invalid amount");
  if (isEvmAddress(toAddr)) {
    await refreshEvm();
    if (W.evm && W.evmBalance >= amount) return sendEvmNative(toAddr, amountStr); // EVM → EVM
    return sendToEvm(toAddr, amountStr);                                          // UTXO → EVM bridge
  }
  if (!Address.validate(toAddr)) throw new Error("Invalid recipient address");
  if (!toAddr.startsWith(W.cfg.prefix + ":")) throw new Error(`Address must be on ${W.cfg.label} (${W.cfg.prefix}:)`);
  await refresh();
  if (W.balance >= amount) return sendUtxo(toAddr, amountStr);                    // UTXO → UTXO
  await refreshEvm();
  if (W.evm && W.evmBalance >= amount) return sendWithdraw(toAddr, amountStr);    // EVM → UTXO bridge
  throw new Error("Insufficient balance");
}

/* ------------------------------- rendering ---------------------------- */
function mount(node) { const v = view(); v.innerHTML = ""; v.appendChild(node); }
function setSyms() {
  document.querySelectorAll(".sym2").forEach((e) => (e.textContent = W.cfg.symbol));
  const bs = $("#bal-sym"); if (bs) bs.textContent = W.cfg.symbol;
}
function on(sel, fn, ev = "click") { const e = $(sel); if (e) e.addEventListener(ev, fn); }

function renderNetSelect() {
  const sel = $("#net");
  sel.innerHTML = Object.values(NETWORKS).map((n) => `<option value="${n.id}">${n.label}</option>`).join("");
  sel.value = W.net;
  sel.onchange = async () => {
    W.net = sel.value; W.cfg = netConfig(W.net); await saveMeta();
    await lock(); // switching networks re-locks (different key/address space)
  };
}

async function render() {
  setSyms();
  const hasVault = !!(await getLocal(VAULT_KEY));
  if (W.seed && W.address) return renderHome();
  if (hasVault) return renderUnlock();
  return renderOnboard();
}

function renderOnboard() {
  mount(tpl("t-onboard"));
  on('[data-act="create"]', async () => {
    const m = Mnemonic.random(24);
    W._flow = { mode: "create", phrase: m.phrase };
    renderBackup(m.phrase);
  });
  on('[data-act="import"]', renderImport);
}

function renderBackup(phrase) {
  mount(tpl("t-backup"));
  const ol = $("#seed-words");
  phrase.split(/\s+/).forEach((w) => { const li = document.createElement("li"); li.textContent = w; ol.appendChild(li); });
  on('[data-act="copy-seed"]', () => navigator.clipboard.writeText(phrase));
  $("#bk-ack").addEventListener("change", (e) => ($("#bk-next").disabled = !e.target.checked));
  on('[data-act="to-setpass"]', () => renderSetPass());
}

function renderImport() {
  mount(tpl("t-import"));
  on('[data-act="back-onboard"]', renderOnboard);
  on('[data-act="import-next"]', () => {
    const phrase = $("#imp-phrase").value.trim().replace(/\s+/g, " ");
    if (!Mnemonic.validate(phrase)) { $("#imp-err").textContent = "Invalid recovery phrase"; return; }
    W._flow = { mode: "import", phrase };
    renderSetPass();
  });
}

function renderSetPass() {
  mount(tpl("t-setpass"));
  on('[data-act="finish-setup"]', async () => {
    const p1 = $("#pw1").value, p2 = $("#pw2").value;
    if (p1.length < 8) { $("#pass-err").textContent = "Password must be at least 8 characters"; return; }
    if (p1 !== p2) { $("#pass-err").textContent = "Passwords do not match"; return; }
    try {
      const vault = await createVault(p1, W._flow.phrase);
      await setLocal({ [VAULT_KEY]: vault });
      const seed = W._flow.phrase; W._flow = {};
      await activate(seed);
    } catch (e) { $("#pass-err").textContent = e.message; }
  });
}

function renderUnlock() {
  mount(tpl("t-unlock"));
  const go = async () => {
    const pw = $("#unlock-pw").value;
    try {
      const vault = await getLocal(VAULT_KEY);
      const seed = await openVault(pw, vault);
      await activate(seed);
    } catch (e) { $("#unlock-err").textContent = e.message || "Unlock failed"; }
  };
  on('[data-act="unlock"]', go);
  $("#unlock-pw").addEventListener("keydown", (e) => { if (e.key === "Enter") go(); });
  on('[data-act="forgot"]', async () => {
    if (confirm("Remove this wallet and restore from your recovery phrase? Make sure you have your 24 words.")) {
      await delLocal(VAULT_KEY); await delSession(SESSION_KEY); renderOnboard();
    }
  });
  setTimeout(() => $("#unlock-pw")?.focus(), 30);
}

function renderHome() {
  mount(tpl("t-home"));
  setSyms();
  $("#home-addr").textContent = W.address;
  updateBalanceDisplay();
  const ex = $("#explorer-link"); ex.href = W.cfg.explorer;
  on('[data-act="copy-addr"]', () => navigator.clipboard.writeText(W.address));
  on('[data-act="go-send"]', renderSend);
  on('[data-act="go-receive"]', renderReceive);
  on('[data-act="go-settings"]', renderSettings);
  on('[data-act="lock"]', lock);
  armLock();
  refresh();
}

function renderReceive() {
  mount(tpl("t-receive"));
  setSyms();
  $("#rx-addr").textContent = W.address;
  on('[data-act="copy-addr2"]', () => navigator.clipboard.writeText(W.address));
  // Same 24-word phrase also controls an EVM (0x) address; show it so the holder
  // can receive EVM-lane funds (or import into MetaMask). Sends auto-bridge.
  const ev = $("#rx-evm-addr"); if (ev && W.evm) ev.textContent = W.evm.address;
  on('[data-act="copy-evm"]', () => { if (W.evm) navigator.clipboard.writeText(W.evm.address); });
  on('[data-act="go-home"]', renderHome);
}

function renderSend() {
  mount(tpl("t-send"));
  setSyms();
  updateBalanceDisplay();
  on('[data-act="go-home"]', renderHome);
  on('[data-act="do-send"]', async () => {
    const to = $("#snd-to").value.trim(), amt = $("#snd-amt").value.trim();
    $("#snd-err").textContent = ""; $("#snd-ok").textContent = "";
    if (!confirm(`Send ${amt} ${W.cfg.symbol}\nto ${to}\non ${W.cfg.label}?`)) return;
    const btn = $("#snd-btn"); btn.disabled = true; btn.textContent = "Signing…";
    try {
      const txid = await send(to, amt);
      $("#snd-ok").innerHTML = `Sent ✓ <a class="link" target="_blank" href="${W.cfg.explorer}/#/tx/${txid}">${txid.slice(0, 16)}…</a>`;
      $("#snd-amt").value = "";
      armLock(); refresh();
    } catch (e) { $("#snd-err").textContent = e.message || String(e); }
    finally { btn.disabled = false; btn.textContent = "Review & send"; }
  });
}

function renderSettings() {
  mount(tpl("t-settings"));
  $("#set-lock").value = W.lockMinutes;
  $("#set-rpc").value = W.rpcOverride;
  const bondsBox = $("#set-bonds"); if (bondsBox) bondsBox.value = [...W.bonds].join("\n");
  on('[data-act="save-settings"]', async () => {
    W.lockMinutes = Math.max(1, Math.min(60, Number($("#set-lock").value) || 5));
    W.rpcOverride = $("#set-rpc").value.trim();
    // Parse bond outpoints (one per line; bad/empty lines are dropped). Normalized to txid:index.
    const refs = (bondsBox ? bondsBox.value : "").split(/[\s,]+/).map(normBondRef).filter(Boolean);
    W.bonds = new Set(refs);
    await saveMeta(); armLock();
    refresh(); // re-split UTXOs so the balance + "locked in bond" line update immediately
    alert("Saved");
  });
  on('[data-act="reveal-seed"]', async () => {
    const pw = prompt("Enter your password to reveal the recovery phrase");
    if (!pw) return;
    try { const seed = await openVault(pw, await getLocal(VAULT_KEY)); $("#seed-reveal").textContent = seed; }
    catch { alert("Incorrect password"); }
  });
  on('[data-act="remove"]', async () => {
    if (confirm("Remove this wallet from this device? You can only restore it with your recovery phrase.")) {
      await delLocal(VAULT_KEY); await delSession(SESSION_KEY); await lock();
    }
  });
  on('[data-act="go-home"]', renderHome);
}

/* ------------------------------- boot --------------------------------- */
(async function boot() {
  try { await init(); } catch (e) { view().innerHTML = `<div class="pad"><p class="err">Failed to load signing core: ${e.message}</p></div>`; return; }
  await loadMeta();
  renderNetSelect();
  // resume an unlocked session if still valid
  const sess = await getSession(SESSION_KEY);
  if (sess && sess.seed && sess.net === W.net && sess.exp > Date.now()) {
    await activate(sess.seed);
  } else {
    if (sess) await delSession(SESSION_KEY);
    await render();
  }
})();
