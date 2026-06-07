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
} from "./kaspa/kaspa.js";

import { createVault, openVault } from "./src/vault.js";
import { NETWORKS, DEFAULT_NETWORK, netConfig } from "./src/networks.js";

const VAULT_KEY = "misaka_vault";
const META_KEY = "misaka_meta"; // { net, lockMinutes, rpcOverride }
const SESSION_KEY = "unlocked"; // { net, seed, exp }

// mass / standardness (ML-DSA-87 sigs are large) — mirror the reference wallet.
const FEE_BUFFER = 5_000n, FEE_CAP = 20_000_000n, DUST = 20_000n;
const TRANSIENT_FACTOR = 4n, MASS_SAFETY = 430_000n, IN_OVERHEAD = 52n, TX_OVERHEAD = 320n;
const HARD_MAX_INPUTS = 60, SIG_BYTES_FALLBACK = 7300;

const W = { net: DEFAULT_NETWORK, cfg: netConfig(DEFAULT_NETWORK), seed: null, kp: null, address: null, rpc: null, connected: false, utxos: [], balance: 0n, lockMinutes: 5, rpcOverride: "", sigBytes: 0, _flow: {} };

const $ = (s, r = document) => r.querySelector(s);
const view = () => document.getElementById("view");
const tpl = (id) => document.getElementById(id).content.cloneNode(true);
const randomness32 = () => [...crypto.getRandomValues(new Uint8Array(32))].map((b) => b.toString(16).padStart(2, "0")).join("");
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
}
async function saveMeta() {
  await setLocal({ [META_KEY]: { net: W.net, lockMinutes: W.lockMinutes, rpcOverride: W.rpcOverride } });
}

/* ----------------------------- auto-lock ------------------------------ */
async function armLock() {
  const exp = Date.now() + W.lockMinutes * 60_000;
  if (W.seed) await setSession({ [SESSION_KEY]: { net: W.net, seed: W.seed, exp } });
  chrome.runtime.sendMessage({ target: "bg", cmd: "arm-lock", minutes: W.lockMinutes });
}
async function lock() {
  W.seed = W.kp = W.address = null;
  W.utxos = []; W.balance = 0n;
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

let _refreshing = false;
async function refresh() {
  if (!W.address || _refreshing) return;
  _refreshing = true;
  try {
    if (!W.connected && !(await connectRpc())) return;
    const res = await W.rpc.getUtxosByAddresses([W.address]);
    W.utxos = (res && res.entries) || [];
    W.balance = W.utxos.reduce((s, e) => s + amtOf(e), 0n);
    const b = $("#bal"); if (b) b.textContent = fmt(W.balance);
    const n = $("#utxo-n"); if (n) n.textContent = String(W.utxos.length);
    const av = $("#snd-avail"); if (av) av.textContent = fmt(W.balance) + " " + W.cfg.symbol;
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
async function send(toAddr, amountStr) {
  if (!Address.validate(toAddr)) throw new Error("Invalid recipient address");
  if (!toAddr.startsWith(W.cfg.prefix + ":")) throw new Error(`Address must be on ${W.cfg.label} (${W.cfg.prefix}:)`);
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
      let change = total - amount - fee;
      const outputs = [{ address: toAddr, amount }];
      if (change >= DUST) outputs.push({ address: W.address, amount: change });
      const signed = signTransactionMlDsa87(createTransaction(picked, outputs, 0n, undefined, 1), W.kp, randomness32());
      const resp = await W.rpc.submitTransaction({ transaction: signed, allowOrphan: false });
      return (resp && (resp.transactionId || resp.transaction_id)) || signed.id;
    }
  }
  throw new Error("Insufficient balance for amount + fee");
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
  $("#bal").textContent = fmt(W.balance);
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
  on('[data-act="go-home"]', renderHome);
}

function renderSend() {
  mount(tpl("t-send"));
  setSyms();
  $("#snd-avail").textContent = fmt(W.balance) + " " + W.cfg.symbol;
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
  on('[data-act="save-settings"]', async () => {
    W.lockMinutes = Math.max(1, Math.min(60, Number($("#set-lock").value) || 5));
    W.rpcOverride = $("#set-rpc").value.trim();
    await saveMeta(); armLock(); alert("Saved");
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
