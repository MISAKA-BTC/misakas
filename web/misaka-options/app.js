/* MISAKA Options - a front-end for the MISAKA chain's model market.
 *
 * ADR-0087: a position is bought from a protocol curve and sold back to it (never transferred).
 * ADR-0088: lines (a class, an owner, a name) and developer-signed versions.
 * ADR-0089: the EVM is the fold's window (read precompiles) and its hand (the writer).
 * ADR-0090: a market exists only once it is SEEDED with at least 100,000 MSK, locked for good;
 *           a position is a whole number, 500,000 a line; the curve is reserve x units = K taken
 *           from the row at every move, with no virtual reserve.
 *
 * Plain ES2020, no build step, no framework. Every number shown comes from an RPC reply or
 * from the chain's own curve arithmetic (ported below from consensus/core/src/palw_model_market_v1.rs).
 */
(() => {
'use strict';

// ============================================================================================
// 0. configuration
// ============================================================================================
const DEFAULTS = {
  WRPC_URL: '',
  EVM_RPC_URL: '/evm',
  CHAIN_ID: '0x4D534B',
  NETWORK_NAME: 'testnet-11',
  CLASS_IDS: [],
  EXPLORER_URL: 'https://misakascan.com',
  POLL_MS: 10000,
  LOG_LOOKBACK_BLOCKS: 5000,
  ADR_URL: 'https://github.com/MISAKA-BTC/misakas/tree/main/docs/adr',
  DOCS_URL: 'https://github.com/MISAKA-BTC/misakas/tree/main/docs',
};
const CFG = Object.assign({}, DEFAULTS, window.MISAKA_CONFIG || {});
const QS = new URLSearchParams(location.search);
const MOCK = QS.get('mock') === '1';
const SELFTEST = QS.get('selftest') === '1';
const STORE_PREFIX = MOCK ? 'mo-mock:' : 'mo:';
const CHAIN_ID_HEX = String(CFG.CHAIN_ID).toLowerCase();
const CHAIN_ID_BI = BigInt(CFG.CHAIN_ID);

const ADDR = {
  REGISTRY: '0x000000000000000000000000000000000000f010',
  AMM: '0x000000000000000000000000000000000000f011',
  POSITION: '0x000000000000000000000000000000000000f012',
  WRITER: '0x000000000000000000000000000000000000f013',
};
const FACADE_PREFIX = '4d50';
const FACADE_DOMAIN = 'misaka-evm/model-position-facade/v1';
const HOLDER_DOMAIN = 'misaka-palw/model-market/holder/evm/v1';
const NATIVE_SCALE_WEI = 10n ** 10n;      // wei per sompi
const WEI_PER_MSK = 10n ** 18n;
const SOMPI_PER_MSK = 100000000n;
const REFUSAL = { 1: 'not armed', 2: 'line missing', 3: 'class or line not active', 4: 'releases nothing', 5: 'below your floor', 6: 'market missing (not seeded)', 7: 'exceeds your position', 8: 'pays nothing', 9: 'other', 10: 'already seeded', 11: 'seed too small', 12: 'class closed (frozen)' };
const ACTION_NAME = { 1: 'buy', 2: 'sell', 3: 'seed' };
// wei per sompi; the value of a buy or a seed is whole sompi scaled to wei (the F002 rule)
const sompiToWei = (sompi) => bi(sompi) * NATIVE_SCALE_WEI;

function resolveWrpcUrl() {
  if (CFG.WRPC_URL) return CFG.WRPC_URL;
  if (!location.host) return '';
  return (location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/kaspa';
}
function resolveEvmUrl() {
  try { return new URL(CFG.EVM_RPC_URL, location.href).href; } catch (e) { return ''; }
}

// ============================================================================================
// 1. small utilities
// ============================================================================================
const $ = (sel, root) => (root || document).querySelector(sel);
const $$ = (sel, root) => Array.from((root || document).querySelectorAll(sel));
const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
class Raw { constructor(s) { this.s = s; } toString() { return this.s; } }
const raw = (s) => new Raw(s);
// Tagged template: interpolations are escaped unless wrapped in raw().
function h(strings, ...vals) {
  let out = '';
  for (let i = 0; i < strings.length; i++) {
    out += strings[i];
    if (i < vals.length) {
      const v = vals[i];
      if (v instanceof Raw) out += v.s;
      else if (Array.isArray(v)) out += v.map((x) => (x instanceof Raw ? x.s : esc(x))).join('');
      else out += esc(v);
    }
  }
  return new Raw(out);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function withTimeout(promise, ms, label) {
  let t;
  const timeout = new Promise((_, rej) => { t = setTimeout(() => rej(new Error((label || 'request') + ' timed out')), ms); });
  return Promise.race([promise.finally(() => clearTimeout(t)), timeout]);
}
async function pool(items, n, fn) {
  const out = new Array(items.length);
  let i = 0;
  async function worker() {
    while (i < items.length) { const k = i++; try { out[k] = await fn(items[k], k); } catch (e) { out[k] = undefined; } }
  }
  await Promise.all(Array.from({ length: Math.min(n, items.length) }, worker));
  return out;
}

// BigInt helpers ------------------------------------------------------------------------------
function bi(v) {
  if (typeof v === 'bigint') return v;
  if (v == null || v === '') return 0n;
  if (typeof v === 'number') return BigInt(Math.trunc(v));
  if (typeof v === 'boolean') return v ? 1n : 0n;
  const s = String(v).trim();
  if (/^-?0x[0-9a-f]+$/i.test(s)) return BigInt(s);
  if (/^-?\d+$/.test(s)) return BigInt(s);
  return 0n;
}
const divCeil = (a, b) => (a + b - 1n) / b;
const bmin = (a, b) => (a < b ? a : b);
const bmax = (a, b) => (a > b ? a : b);
const toHex = (v) => '0x' + bi(v).toString(16);
// JSON with big integers: quote integer literals of 16+ digits before parsing so u64 values survive.
function safeParse(text) {
  return JSON.parse(String(text).replace(/([:\[,]\s*)(-?\d{16,})(?=\s*[,}\]])/g, '$1"$2"'));
}
// ratio a/b as a float (for charts and percentages only, never for amounts)
function ratio(a, b) { if (bi(b) === 0n) return 0; return Number((bi(a) * 1000000n) / bi(b)) / 1000000; }

// bytes / hex ---------------------------------------------------------------------------------
function hexToBytes(hex) {
  hex = String(hex).replace(/^0x/i, '');
  if (hex.length % 2) hex = '0' + hex;
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.substr(i * 2, 2), 16);
  return out;
}
function bytesToHex(bytes) { let s = ''; for (const b of bytes) s += b.toString(16).padStart(2, '0'); return s; }
const utf8 = (s) => new TextEncoder().encode(s);
function concatBytes(...arrs) {
  const n = arrs.reduce((a, b) => a + b.length, 0);
  const out = new Uint8Array(n); let o = 0;
  for (const a of arrs) { out.set(a, o); o += a.length; }
  return out;
}
const isId128 = (s) => typeof s === 'string' && /^[0-9a-f]{128}$/i.test(s);
const isAddr = (s) => typeof s === 'string' && /^0x[0-9a-f]{40}$/i.test(s);
const normId = (s) => (isId128(s) ? s.toLowerCase() : null);

// formatting ----------------------------------------------------------------------------------
function groupInt(s) { return s.replace(/\B(?=(\d{3})+(?!\d))/g, ','); }
// value is an integer scaled by 10^decimals; render with at most maxDp and at least minDp decimals.
function fmtScaled(value, decimals, maxDp, minDp) {
  if (value == null) return '—';
  let v = bi(value);
  const neg = v < 0n; if (neg) v = -v;
  const base = 10n ** BigInt(decimals);
  const int = v / base;
  let frac = (v % base).toString().padStart(decimals, '0');
  maxDp = maxDp == null ? decimals : Math.min(maxDp, decimals);
  minDp = minDp == null ? 0 : minDp;
  frac = frac.slice(0, maxDp);
  while (frac.length > minDp && frac.endsWith('0')) frac = frac.slice(0, -1);
  return (neg ? '-' : '') + groupInt(int.toString()) + (frac.length ? '.' + frac : '');
}
const fmtMsk = (sompi, dp) => fmtScaled(sompi, 8, dp == null ? 4 : dp, 0);
const fmtPrice = (sompi) => fmtScaled(sompi, 8, 8, 2);
const fmtPx = (sompi) => fmtScaled(sompi, 8, 6, 2);   // dense tables: six decimals
// ADR-0090: a position is a whole number (one unit), so a position count is an integer
const fmtPos = (units) => (units == null ? '—' : groupInt(bi(units).toString()));
const fmtWeiMsk = (wei, dp) => fmtScaled(wei, 18, dp == null ? 4 : dp, 0);
const fmtInt = (v) => (v == null ? '—' : groupInt(bi(v).toString()));
function fmtPct(x, dp) { if (x == null || !isFinite(x)) return '—'; const s = x.toFixed(dp == null ? 2 : dp); return (x > 0 ? '+' : '') + s + '%'; }
const shortId = (id, n) => (id ? String(id).slice(0, n || 8) + '…' + String(id).slice(-4) : '—');
const shortAddr = (a) => (a ? a.slice(0, 6) + '…' + a.slice(-4) : '—');
const pad2 = (n) => String(n).padStart(2, '0');
function fmtTime(ts) { const d = new Date(ts); return pad2(d.getHours()) + ':' + pad2(d.getMinutes()) + ':' + pad2(d.getSeconds()); }
function fmtDateTime(ts) { const d = new Date(ts); return d.getFullYear() + '-' + pad2(d.getMonth() + 1) + '-' + pad2(d.getDate()) + ' ' + pad2(d.getHours()) + ':' + pad2(d.getMinutes()); }
function fmtAgo(ts) { const s = Math.max(0, Math.round((Date.now() - ts) / 1000)); if (s < 60) return s + 's ago'; if (s < 3600) return Math.round(s / 60) + 'm ago'; if (s < 86400) return Math.round(s / 3600) + 'h ago'; return Math.round(s / 86400) + 'd ago'; }
// decimal text -> integer scaled by 10^decimals; null when not a clean non-negative number
function parseDec(text, decimals) {
  const s = String(text || '').trim().replace(/,/g, '');
  const m = /^(\d*)(?:\.(\d*))?$/.exec(s);
  if (!m || (m[1] === '' && !m[2])) return null;
  const int = m[1] || '0';
  let frac = m[2] || '';
  if (frac.length > decimals) return null;
  frac = frac.padEnd(decimals, '0');
  return BigInt(int) * 10n ** BigInt(decimals) + BigInt(frac || '0');
}

// storage -------------------------------------------------------------------------------------
const store = {
  get(k, d) { try { const v = localStorage.getItem(STORE_PREFIX + k); return v == null ? d : JSON.parse(v); } catch (e) { return d; } },
  set(k, v) { try { localStorage.setItem(STORE_PREFIX + k, JSON.stringify(v)); } catch (e) { /* storage unavailable */ } },
  del(k) { try { localStorage.removeItem(STORE_PREFIX + k); } catch (e) { /* ignore */ } },
};

// ============================================================================================
// 2. hashes: keccak-256 (selectors, topics) and BLAKE2b-512 (facade address, EVM holder id)
// ============================================================================================
const MASK64 = (1n << 64n) - 1n;
const rotl64 = (x, n) => ((x << BigInt(n)) | (x >> BigInt(64 - n))) & MASK64;
const KECCAK_RC = [
  0x0000000000000001n, 0x0000000000008082n, 0x800000000000808an, 0x8000000080008000n, 0x000000000000808bn, 0x0000000080000001n,
  0x8000000080008081n, 0x8000000000008009n, 0x000000000000008an, 0x0000000000000088n, 0x0000000080008009n, 0x000000008000000an,
  0x000000008000808bn, 0x800000000000008bn, 0x8000000000008089n, 0x8000000000008003n, 0x8000000000008002n, 0x8000000000000080n,
  0x000000000000800an, 0x800000008000000an, 0x8000000080008081n, 0x8000000000008080n, 0x0000000080000001n, 0x8000000080008008n,
];
const KECCAK_ROT = [[0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14]];
function keccakF(A) {
  const C = new Array(5), D = new Array(5), B = new Array(25);
  for (let r = 0; r < 24; r++) {
    for (let x = 0; x < 5; x++) C[x] = A[x] ^ A[x + 5] ^ A[x + 10] ^ A[x + 15] ^ A[x + 20];
    for (let x = 0; x < 5; x++) D[x] = C[(x + 4) % 5] ^ rotl64(C[(x + 1) % 5], 1);
    for (let i = 0; i < 25; i++) A[i] ^= D[i % 5];
    for (let x = 0; x < 5; x++) for (let y = 0; y < 5; y++) {
      const rot = KECCAK_ROT[x][y];
      B[y + 5 * ((2 * x + 3 * y) % 5)] = rot ? rotl64(A[x + 5 * y], rot) : A[x + 5 * y];
    }
    for (let x = 0; x < 5; x++) for (let y = 0; y < 5; y++) A[x + 5 * y] = B[x + 5 * y] ^ ((~B[(x + 1) % 5 + 5 * y] & MASK64) & B[(x + 2) % 5 + 5 * y]);
    A[0] ^= KECCAK_RC[r];
  }
}
function keccak256(bytes) {
  const rate = 136;
  const padded = new Uint8Array(Math.ceil((bytes.length + 1) / rate) * rate);
  padded.set(bytes); padded[bytes.length] ^= 0x01; padded[padded.length - 1] ^= 0x80;
  const A = new Array(25).fill(0n);
  for (let off = 0; off < padded.length; off += rate) {
    for (let i = 0; i < rate / 8; i++) {
      let lane = 0n;
      for (let b = 7; b >= 0; b--) lane = (lane << 8n) | BigInt(padded[off + i * 8 + b]);
      A[i] ^= lane;
    }
    keccakF(A);
  }
  const out = new Uint8Array(32);
  for (let i = 0; i < 4; i++) { let lane = A[i]; for (let b = 0; b < 8; b++) { out[i * 8 + b] = Number(lane & 0xffn); lane >>= 8n; } }
  return out;
}

const B2B_IV = [0x6a09e667f3bcc908n, 0xbb67ae8584caa73bn, 0x3c6ef372fe94f82bn, 0xa54ff53a5f1d36f1n, 0x510e527fade682d1n, 0x9b05688c2b3e6c1fn, 0x1f83d9abfb41bd6bn, 0x5be0cd19137e2179n];
const B2B_SIGMA = [
  [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15], [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
  [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4], [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
  [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13], [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
  [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11], [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
  [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5], [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];
const rotr64 = (x, n) => ((x >> BigInt(n)) | (x << BigInt(64 - n))) & MASK64;
function b2bCompress(hs, block, t, last) {
  const m = new Array(16);
  for (let i = 0; i < 16; i++) { let w = 0n; for (let b = 7; b >= 0; b--) w = (w << 8n) | BigInt(block[i * 8 + b]); m[i] = w; }
  const v = hs.concat(B2B_IV);
  v[12] ^= t & MASK64; v[13] ^= (t >> 64n) & MASK64;
  if (last) v[14] = (~v[14]) & MASK64;
  const G = (a, b, c, d, x, y) => {
    v[a] = (v[a] + v[b] + x) & MASK64; v[d] = rotr64(v[d] ^ v[a], 32);
    v[c] = (v[c] + v[d]) & MASK64; v[b] = rotr64(v[b] ^ v[c], 24);
    v[a] = (v[a] + v[b] + y) & MASK64; v[d] = rotr64(v[d] ^ v[a], 16);
    v[c] = (v[c] + v[d]) & MASK64; v[b] = rotr64(v[b] ^ v[c], 63);
  };
  for (let r = 0; r < 12; r++) {
    const s = B2B_SIGMA[r % 10];
    G(0, 4, 8, 12, m[s[0]], m[s[1]]); G(1, 5, 9, 13, m[s[2]], m[s[3]]); G(2, 6, 10, 14, m[s[4]], m[s[5]]); G(3, 7, 11, 15, m[s[6]], m[s[7]]);
    G(0, 5, 10, 15, m[s[8]], m[s[9]]); G(1, 6, 11, 12, m[s[10]], m[s[11]]); G(2, 7, 8, 13, m[s[12]], m[s[13]]); G(3, 4, 9, 14, m[s[14]], m[s[15]]);
  }
  for (let i = 0; i < 8; i++) hs[i] ^= v[i] ^ v[i + 8];
}
// BLAKE2b with an optional key (the RFC 7693 keyed mode the node's blake2b_512_keyed uses).
function blake2b(data, key, outlen) {
  outlen = outlen || 64; key = key || new Uint8Array(0);
  const hs = B2B_IV.slice();
  hs[0] ^= 0x01010000n ^ (BigInt(key.length) << 8n) ^ BigInt(outlen);
  let input = data;
  if (key.length) { const kb = new Uint8Array(128); kb.set(key); input = concatBytes(kb, data); }
  const nblocks = Math.max(1, Math.ceil(input.length / 128));
  let t = 0n;
  for (let i = 0; i < nblocks; i++) {
    const last = i === nblocks - 1;
    const block = new Uint8Array(128);
    block.set(input.subarray(i * 128, Math.min(input.length, (i + 1) * 128)));
    t = last ? BigInt(input.length) : t + 128n;
    b2bCompress(hs, block, t, last);
  }
  const out = new Uint8Array(64);
  for (let i = 0; i < 8; i++) { let w = hs[i]; for (let b = 0; b < 8; b++) { out[i * 8 + b] = Number(w & 0xffn); w >>= 8n; } }
  return out.subarray(0, outlen);
}
// 0x4d50 || blake2b_512_keyed("misaka-evm/model-position-facade/v1", line_id)[..18]
function facadeDerived(lineId) {
  const d = blake2b(hexToBytes(lineId), utf8(FACADE_DOMAIN), 64);
  return '0x' + FACADE_PREFIX + bytesToHex(d.subarray(0, 18));
}
// evm_holder_v1(chain_id, address) = blake2b_512_keyed(domain, chain_id_le8 || address)
function holderIdDerived(chainId, address) {
  const cid = new Uint8Array(8); let c = bi(chainId);
  for (let i = 0; i < 8; i++) { cid[i] = Number(c & 0xffn); c >>= 8n; }
  return bytesToHex(blake2b(concatBytes(cid, hexToBytes(address)), utf8(HOLDER_DOMAIN), 64));
}

// ============================================================================================
// 3. ABI encoding for the native doors (static words only; string returns for name/symbol)
// ============================================================================================
const selCache = new Map();
const ABI = {
  selector(sig) { if (!selCache.has(sig)) selCache.set(sig, bytesToHex(keccak256(utf8(sig))).slice(0, 8)); return selCache.get(sig); },
  topic(sig) { return '0x' + bytesToHex(keccak256(utf8(sig))); },
  word(v) { return bi(v).toString(16).padStart(64, '0'); },
  addrWord(a) { return String(a).toLowerCase().replace(/^0x/, '').padStart(64, '0'); },
  idWords(id) { const s = String(id).toLowerCase(); return [s.slice(0, 64), s.slice(64, 128)]; },
  call(sig, ...words) { return '0x' + ABI.selector(sig) + words.join(''); },
  words(hex) { const s = String(hex || '').replace(/^0x/, ''); const out = []; for (let i = 0; i + 64 <= s.length; i += 64) out.push(s.slice(i, i + 64)); return out; },
  u(w) { return w == null ? 0n : BigInt('0x' + w); },
  addr(w) { return w == null ? null : '0x' + w.slice(24); },
  bool(w) { return w != null && BigInt('0x' + w) !== 0n; },
  str(hex) {
    const s = String(hex || '').replace(/^0x/, '');
    if (s.length < 128) return '';
    const off = Number(BigInt('0x' + s.slice(0, 64))) * 2;
    const len = Number(BigInt('0x' + s.slice(off, off + 64)));
    const data = s.slice(off + 64, off + 64 + len * 2);
    try { return new TextDecoder().decode(hexToBytes(data)); } catch (e) { return ''; }
  },
  isEmpty(hex) { return !hex || String(hex).replace(/^0x/, '').length === 0; },
};
const SIG = {
  chainDaa: 'chainDaa()', classCount: 'classCount()', classAt: 'classAt(uint256)', classRow: 'classRow(bytes32,bytes32)',
  lineCount: 'lineCount()', lineAt: 'lineAt(uint256)', linesOfCount: 'linesOfCount(bytes32,bytes32)', lineOfClassAt: 'lineOfClassAt(bytes32,bytes32,uint32)',
  line: 'line(bytes32,bytes32)', rootsInForceCount: 'rootsInForceCount(bytes32,bytes32)', facadeOf: 'facadeOf(bytes32,bytes32)', usage: 'usage(bytes32,bytes32,uint32)',
  market: 'market(bytes32,bytes32)', price: 'price(bytes32,bytes32)', quoteBuy: 'quoteBuy(bytes32,bytes32,uint64)', quoteSell: 'quoteSell(bytes32,bytes32,uint64)', constants: 'constants()',
  balanceOfAddress: 'balanceOfAddress(bytes32,bytes32,address)', holderIdOf: 'holderIdOf(address)', certified: 'certified(bytes32,bytes32,uint8)',
  symbol: 'symbol()', name: 'name()', decimals: 'decimals()', balanceOf: 'balanceOf(address)', buy: 'buy(uint256)', sell: 'sell(uint256,uint256)', seed: 'seed()',
  sendAction: 'sendAction(bytes)',
};
const EVT = {
  Bought: 'Bought(address,uint256,uint256,uint256)',
  Sold: 'Sold(address,uint256,uint256,uint256)',
  Seeded: 'Seeded(address,uint256,uint256)',
  Refused: 'Refused(address,uint8,uint256,bytes32)',
  ActionQueued: 'ActionQueued(address,uint8,bytes)',
};
const topics = () => ({ bought: ABI.topic(EVT.Bought), sold: ABI.topic(EVT.Sold), seeded: ABI.topic(EVT.Seeded), refused: ABI.topic(EVT.Refused), queued: ABI.topic(EVT.ActionQueued) });
// IMisakaModelWriter.sendAction data for a seed (ADR-0090 D3): 0x01 || 0x000003 || lineA || lineB.
// The site sends facade.seed() (the same action with the line filled in by the address); this is
// the writer form, pinned by the self-test and exposed for tools.
const ACTION_SEED = 3;
function seedActionData(lineId) { return '0x01' + '000003' + String(lineId).toLowerCase(); }

// ============================================================================================
// 4. the curve: ADR-0087 as amended by ADR-0090, ported from palw_model_market_v1.rs (BigInt, exact)
// ============================================================================================
// A position is the unit (no fraction); a line holds 500,000 of them; a market exists only once
// a seed of at least 100,000 MSK is locked into it; the curve is reserve x units = K with K taken
// from the row at every move, so the product never falls and the reserve never falls under the
// seed. There is no virtual reserve.
const CURVE_DEFAULTS = {
  unitsPerPosition: 1n,                       // PALW_MODEL_POSITION_UNITS_V1 (ADR-0090 D1: one, no fraction)
  supplyUnits: 500000n,                       // PALW_MODEL_SUPPLY_UNITS_V1 = 500,000 whole positions a line
  seedMinSompi: 100000n * SOMPI_PER_MSK,      // PALW_MODEL_SEED_MIN_SOMPI_V1 (100,000 MSK)
  burnPermille: 50n,                          // PALW_MODEL_BURN_PERMILLE_V1
  buybackPermille: 50n,                       // PALW_MODEL_BUYBACK_PERMILLE_V1 (ADR-0091: 5 % of a block's reward)
  legPermille: 10n,                           // PALW_MODEL_REGISTRANT_PERMILLE_V1 (the owner's leg since ADR-0088)
};
const curve = {
  consts(m) {
    const c = Object.assign({}, CURVE_DEFAULTS);
    if (m && m.supplyUnits && bi(m.supplyUnits) > 0n) c.supplyUnits = bi(m.supplyUnits);
    if (m && m.seedMinSompi && bi(m.seedMinSompi) > 0n) c.seedMinSompi = bi(m.seedMinSompi);
    if (m && m.consts) Object.assign(c, m.consts);
    return c;
  },
  // the product the next move must not fall under: the row's own reserve x units (ADR-0090 D2)
  k(m) { return bi(m.mskReserve) * bi(m.positionUnits); },
  // a row with a reserve is a market; the fold never writes one without, and a reader
  // synthesises a zero row for a line nobody seeded (reserve 0 = not seeded)
  seeded(m) { return !!m && bi(m.mskReserve) > 0n; },
  // the row a seed makes: the whole supply in the curve, the seed as the reserve, fee-free
  seed(seedSompi, c, seededBy, daa) {
    c = c || CURVE_DEFAULTS; seedSompi = bi(seedSompi);
    return { found: true, opened: seedSompi > 0n, openedDaa: bi(daa || 0), mskReserve: seedSompi, positionUnits: c.supplyUnits, soldUnits: 0n, burnedSompi: 0n, ownerPaid: 0n, contributorPaid: 0n, closedToBuys: false, seedSompi, seededBy: seededBy || null, supplyUnits: c.supplyUnits, seedMinSompi: c.seedMinSompi, buybackSompi: 0n, retiredUnits: 0n, classStatus: '' };
  },
  // the zero row of a line nobody seeded yet: no reserve, the whole supply, no price
  unseeded(c) { return curve.seed(0n, c); },
  // price of one position in sompi, rounded down (reserve / units); null for an unseeded row,
  // where the chain reports 0 and the site refuses to show a price that no curve stands behind
  price(m, c) {
    c = c || curve.consts(m);
    const u = bi(m.positionUnits), r = bi(m.mskReserve);
    if (u === 0n || r === 0n) return null;
    return (r * c.unitsPerPosition) / u;
  },
  // ADR-0091 D1: the slice of an escrowed worker reward that buys from the pair
  buybackSlice(escrow, c) { c = c || CURVE_DEFAULTS; return (bi(escrow) * c.buybackPermille) / 1000n; },
  // ADR-0091 D2: the reward's own move — the whole slice enters the reserve (no leg, no holder)
  // and what the curve gives up is RETIRED. null where no pair takes it (closed, unseeded, zero).
  buyback(m, slice) {
    slice = bi(slice);
    if (!m || m.closedToBuys || slice === 0n || bi(m.positionUnits) === 0n || bi(m.mskReserve) === 0n) return null;
    const k = curve.k(m), reserve = bi(m.mskReserve) + slice;
    let units = (k + reserve - 1n) / reserve;                    // ceil(K / reserve')
    if (units > bi(m.positionUnits)) units = bi(m.positionUnits);
    const retired = bi(m.positionUnits) - units;
    return { slice, retired, after: Object.assign({}, m, { mskReserve: reserve, positionUnits: units, buybackSompi: bi(m.buybackSompi || 0n) + slice, retiredUnits: bi(m.retiredUnits || 0n) + retired }) };
  },
  // burn + leg + net == gross, the remainder on the net leg
  feeSplit(gross, c) {
    c = c || CURVE_DEFAULTS; gross = bi(gross);
    const burn = (gross * c.burnPermille) / 1000n;
    const leg = (gross * c.legPermille) / 1000n;
    return { gross, burn, leg, net: gross - burn - leg };
  },
  // units_out = units - ceil(K / (reserve + net)), K = reserve x units of this row; null when
  // nothing is released (a dust buy), the market is closed to buys, or the line is not seeded
  buyQuote(m, mskIn, c) {
    c = c || curve.consts(m); mskIn = bi(mskIn);
    const units = bi(m.positionUnits), reserve = bi(m.mskReserve);
    if (m.closedToBuys || mskIn <= 0n || units === 0n || reserve === 0n) return null;
    const fees = curve.feeSplit(mskIn, c);
    const k = curve.k(m);
    const xAfter = reserve + fees.net;
    const unitsAfter = divCeil(k, xAfter);
    if (unitsAfter > units) return null;
    const unitsOut = units - unitsAfter;
    if (unitsOut === 0n) return null;
    const after = Object.assign({}, m, {
      mskReserve: reserve + fees.net, positionUnits: units - unitsOut,
      soldUnits: bi(m.soldUnits) + unitsOut, burnedSompi: bi(m.burnedSompi) + fees.burn,
    });
    return { fees, unitsOut, after, priceAfter: curve.price(after, c) };
  },
  // gross = reserve - ceil(K / (units + unitsIn)), never more than the reserve; null when the
  // curve pays nothing or the sell would put more than the supply back
  sellQuote(m, unitsIn, c) {
    c = c || curve.consts(m); unitsIn = bi(unitsIn);
    if (unitsIn <= 0n) return null;
    const reserve = bi(m.mskReserve);
    const unitsAfter = bi(m.positionUnits) + unitsIn;
    // ADR-0091 D4: the curve holds at most the supply less what the reward retired
    if (unitsAfter > c.supplyUnits - bi(m.retiredUnits || 0n)) return null;
    const xAfter = divCeil(curve.k(m), unitsAfter);
    if (xAfter > reserve) return null;
    const gross = bmin(reserve - xAfter, reserve);
    if (gross === 0n) return null;
    const fees = curve.feeSplit(gross, c);
    const after = Object.assign({}, m, { mskReserve: reserve - gross, positionUnits: unitsAfter, burnedSompi: bi(m.burnedSompi) + fees.burn });
    return { fees, after, priceAfter: curve.price(after, c) };
  },
  // the least gross MSK (sompi) whose buy releases at least `units` whole positions; null when the curve cannot
  buyCostForUnits(m, units, c) {
    c = c || curve.consts(m); units = bi(units);
    const have = bi(m.positionUnits), reserve = bi(m.mskReserve);
    if (m.closedToBuys || reserve === 0n || units <= 0n || units >= have) return null;
    const need = divCeil(curve.k(m), have - units) - reserve;
    // start a few sompi under the estimate (the fee floors round in the buyer's favour) and step up
    let gross = bmax((bmax(need, 1n) * 1000n) / (1000n - c.burnPermille - c.legPermille) - 3n, 1n);
    for (let i = 0; i < 64; i++) {
      const q = curve.buyQuote(m, gross, c);
      if (q && q.unitsOut >= units) return { gross, quote: q };
      gross += 1n;
    }
    return null;
  },
};

// The market row as this site holds it: bigint fields, normalised from wRPC or the AMM precompile.
// `seeded` is the one fact that decides whether a curve exists: reserve > 0 (the wRPC's `opened`
// and the window's `exists` say the same thing; a reader that synthesises a row for an unseeded
// line gives it no reserve).
function normMarket(src, source) {
  const m = {
    found: !!src.found, openedDaa: bi(src.openedDaa), mskReserve: bi(src.mskReserve),
    positionUnits: bi(src.positionUnits), soldUnits: bi(src.soldUnits), burnedSompi: bi(src.burnedSompi),
    ownerPaid: bi(src.registrantPaidSompi != null ? src.registrantPaidSompi : src.ownerPaid), contributorPaid: bi(src.contributorPaidSompi != null ? src.contributorPaidSompi : src.contributorPaid),
    closedToBuys: !!src.closedToBuys, supplyUnits: bi(src.supplyUnits) || CURVE_DEFAULTS.supplyUnits,
    // the seed and its payer travel on the wRPC only; the AMM window has no such word (null = not served)
    seedSompi: src.seedSompi != null ? bi(src.seedSompi) : null, seededBy: normId(src.seededBy) || null,
    seedMinSompi: bi(src.seedMinSompi) || (db.constsFromChain && db.constsFromChain.seedMinSompi) || CURVE_DEFAULTS.seedMinSompi,
    // ADR-0091: both lanes serve these; a node from before it serves neither (null = not served)
    buybackSompi: src.buybackSompi != null ? bi(src.buybackSompi) : null, retiredUnits: src.retiredUnits != null ? bi(src.retiredUnits) : null,
    classStatus: src.classStatus || '', source, at: Date.now(),
  };
  m.seeded = curve.seeded(m);        // reserve > 0: the same guard the fold's quote applies
  m.opened = m.seeded;
  // A node from before ADR-0090 serves a virtual reserve and no least seed; its units are 10^-6 of
  // a position and its curve is another curve. Its row is read as unseeded (no reserve) and its
  // position counts are not shown, because they would be in the wrong unit.
  m.legacy = src.seedMinSompi == null && bi(src.virtualSompi) > 0n;
  const reported = src.priceSompiPerPosition != null ? bi(src.priceSompiPerPosition) : null;
  m.price = m.seeded ? (reported != null && reported > 0n ? reported : curve.price(m)) : null;
  return m;
}
// the class status as the RPC names it (`Active`, `Registered { activation_daa: n, .. }`, `Frozen {..}`, `Dormant {..}`)
function parseClassStatus(s) {
  s = String(s || '').trim(); if (!s) return null;
  const head = (/^[A-Za-z]+/.exec(s) || [''])[0];
  const daa = /activation_daa:\s*(\d+)/.exec(s);
  const since = /since_daa:\s*(\d+)/.exec(s);
  return { head, activationDaa: daa ? bi(daa[1]) : null, sinceDaa: since ? bi(since[1]) : null, raw: s };
}
const CLASS_STATUS_CODE = { 0: 'Active', 1: 'Frozen', 2: 'Registered', 3: 'Dormant' };

// ============================================================================================
// 5. transports: wRPC (JSON over WebSocket), EVM JSON-RPC (HTTP POST), the wallet (EIP-1193)
// ============================================================================================
const status = { wrpc: 'idle', wrpcErr: '', evm: 'idle', evmErr: '', listeners: new Set() };
function setStatus(patch) { Object.assign(status, patch); for (const fn of status.listeners) { try { fn(); } catch (e) { /* ignore */ } } }

class WRpc {
  constructor(url) { this.url = url; this.ws = null; this.pending = new Map(); this.nextId = 1; this.opening = null; this.failures = 0; this.nextTry = 0; }
  connect() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) return Promise.resolve();
    if (this.opening) return this.opening;
    if (!this.url) return Promise.reject(new Error('wRPC URL not configured'));
    // back off after failures (5 s, 10 s, ... 60 s) so a dead endpoint is not hammered every poll
    if (Date.now() < this.nextTry) return Promise.reject(new Error('wRPC unreachable (retry in ' + Math.ceil((this.nextTry - Date.now()) / 1000) + ' s)'));
    setStatus({ wrpc: 'connecting' });
    this.opening = new Promise((resolve, reject) => {
      let ws;
      try { ws = new WebSocket(this.url); } catch (e) { this.opening = null; setStatus({ wrpc: 'down', wrpcErr: e.message }); reject(e); return; }
      const timer = setTimeout(() => { try { ws.close(); } catch (e) { /* ignore */ } }, 8000);
      ws.onopen = () => { clearTimeout(timer); this.ws = ws; this.opening = null; this.failures = 0; this.nextTry = 0; setStatus({ wrpc: 'up', wrpcErr: '' }); resolve(); };
      ws.onmessage = (ev) => this.onMessage(ev.data);
      ws.onerror = () => { /* the close event carries the failure */ };
      ws.onclose = () => {
        clearTimeout(timer);
        const wasOpen = this.ws === ws;
        this.ws = null; this.opening = null;
        for (const [, p] of this.pending) p.reject(new Error('wRPC disconnected'));
        this.pending.clear();
        if (!wasOpen) { this.failures++; this.nextTry = Date.now() + Math.min(60000, 5000 * 2 ** Math.min(this.failures - 1, 4)); }
        // A socket that WORKED and then closed is not a dead endpoint: this client keeps one
        // connection and reopens it on the next read, so calling that "down" told the reader the
        // node was unreachable while it was answering. Only a connect that never opened is "down".
        setStatus(wasOpen ? { wrpc: 'idle', wrpcErr: 'reconnects on the next read' } : { wrpc: 'down', wrpcErr: 'cannot connect' });
        if (!wasOpen) reject(new Error('wRPC unreachable at ' + this.url));
      };
    });
    return this.opening;
  }
  onMessage(text) {
    let msg; try { msg = safeParse(text); } catch (e) { return; }
    if (msg == null || msg.id == null) return;               // a notification, or noise
    const p = this.pending.get(msg.id); if (!p) return;
    this.pending.delete(msg.id);
    if (msg.error) p.reject(new Error(typeof msg.error === 'string' ? msg.error : (msg.error.message || 'wRPC error')));
    else if (msg.params && msg.params.error) p.reject(new Error(String(msg.params.error)));
    else p.resolve(msg.params == null ? {} : msg.params);
  }
  async call(method, params, timeoutMs) {
    await this.connect();
    const id = this.nextId++;
    const done = new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
    this.ws.send(JSON.stringify({ id, method, params: params || {} }));
    try { return await withTimeout(done, timeoutMs || 10000, method); }
    finally { this.pending.delete(id); }
  }
}
const wrpc = new WRpc(resolveWrpcUrl());
async function rpc(method, params) {
  if (MOCK && window.MISAKA_MOCK) { const r = await window.MISAKA_MOCK.wrpc(method, params || {}); setStatus({ wrpc: 'up', wrpcErr: '' }); return r; }
  return wrpc.call(method, params);
}

const evm = {
  url: resolveEvmUrl(), nextId: 1,
  async rpc(method, params) {
    if (MOCK && window.MISAKA_MOCK) { const r = await window.MISAKA_MOCK.evm(method, params || []); setStatus({ evm: 'up', evmErr: '' }); return r; }
    if (!this.url) throw new Error('EVM RPC URL not configured');
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), 10000);
    try {
      const res = await fetch(this.url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: this.nextId++, method, params: params || [] }), signal: ctrl.signal });
      if (!res.ok) throw new Error('HTTP ' + res.status);
      const body = safeParse(await res.text());
      if (body.error) throw new Error(body.error.message || 'EVM RPC error');
      setStatus({ evm: 'up', evmErr: '' });
      return body.result;
    } catch (e) {
      if (!/revert|execution|invalid|gas|nonce|insufficient|unknown|exist|reason/i.test(e.message)) setStatus({ evm: 'down', evmErr: e.message });
      throw e;
    } finally { clearTimeout(timer); }
  },
  async call(to, data) { const r = await this.rpc('eth_call', [{ to, data }, 'latest']); return typeof r === 'string' ? r : '0x'; },
  // the fold's clock; null when the doors are empty accounts (the fence is dormant)
  async chainDaa() { const r = await this.call(ADDR.REGISTRY, ABI.call(SIG.chainDaa)); return ABI.isEmpty(r) ? null : ABI.u(ABI.words(r)[0]); },
};

const wallet = {
  provider: null, account: null, chainId: null, listeners: new Set(),
  detect() { this.provider = (MOCK && window.MISAKA_MOCK && window.MISAKA_MOCK.ethereum) || window.ethereum || null; return !!this.provider; },
  emit() { for (const fn of this.listeners) { try { fn(); } catch (e) { /* ignore */ } } },
  onChain() { return this.chainId != null && this.chainId.toLowerCase() === CHAIN_ID_HEX; },
  async init() {
    if (!this.detect()) return;
    try {
      const accts = await this.provider.request({ method: 'eth_accounts' });
      if (accts && accts.length && store.get('wallet:auto', MOCK)) this.account = accts[0].toLowerCase();
      this.chainId = String(await this.provider.request({ method: 'eth_chainId' }));
    } catch (e) { /* not connected */ }
    if (this.provider.on) {
      this.provider.on('accountsChanged', (a) => { this.account = a && a.length ? a[0].toLowerCase() : null; this.emit(); });
      this.provider.on('chainChanged', (c) => { this.chainId = String(c); this.emit(); });
    }
    this.emit();
  },
  async connect() {
    if (!this.detect()) throw new Error('No EIP-1193 wallet found. Install MetaMask (or another injected wallet) and reload.');
    const accts = await this.provider.request({ method: 'eth_requestAccounts' });
    this.account = accts && accts.length ? accts[0].toLowerCase() : null;
    store.set('wallet:auto', true);
    this.chainId = String(await this.provider.request({ method: 'eth_chainId' }));
    this.emit();
    if (!this.onChain()) await this.ensureChain();
  },
  disconnect() { this.account = null; store.set('wallet:auto', false); this.emit(); },
  async ensureChain() {
    const rpcUrl = evm.url;
    try { await this.provider.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: CHAIN_ID_HEX }] }); }
    catch (e) {
      const code = e && (e.code || (e.data && e.data.originalError && e.data.originalError.code));
      if (code === 4902 || /unrecognized|not added|4902/i.test(String(e.message))) {
        await this.provider.request({ method: 'wallet_addEthereumChain', params: [{
          chainId: CHAIN_ID_HEX, chainName: 'MISAKA ' + CFG.NETWORK_NAME,
          nativeCurrency: { name: 'MSK', symbol: 'MSK', decimals: 18 },
          rpcUrls: [rpcUrl], blockExplorerUrls: CFG.EXPLORER_URL ? [CFG.EXPLORER_URL] : undefined,
        }] });
      } else throw e;
    }
    this.chainId = String(await this.provider.request({ method: 'eth_chainId' }));
    this.emit();
    if (!this.onChain()) throw new Error('The wallet is not on the MISAKA chain (' + CFG.CHAIN_ID + ').');
  },
  async sendTx(tx) { return this.provider.request({ method: 'eth_sendTransaction', params: [tx] }); },
};

// ============================================================================================
// 6. local records: price history (sampled), sent transactions
// ============================================================================================
const history = {
  cache: new Map(),
  load(id) { if (!this.cache.has(id)) this.cache.set(id, store.get('hist:' + id, [])); return this.cache.get(id); },
  save(id) { store.set('hist:' + id, this.load(id)); },
  add(id, price, ts, source) {
    if (price == null) return;
    const arr = this.load(id); ts = ts || Date.now(); const p = bi(price).toString();
    const last = arr[arr.length - 1];
    if (last && last[1] === p && ts - last[0] < 60000) return;     // unchanged: one sample a minute is enough
    if (last && ts <= last[0] && source !== 'event') return;
    arr.push([ts, p, source || 's']);
    arr.sort((a, b) => a[0] - b[0]);
    if (arr.length > 2400) { const keep = arr.slice(-1200); const old = arr.slice(0, -1200).filter((_, i) => i % 2 === 0); arr.splice(0, arr.length, ...old, ...keep); }
    this.save(id);
  },
  points(id, sinceMs) { const now = Date.now(); return this.load(id).filter((s) => !sinceMs || s[0] >= now - sinceMs).map((s) => ({ t: s[0], p: bi(s[1]) })); },
  // 24h change against the sample nearest to 24h ago (or the oldest one, labelled), null with fewer than two samples
  change24h(id, nowPrice) {
    const arr = this.load(id); if (!arr.length || nowPrice == null) return null;
    const target = Date.now() - 86400000;
    let best = null;
    for (const s of arr) { if (!best || Math.abs(s[0] - target) < Math.abs(best[0] - target)) best = s; }
    if (!best || arr.length < 2 && best[1] === bi(nowPrice).toString()) return null;
    const then = bi(best[1]); if (then === 0n) return null;
    const pct = Number(((bi(nowPrice) - then) * 1000000n) / then) / 10000;
    return { pct, since: best[0], partial: best[0] > target + 3600000 };
  },
};

const txlog = {
  list: store.get('txs', []),
  claimed: new Set(store.get('txs:claimed', [])),
  save() { store.set('txs', this.list.slice(-200)); store.set('txs:claimed', Array.from(this.claimed).slice(-400)); },
  add(rec) { this.list.push(rec); this.save(); },
  update(hash, patch) { const r = this.list.find((x) => x.hash === hash); if (r) { Object.assign(r, patch); this.save(); } return r; },
  forAccount(acct) { return this.list.filter((x) => !acct || x.from === acct).slice().reverse(); },
};

// ============================================================================================
// 7. the data layer: classes, lines, markets, positions, settlements
// ============================================================================================
const db = {
  classes: new Map(),   // classId -> { classId, status, isBase, registrant, registeredDaa, source }
  lines: new Map(),     // lineId -> LineRec
  armed: null,          // true: palw_model_evm doors answer; false: empty accounts; null: unknown (EVM unreachable)
  evmDaa: null,
  chain: { daa: null, network: null, at: 0 },
  constsFromChain: null,
  listeners: new Set(),
  emit() { for (const fn of this.listeners) { try { fn(); } catch (e) { console.error(e); } } },
  line(id) { return this.lines.get(id) || null; },
  upsertLine(id, patch) {
    id = normId(id); if (!id) return null;
    let rec = this.lines.get(id);
    if (!rec) { rec = { lineId: id, classId: null, name: null, symbol: null, facade: null, facadeSource: null, row: null, info: null, market: null, usage: null, versions: null, proposals: null, err: null }; this.lines.set(id, rec); }
    Object.assign(rec, patch);
    if (!rec.symbol) rec.symbol = 'MP-' + id.slice(0, 8);
    if (!rec.facade) { rec.facade = facadeDerived(id); rec.facadeSource = 'derived'; }
    return rec;
  },
  persist() {
    store.set('lines', Array.from(this.lines.values()).map((r) => ({ lineId: r.lineId, classId: r.classId, name: r.name, symbol: r.symbol })));
    store.set('classes', Array.from(this.classes.keys()));
  },
  // What this line is called, and — as important — WHO SAID SO. The chain's own name (an
  // ADR-0088 node serves it) wins; `config.js`'s catalogue is the fallback for a node that does
  // not; the derived MP- symbol is the last resort. Every caller that shows a catalogue title
  // marks it, because this site's rule is that a figure names its source.
  named(rec) {
    if (!rec) return { text: 'this line', source: 'none' };
    if (rec.name) return { text: rec.name, source: 'chain' };
    const c = catalogue(rec);
    if (c && c.title) return { text: c.title, source: 'catalogue', entry: c };
    return { text: rec.symbol || shortId(rec.lineId), source: 'id' };
  },
  label(rec) { return db.named(rec).text; },
};

// `config.js`'s MODELS, by class id or line id. A line the catalogue does not know reads as its
// symbol, which is what it did before this existed.
function catalogue(rec) {
  const m = CFG.MODELS || {};
  if (!rec) return null;
  return (rec.classId && m[rec.classId]) || m[rec.lineId] || null;
}
// The line under a title: what the chain registered, when the catalogue knows it.
function modelSub(rec) {
  const c = catalogue(rec);
  return c && c.variant ? c.variant : null;
}
// The mark a catalogue title carries, so nobody reads this site's word as the chain's.
const CATALOGUE_NOTE = 'This name is from the site\'s own catalogue (config.js), not from the chain: this node serves no line name. The id below is the chain fact.';
// The catalogue's own row: the repository the artifact comes from, what the chain registered of
// it, and the size. Empty for a line the catalogue does not know.
function catalogueRows(rec) {
  const c = catalogue(rec);
  if (!c) return '';
  return h`<dt>Model</dt><dd>${c.hf ? raw('<a href="' + esc(c.hf) + '" target="_blank" rel="noopener">' + esc(c.title) + '</a>') : c.title}${raw('<span class="dim tiny" title="' + esc(CATALOGUE_NOTE) + '"> \u24d8 site catalogue</span>')}</dd>
    ${c.variant ? h`<dt>Registered as</dt><dd>${c.variant}${c.params ? ' · ' + c.params + ' parameters' : ''}</dd>` : ''}
    ${c.artifact ? h`<dt>Artifact</dt><dd class="tiny">${c.artifact}</dd>` : ''}`;
}
function nameCell(rec) {
  const n = db.named(rec), sub = modelSub(rec);
  const mark = n.source === 'catalogue' ? h`<span class="dim tiny" title="${CATALOGUE_NOTE}"> \u24d8</span>` : '';
  const sym = n.text === rec.symbol ? '' : h` <span class="dim tiny mono">${rec.symbol}</span>`;
  return h`<b>${n.text}</b>${mark}${sym}${sub ? h`<br><span class="dim tiny">${sub}</span>` : ''}`;
}

async function refreshChainInfo() {
  try {
    const r = await rpc('getBlockDagInfo', {});
    db.chain = { daa: bi(r.virtualDaaScore), network: r.network || null, blockCount: bi(r.blockCount), at: Date.now() };
  } catch (e) { db.chain = Object.assign({}, db.chain, { err: e.message }); }
  try {
    const daa = await evm.chainDaa();
    db.armed = daa != null; db.evmDaa = daa;
    if (db.armed && !db.constsFromChain) {
      const r = await evm.call(ADDR.AMM, ABI.call(SIG.constants));
      const w = ABI.words(r);
      // ADR-0090: the third word carries the least seed (it carried the virtual reserve before)
      if (w.length >= 5) db.constsFromChain = { supplyUnits: ABI.u(w[0]), unitsPerPosition: ABI.u(w[1]), seedMinSompi: ABI.u(w[2]), burnPermille: ABI.u(w[3]), legPermille: ABI.u(w[4]) };
    }
  } catch (e) { db.armed = null; }
  db.emit();
}

// class ids: config + remembered + (when the doors answer) the registry's enumeration
async function discoverClasses() {
  for (const id of (CFG.CLASS_IDS || []).concat(store.get('classes', []))) {
    const n = normId(id); if (n && !db.classes.has(n)) db.classes.set(n, { classId: n, source: 'config' });
  }
  if (db.armed) {
    try {
      const count = Number(ABI.u(ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.classCount)))[0]));
      const ids = await pool(Array.from({ length: Math.min(count, 256) }, (_, i) => i), 4, async (i) => {
        const w = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.classAt, ABI.word(i))));
        return w.length >= 2 ? w[0] + w[1] : null;
      });
      for (const id of ids) {
        if (!id) continue;
        const w = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.classRow, ...ABI.idWords(id))));
        const row = w.length >= 8 ? { status: Number(ABI.u(w[0])), statusLabel: CLASS_STATUS_CODE[Number(ABI.u(w[0]))] || null, sharePermille: Number(ABI.u(w[1])), isBase: ABI.bool(w[4]), registrant: ABI.u(w[5]) + ABI.u(w[6]) ? w[5] + w[6] : null, registeredDaa: ABI.u(w[7]) } : {};
        if (row.isBase) continue;                       // the floor has no line and no market
        db.classes.set(id, Object.assign({ classId: id, source: 'registry' }, row));
      }
    } catch (e) { /* the registry is optional */ }
  }
}

function lineFromRpcRow(row) {
  return db.upsertLine(row.lineId, { classId: normId(row.classId), name: row.name || null, row });
}
// lines of every known class: wRPC first, the registry window second, a bare founding line last
async function discoverLines() {
  for (const r of store.get('lines', [])) if (normId(r.lineId)) db.upsertLine(r.lineId, { classId: normId(r.classId), name: r.name || null, symbol: r.symbol || null });
  await pool(Array.from(db.classes.keys()), 3, async (classId) => {
    let done = false;
    try {
      const r = await rpc('getPalwModelLines', { classId });
      if (r && r.exists) { for (const row of r.lines || []) lineFromRpcRow(row); done = true; }
    } catch (e) { /* fall through */ }
    if (!done && db.armed) {
      try {
        const n = Number(ABI.u(ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.linesOfCount, ...ABI.idWords(classId))))[0]));
        const ids = [classId];
        for (let i = 0; i < Math.min(n, 64); i++) {
          const w = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.lineOfClassAt, ...ABI.idWords(classId), ABI.word(i))));
          if (w.length >= 2 && ABI.u(w[0]) + ABI.u(w[1]) !== 0n) ids.push(w[0] + w[1]);
        }
        for (const id of new Set(ids)) db.upsertLine(id, { classId });
        done = true;
      } catch (e) { /* fall through */ }
    }
    if (!done && !db.lines.has(classId)) db.upsertLine(classId, { classId });
  });
  db.persist();
  db.emit();
}

async function marketFromAmm(lineId) {
  const w = ABI.words(await evm.call(ADDR.AMM, ABI.call(SIG.market, ...ABI.idWords(lineId))));
  if (w.length < 9) return null;
  // ADR-0091 appended two words; a node from before it answers nine and serves neither
  const buyback = w.length >= 11 ? ABI.u(w[9]) : null, retired = w.length >= 11 ? ABI.u(w[10]) : null;
  const c = db.constsFromChain || CURVE_DEFAULTS;
  // `exists` (word 8) is false for a line nobody seeded: the window answers the zero row
  return normMarket({
    found: true, opened: ABI.bool(w[8]), openedDaa: ABI.u(w[0]), mskReserve: ABI.u(w[1]), positionUnits: ABI.u(w[2]), soldUnits: ABI.u(w[3]),
    burnedSompi: ABI.u(w[4]), ownerPaid: ABI.u(w[5]), contributorPaid: ABI.u(w[6]), closedToBuys: ABI.bool(w[7]), supplyUnits: c.supplyUnits, seedMinSompi: c.seedMinSompi,
    buybackSompi: buyback, retiredUnits: retired,
  }, 'evm');
}
// the class's status and certification as the chain answers them: the wRPC's market row carries
// the status string, getPalwProducerFacts the free-prompt certification; the registry window
// carries the status code and both lanes' certification when the doors are armed
async function classFacts(classId) {
  const f = { classId, status: null, certifiedAttempt: null, certifiedFp: null, exists: null, source: [] };
  try {
    const r = await rpc('getPalwModelMarket', { lineId: classId, classId });
    if (r && r.found) { f.exists = true; f.status = parseClassStatus(r.classStatus); f.source.push('wRPC getPalwModelMarket'); }
    else if (r) f.exists = false;
  } catch (e) { /* the window may answer */ }
  try {
    const r = await rpc('getPalwProducerFacts', { classId, bondTransactionId: '', bondIndex: 0, withBond: false });
    if (r && r.available) { f.certifiedFp = !!r.fpCertified; if (r.isBaseClass) f.isBase = true; f.source.push('wRPC getPalwProducerFacts'); if (f.exists == null) f.exists = true; }
    else if (r && f.exists == null) f.exists = false;
  } catch (e) { /* optional */ }
  if (db.armed) {
    try {
      const w = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.classRow, ...ABI.idWords(classId))));
      if (w.length >= 8 && (ABI.u(w[7]) !== 0n || ABI.u(w[0]) !== 0n || ABI.bool(w[4]))) {
        f.exists = true;
        if (!f.status) f.status = { head: CLASS_STATUS_CODE[Number(ABI.u(w[0]))] || 'code ' + ABI.u(w[0]), activationDaa: null, sinceDaa: null, raw: 'registry classRow' };
        f.registeredDaa = ABI.u(w[7]); f.sharePermille = Number(ABI.u(w[1]));
        const a = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.certified, ...ABI.idWords(classId), ABI.word(0))));
        const p = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.certified, ...ABI.idWords(classId), ABI.word(1))));
        if (a.length) f.certifiedAttempt = ABI.bool(a[0]);
        if (p.length && f.certifiedFp == null) f.certifiedFp = ABI.bool(p[0]);
        f.source.push('registry window');
      } else if (f.exists == null) f.exists = false;
    } catch (e) { /* the window is optional */ }
  }
  return f;
}
async function refreshMarket(lineId) {
  const rec = db.upsertLine(lineId, {});
  let m = null, err = null;
  // `classId` rides along for a node that predates ADR-0088 (its request is keyed by class; the
  // founding line's id is the class id, and serde ignores the key it does not know either way)
  try { const r = await rpc('getPalwModelMarket', { lineId, classId: lineId }); if (r && r.found) m = normMarket(r, 'wrpc'); else if (r) { rec.notFound = true; } }
  catch (e) { err = e.message; }
  if (!m && db.armed) { try { m = await marketFromAmm(lineId); err = null; } catch (e) { err = err || e.message; } }
  if (m) { rec.market = m; rec.notFound = false; history.add(lineId, m.price, Date.now(), 's'); }
  rec.err = err;
  db.emit();
  return rec;
}
async function refreshLineInfo(lineId) {
  const rec = db.upsertLine(lineId, {});
  try {
    const r = await rpc('getPalwModelLine', { lineId });
    if (r && r.exists) { rec.info = r; if (r.line) lineFromRpcRow(r.line); }
    else if (r) rec.notFound = true;
  } catch (e) {
    if (db.armed) {
      try {
        const w = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.line, ...ABI.idWords(lineId))));
        if (w.length >= 14) {
          const cls = w[0] + w[1];
          if (ABI.u(w[0]) + ABI.u(w[1]) !== 0n) rec.classId = cls;
          rec.row = rec.row || {};
          Object.assign(rec.row, {
            lineId, classId: cls, ownerPayoutPayload: ABI.u(w[2]) + ABI.u(w[3]) ? w[2] + w[3] : null,
            developerPayoutPayload: ABI.u(w[4]) + ABI.u(w[5]) ? w[4] + w[5] : null, maintainerPayoutPayload: ABI.u(w[6]) + ABI.u(w[7]) ? w[6] + w[7] : null,
            current: Number(ABI.u(w[8])), versionsPublished: Number(ABI.u(w[9])), previews: Array(Number(ABI.u(w[10]))).fill(0),
            contributorPermilleOfLeg: Number(ABI.u(w[11])), status: Number(ABI.u(w[12])) === 1 ? 'Retired' : 'Active', nameHash: w[13], source: 'evm',
          });
          const n = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.rootsInForceCount, ...ABI.idWords(cls))));
          rec.info = { exists: true, lineId, line: rec.row, rootsInForce: Array(Number(ABI.u(n[0]))).fill(null), currentRoot: null, source: 'evm' };
        }
      } catch (e2) { /* nothing more to try */ }
    }
  }
  db.emit();
  return rec;
}
async function refreshFacade(lineId) {
  const rec = db.upsertLine(lineId, {});
  if (!db.armed) return rec;
  try {
    const w = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.facadeOf, ...ABI.idWords(lineId))));
    const a = w.length ? ABI.addr(w[0]) : null;
    if (a && bi(a) !== 0n) { rec.facade = a; rec.facadeSource = 'registry'; }
    const s = ABI.str(await evm.call(rec.facade, ABI.call(SIG.symbol)));
    if (s) rec.symbol = s;
  } catch (e) { /* keep the derived address */ }
  return rec;
}
async function refreshVersion(lineId, n) {
  try {
    const r = await rpc('getPalwModelVersion', { lineId, version: n });
    if (r && r.exists && r.version) return Object.assign({ evaluations: r.evaluations || [] }, r.version);
    return null;
  } catch (e) {
    if (!db.armed) throw e;
    const w = ABI.words(await evm.call(ADDR.REGISTRY, ABI.call(SIG.usage, ...ABI.idWords(lineId), ABI.word(n))));
    if (w.length < 5) return null;
    return { lineId, version: n, attemptClaims: ABI.u(w[0]), fpClaims: ABI.u(w[1]), workLeaves: ABI.u(w[2]).toString(), firstUsedDaa: ABI.u(w[3]) || null, lastUsedDaa: ABI.u(w[4]) || null, evaluations: [], source: 'evm' };
  }
}
// the current version's usage counters, the one measurement the chain makes
async function refreshUsage(lineId) {
  const rec = db.upsertLine(lineId, {});
  const cur = rec.row && rec.row.current ? Number(rec.row.current) : 1;
  try {
    const v = await refreshVersion(lineId, cur);
    rec.usage = v ? { version: cur, attempt: bi(v.attemptClaims), fp: bi(v.fpClaims), workLeaves: bi(v.workLeaves), lastUsedDaa: v.lastUsedDaa, evaluations: (v.evaluations || []).length } : { version: cur, attempt: 0n, fp: 0n, workLeaves: 0n, evaluations: 0, none: true };
  } catch (e) { rec.usage = null; }
  return rec;
}
async function refreshVersions(lineId) {
  const rec = db.upsertLine(lineId, {});
  const published = rec.row ? Number(rec.row.versionsPublished || 0) : 0;
  const top = Math.max(published, rec.row && rec.row.current ? Number(rec.row.current) : 1);
  const from = Math.max(1, top - 63);
  const nums = []; for (let n = top; n >= from; n--) nums.push(n);
  const rows = await pool(nums, 4, (n) => refreshVersion(lineId, n));
  rec.versions = rows.filter(Boolean);
  db.emit();
  return rec;
}
async function refreshProposals(lineId) {
  const rec = db.upsertLine(lineId, {});
  try { const r = await rpc('getPalwModelProposals', { lineId }); rec.proposals = r && r.exists ? (r.proposals || []) : []; } catch (e) { rec.proposals = null; }
  return rec;
}
async function holderIdOf(address) {
  if (db.armed) {
    try { const w = ABI.words(await evm.call(ADDR.POSITION, ABI.call(SIG.holderIdOf, ABI.addrWord(address)))); if (w.length >= 2) return w[0] + w[1]; } catch (e) { /* derive */ }
  }
  return holderIdDerived(CHAIN_ID_BI, address);
}
// the account's EVM-namespace positions: wRPC by holder id, else the position window per known line
async function positionsOf(address) {
  const holder = await holderIdOf(address);
  try {
    const r = await rpc('getPalwModelPositions', { holder });
    return { holder, source: 'wrpc', positions: (r.positions || []).map((p) => ({ lineId: normId(p.lineId), units: bi(p.units) })).filter((p) => p.lineId && p.units > 0n) };
  } catch (e) {
    if (!db.armed) throw e;
    const out = [];
    for (const id of db.lines.keys()) {
      const w = ABI.words(await evm.call(ADDR.POSITION, ABI.call(SIG.balanceOfAddress, ...ABI.idWords(id), ABI.addrWord(address))));
      const units = w.length ? ABI.u(w[0]) : 0n;
      if (units > 0n) out.push({ lineId: id, units });
    }
    return { holder, source: 'evm', positions: out };
  }
}
async function positionOf(lineId, address) {
  if (db.armed) {
    try { const w = ABI.words(await evm.call(ADDR.POSITION, ABI.call(SIG.balanceOfAddress, ...ABI.idWords(lineId), ABI.addrWord(address)))); return w.length ? ABI.u(w[0]) : 0n; } catch (e) { /* fall through */ }
  }
  const r = await positionsOf(address);
  const p = r.positions.find((x) => x.lineId === lineId);
  return p ? p.units : 0n;
}
async function balanceOf(address) { const r = await evm.rpc('eth_getBalance', [address, 'latest']); return bi(r); }
async function nodeQuoteBuy(lineId, sompi) {
  const w = ABI.words(await evm.call(ADDR.AMM, ABI.call(SIG.quoteBuy, ...ABI.idWords(lineId), ABI.word(sompi))));
  if (w.length < 5) return null;
  return { unitsOut: ABI.u(w[0]), fees: { gross: bi(sompi), burn: ABI.u(w[1]), leg: ABI.u(w[2]), net: ABI.u(w[3]) }, priceAfter: ABI.u(w[4]), source: 'node' };
}
async function nodeQuoteSell(lineId, units) {
  const w = ABI.words(await evm.call(ADDR.AMM, ABI.call(SIG.quoteSell, ...ABI.idWords(lineId), ABI.word(units))));
  if (w.length < 5) return null;
  return { fees: { gross: ABI.u(w[0]), burn: ABI.u(w[1]), leg: ABI.u(w[2]), net: ABI.u(w[3]) }, priceAfter: ABI.u(w[4]), source: 'node' };
}
function decodeSettlement(log) {
  const T = topics(); const t0 = (log.topics && log.topics[0] || '').toLowerCase();
  const d = ABI.words(log.data || '0x');
  const base = { blockNumber: Number(bi(log.blockNumber)), logIndex: Number(bi(log.logIndex)), txHash: log.transactionHash, address: (log.address || '').toLowerCase(), holder: log.topics && log.topics[1] ? ABI.addr(log.topics[1].replace(/^0x/, '')) : null };
  if (t0 === T.bought) return Object.assign(base, { kind: 'Bought', mskIn: ABI.u(d[0]), units: ABI.u(d[1]), priceAfter: ABI.u(d[2]) });
  if (t0 === T.sold) return Object.assign(base, { kind: 'Sold', units: ABI.u(d[0]), mskOut: ABI.u(d[1]), priceAfter: ABI.u(d[2]) });
  if (t0 === T.seeded) return Object.assign(base, { kind: 'Seeded', mskIn: ABI.u(d[0]), priceAfter: ABI.u(d[1]) });
  if (t0 === T.refused) return Object.assign(base, { kind: 'Refused', actionId: Number(ABI.u(d[0])), amount: ABI.u(d[1]), reason: Number(ABI.u(d[2])) });
  return null;
}
// facade events over the lookback window (Bought / Sold / Seeded / Refused), optionally for one holder
async function settlementLogs(lineId, holder) {
  const rec = db.upsertLine(lineId, {});
  const head = bi(await evm.rpc('eth_blockNumber', []));
  const lookback = bmin(bi(CFG.LOG_LOOKBACK_BLOCKS || 5000), 9999n);
  const from = head > lookback ? head - lookback : 0n;
  const T = topics();
  const filter = { address: rec.facade, fromBlock: toHex(from), toBlock: 'latest', topics: [[T.bought, T.sold, T.seeded, T.refused]] };
  if (holder) filter.topics.push('0x' + ABI.addrWord(holder));
  const logs = await evm.rpc('eth_getLogs', [filter]);
  const out = (logs || []).map(decodeSettlement).filter(Boolean).map((s) => Object.assign(s, { lineId }));
  out.sort((a, b) => b.blockNumber - a.blockNumber || b.logIndex - a.logIndex);
  return out;
}
const blockTimeCache = new Map();
async function blockTimestamp(n) {
  if (blockTimeCache.has(n)) return blockTimeCache.get(n);
  const b = await evm.rpc('eth_getBlockByNumber', [toHex(n), false]);
  const ts = b && b.timestamp ? Number(bi(b.timestamp)) * 1000 : null;
  blockTimeCache.set(n, ts);
  return ts;
}
// fold settlement prices into the sampled history (what the chain exposes about the past)
async function foldEventsIntoHistory(lineId, events) {
  const seen = new Set(store.get('hist:events:' + lineId, []));
  const fresh = events.filter((e) => e.kind !== 'Refused' && !seen.has(e.blockNumber + ':' + e.logIndex)).slice(0, 40);
  for (const e of fresh) {
    try { const ts = await blockTimestamp(e.blockNumber); if (ts) history.add(lineId, e.priceAfter, ts, 'event'); seen.add(e.blockNumber + ':' + e.logIndex); } catch (err) { break; }
  }
  store.set('hist:events:' + lineId, Array.from(seen).slice(-500));
}

// pending transactions: receipt (queued / reverted), then the settlement event at the next block
async function pollTransactions() {
  const open = txlog.list.filter((t) => t.status === 'sent' || t.status === 'queued');
  if (!open.length) return;
  for (const t of open) {
    try {
      if (t.status === 'sent') {
        const r = await evm.rpc('eth_getTransactionReceipt', [t.hash]);
        if (!r) { if (Date.now() - t.sentAt > 30 * 60000) txlog.update(t.hash, { status: 'lost' }); continue; }
        const ok = bi(r.status) === 1n;
        txlog.update(t.hash, { status: ok ? 'queued' : 'reverted', blockNumber: Number(bi(r.blockNumber)), gasUsed: bi(r.gasUsed).toString() });
        toast(ok ? 'Action queued in block ' + Number(bi(r.blockNumber)) + '. The fold applies it after this block and settles it one block later.' : 'Transaction reverted at the call (no action was queued).', ok ? 'ok' : 'bad', t.label);
        if (!ok) continue;
      }
      if (t.status === 'queued' || t.status === 'sent') {
        const rec = txlog.list.find((x) => x.hash === t.hash);
        if (rec.status !== 'queued') continue;
        const events = await settlementLogs(rec.lineId, rec.from);
        const want = rec.kind === 'buy' ? 'Bought' : rec.kind === 'seed' ? 'Seeded' : 'Sold';
        const actionId = rec.kind === 'buy' ? 1 : rec.kind === 'seed' ? ACTION_SEED : 2;
        const hit = events.filter((e) => e.blockNumber > rec.blockNumber && !txlog.claimed.has(e.blockNumber + ':' + e.logIndex) && (e.kind === want || (e.kind === 'Refused' && e.actionId === actionId))).sort((a, b) => a.blockNumber - b.blockNumber || a.logIndex - b.logIndex)[0];
        if (!hit) continue;
        txlog.claimed.add(hit.blockNumber + ':' + hit.logIndex);
        if (hit.kind === 'Refused') { txlog.update(rec.hash, { status: 'refused', settledBlock: hit.blockNumber, reason: hit.reason }); toast('Refused by the fold: ' + (REFUSAL[hit.reason] || 'code ' + hit.reason) + '. ' + (rec.kind === 'sell' ? 'Your positions never left.' : 'The escrow was refunded.'), 'warn', rec.label); }
        else if (hit.kind === 'Seeded') { txlog.update(rec.hash, { status: 'settled', settledBlock: hit.blockNumber, units: '0', msk: hit.mskIn.toString(), priceAfter: hit.priceAfter.toString() }); toast('Seeded: ' + fmtMsk(hit.mskIn) + ' MSK locked into the curve for good; first price ' + fmtPrice(hit.priceAfter) + ' MSK per position (settled in block ' + hit.blockNumber + ').', 'ok', rec.label); refreshMarket(rec.lineId).catch(() => {}); }
        else { txlog.update(rec.hash, { status: 'settled', settledBlock: hit.blockNumber, units: (hit.units || 0n).toString(), msk: (hit.kind === 'Bought' ? hit.mskIn : hit.mskOut).toString(), priceAfter: hit.priceAfter.toString() }); toast((hit.kind === 'Bought' ? 'Bought ' + fmtPos(hit.units) + ' positions for ' + fmtMsk(hit.mskIn) + ' MSK' : 'Sold ' + fmtPos(hit.units) + ' positions for ' + fmtMsk(hit.mskOut) + ' MSK net') + ' (settled in block ' + hit.blockNumber + ').', 'ok', rec.label); }
        db.emit();
      }
    } catch (e) { /* try again next tick */ }
  }
}

// ============================================================================================
// 8. UI plumbing: toasts, nav, status pill, banner, router, page lifecycle
// ============================================================================================
const MAX_TOASTS = 4;
function toast(msg, kind, title) {
  const box = $('#toasts'); if (!box) return;
  while (box.children.length >= MAX_TOASTS) box.firstChild.remove();
  const el = document.createElement('div');
  el.className = 'toast ' + (kind === 'bad' ? 'bad' : kind === 'warn' ? 'warn' : '');
  el.innerHTML = (title ? h`<div class="t">${title}</div>` : '') + h`<div>${msg}</div>`;
  box.appendChild(el);
  setTimeout(() => { if (el.parentNode) el.remove(); }, kind === 'bad' ? 12000 : 8000);
}
function copyText(text) {
  try { navigator.clipboard.writeText(text).then(() => toast('Copied to clipboard.', 'ok')); } catch (e) { toast('Copy failed.', 'warn'); }
}
function idCell(id, n) {
  if (!id) return h`<span class="dim">—</span>`;
  return h`<span class="mono" title="${id}">${shortId(id, n)}</span><button class="copy" data-copy="${id}" title="Copy" aria-label="Copy id">⧉</button>`;
}
function explorerLink(kind, id) {
  if (!CFG.EXPLORER_URL) return null;
  return CFG.EXPLORER_URL.replace(/\/$/, '') + (kind === 'block' ? '/blocks/' : kind === 'tx' ? '/txs/' : '/') + id;
}

const pageState = { timers: [], cleanups: [], name: '', arg: '' };
// a page's poll; a failure is logged (never swallowed: a silent poll that stopped refreshing looks like a stale chain)
function every(ms, fn) { const t = setInterval(() => { try { const p = fn(); if (p && p.catch) p.catch((e) => console.error('[poll]', e)); } catch (e) { console.error('[poll]', e); } }, ms); pageState.timers.push(t); return t; }
function onCleanup(fn) { pageState.cleanups.push(fn); }
function clearPage() { for (const t of pageState.timers) clearInterval(t); for (const fn of pageState.cleanups) { try { fn(); } catch (e) { /* ignore */ } } pageState.timers = []; pageState.cleanups = []; }

function parseHash() {
  const raw = location.hash.replace(/^#\/?/, '');
  const [name, ...rest] = raw.split('/');
  return { name: name || 'trade', arg: rest.join('/') || '' };
}
function navigate(hash) { location.hash = hash; }

function renderNav() {
  const { name } = parseHash();
  for (const a of $$('#navLinks a[data-page]')) a.classList.toggle('active', a.dataset.page === name || (name === 'line' && a.dataset.page === 'lines'));
  const btn = $('#connectBtn');
  if (wallet.account) { btn.textContent = shortAddr(wallet.account) + (wallet.onChain() ? '' : ' (wrong chain)'); btn.className = 'btn' + (wallet.onChain() ? '' : ' btn-sell'); btn.title = wallet.account; }
  else { btn.textContent = 'Connect'; btn.className = 'btn btn-accent'; btn.title = 'Connect an EIP-1193 wallet'; }
  const dot = $('#netDot'), txt = $('#netText'), pill = $('#netPill');
  const daa = db.chain.daa != null ? fmtInt(db.chain.daa) : null;
  const market = db.armed === true ? 'market armed' : db.armed === false ? 'market dormant' : 'market: unknown';
  const wr = status.wrpc === 'up' ? 'up' : (status.wrpc === 'connecting' || status.wrpc === 'idle') ? 'connecting' : 'down';
  txt.textContent = (db.chain.network || CFG.NETWORK_NAME) + (daa ? ' · DAA ' + daa : '') + ' · ' + market;
  dot.className = 'dot ' + (wr === 'up' && db.armed ? 'ok' : wr === 'up' || status.evm === 'up' ? 'warn' : 'bad');
  pill.title = 'wRPC ' + (wrpc.url || '(not configured)') + ': ' + status.wrpc + (status.wrpcErr ? ' (' + status.wrpcErr + ')' : '') + '\nEVM RPC ' + (evm.url || '(not configured)') + ': ' + status.evm + (status.evmErr ? ' (' + status.evmErr + ')' : '') + (db.evmDaa != null ? '\nfold clock (chainDaa): ' + fmtInt(db.evmDaa) : '');
  $('#footStatus').textContent = 'wRPC: ' + status.wrpc + (status.wrpcErr ? ' (' + status.wrpcErr + ')' : '') + '. EVM RPC: ' + status.evm + (status.evmErr ? ' (' + status.evmErr + ')' : '') + '.' + (MOCK ? ' MOCK MODE: every number on this page is produced by mock.js, not by a node.' : '');
}
function renderBanner() {
  const b = $('#banner');
  let text = '', cls = 'banner';
  const anySeeded = Array.from(db.lines.values()).some((r) => r.market && r.market.seeded);
  const anyLegacy = Array.from(db.lines.values()).some((r) => r.market && r.market.legacy);
  if (MOCK) { text = 'Mock mode: the wRPC, the EVM RPC and the wallet are simulated by mock.js. Nothing here is a chain fact.'; cls += ' info'; }
  else if (db.armed === false) text = 'Market not armed on ' + CFG.NETWORK_NAME + ' yet: the palw_model_market / palw_model_lines / palw_model_evm fences are dormant, so every line reads as an unseeded market (no reserve, no price) and neither a seed nor a trade can be sent: the facades are empty accounts and a Seed or Buy would be refused. The layout below is live against the node.' + (anyLegacy ? ' The node also serves the market row in its pre-ADR-0090 shape (a virtual reserve, no least seed), so position counts are not shown.' : '');
  else if (db.armed === null && status.evm === 'down' && status.wrpc === 'down') { text = 'No node reachable: neither the wRPC (' + (wrpc.url || 'not configured') + ') nor the EVM RPC (' + (evm.url || 'not configured') + ') answered. Showing the layout with no data.'; cls += ' bad'; }
  else if (db.armed === null && status.evm === 'down') { text = 'EVM RPC unreachable: the market fence state cannot be read and neither seeds nor trades can be sent. Chain reads still come from the wRPC.' + (anySeeded ? '' : ' No market has been seeded on this network yet.'); cls += ' info'; }
  else if (status.wrpc === 'down' && db.armed !== null) { text = 'wRPC unreachable: line names, versions and proposals cannot be read; markets and positions are read through the EVM window instead.'; cls += ' info'; }
  b.className = cls; b.textContent = text; b.hidden = !text;
}
function bindNav() {
  $('#connectBtn').addEventListener('click', async () => {
    if (wallet.account) { if (confirm('Disconnect ' + shortAddr(wallet.account) + ' from this site? (The wallet itself stays connected.)')) wallet.disconnect(); return; }
    try { await wallet.connect(); toast('Connected ' + shortAddr(wallet.account) + (wallet.onChain() ? ' on MISAKA.' : '.'), 'ok'); }
    catch (e) { toast(e.message || String(e), 'bad', 'Wallet'); }
  });
  $('#navToggle').addEventListener('click', () => { const l = $('#navLinks'); l.classList.toggle('open'); $('#navToggle').setAttribute('aria-expanded', l.classList.contains('open')); });
  $('#moreBtn').addEventListener('click', (ev) => { ev.stopPropagation(); const m = $('#moreMenu'); m.hidden = !m.hidden; $('#moreBtn').setAttribute('aria-expanded', !m.hidden); });
  document.addEventListener('click', (ev) => {
    const m = $('#moreMenu'); if (m && !m.hidden && !m.contains(ev.target)) m.hidden = true;
    const c = ev.target.closest('[data-copy]'); if (c) { ev.preventDefault(); copyText(c.dataset.copy); }
    const row = ev.target.closest('tr[data-href]'); if (row && !ev.target.closest('a, button')) navigate(row.dataset.href);
  });
  $('#lnkExplorer').href = CFG.EXPLORER_URL || '#';
  $('#lnkAdr').href = CFG.ADR_URL || '#';
  $$('#navLinks a').forEach((a) => a.addEventListener('click', () => $('#navLinks').classList.remove('open')));
}

// ============================================================================================
// 9. the chart: a canvas line of sampled curve prices with crosshair and tooltip
// ============================================================================================
class PriceChart {
  constructor(box) {
    this.box = box; this.canvas = document.createElement('canvas'); box.appendChild(this.canvas);
    this.empty = document.createElement('div'); this.empty.className = 'chart-empty'; box.appendChild(this.empty);
    this.tip = document.createElement('div'); this.tip.className = 'chart-tip'; this.tip.hidden = true; box.appendChild(this.tip);
    this.points = []; this.rangeMs = 86400000; this.hover = null; this.layout = null; this.hint = null;
    this.onResize = () => this.draw();
    window.addEventListener('resize', this.onResize);
    this.canvas.addEventListener('mousemove', (e) => { const r = this.canvas.getBoundingClientRect(); this.hover = { x: e.clientX - r.left, y: e.clientY - r.top }; this.draw(); });
    this.canvas.addEventListener('mouseleave', () => { this.hover = null; this.draw(); });
  }
  destroy() { window.removeEventListener('resize', this.onResize); }
  setPoints(points) { this.points = points; this.draw(); }
  setRange(ms) { this.rangeMs = ms; this.draw(); }
  visible() { const now = Date.now(); return this.rangeMs ? this.points.filter((p) => p.t >= now - this.rangeMs) : this.points; }
  draw() {
    const dpr = window.devicePixelRatio || 1;
    const W = this.box.clientWidth, H = this.box.clientHeight;
    if (!W || !H) return;
    this.canvas.width = Math.round(W * dpr); this.canvas.height = Math.round(H * dpr);
    const ctx = this.canvas.getContext('2d'); ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    const pts = this.visible();
    if (pts.length < 2) {
      this.empty.hidden = false;
      this.empty.textContent = this.hint || (this.points.length ? 'Only ' + this.points.length + ' price sample' + (this.points.length === 1 ? '' : 's') + ' in this range. The chart fills in as the curve is sampled (every ' + Math.round(CFG.POLL_MS / 1000) + ' s while this page is open).' : 'No price history yet. Samples are taken from the node while this page is open and kept in this browser.');
      this.tip.hidden = true; return;
    }
    this.empty.hidden = true;
    const padL = 10, padR = 84, padT = 14, padB = 24;
    const x0 = padL, x1 = W - padR, y0 = padT, y1 = H - padB;
    const tMin = pts[0].t, tMax = pts[pts.length - 1].t || tMin + 1;
    const prices = pts.map((p) => Number(p.p) / 1e8);
    let pMin = Math.min(...prices), pMax = Math.max(...prices);
    if (pMax === pMin) { pMin *= 0.99; pMax *= 1.01; }
    const padP = (pMax - pMin) * 0.08; pMin -= padP; pMax += padP;
    const X = (t) => x0 + ((t - tMin) / Math.max(1, tMax - tMin)) * (x1 - x0);
    const Y = (p) => y1 - ((p - pMin) / (pMax - pMin)) * (y1 - y0);
    this.layout = { X, Y, x0, x1, y0, y1, pts, prices, tMin, tMax };
    // grid + axes (recessive)
    ctx.font = '11px ' + getComputedStyle(document.body).getPropertyValue('--mono');
    ctx.fillStyle = '#7a908c'; ctx.strokeStyle = 'rgba(255,255,255,0.06)'; ctx.lineWidth = 1;
    const dp = pMax - pMin < 0.001 ? 8 : pMax - pMin < 0.1 ? 6 : 4;
    for (let i = 0; i <= 4; i++) {
      const p = pMin + ((pMax - pMin) * i) / 4, y = Math.round(Y(p)) + 0.5;
      ctx.beginPath(); ctx.moveTo(x0, y); ctx.lineTo(x1, y); ctx.stroke();
      ctx.textAlign = 'left'; ctx.fillText(p.toFixed(dp), x1 + 6, y + 4);
    }
    const ticks = Math.max(2, Math.min(6, Math.floor((x1 - x0) / 110)));
    const span = tMax - tMin;
    for (let i = 0; i <= ticks; i++) {
      const t = tMin + (span * i) / ticks, x = Math.round(X(t)) + 0.5;
      ctx.beginPath(); ctx.moveTo(x, y0); ctx.lineTo(x, y1); ctx.stroke();
      ctx.textAlign = i === 0 ? 'left' : i === ticks ? 'right' : 'center';
      ctx.fillText(span > 2 * 86400000 ? fmtDateTime(t).slice(5, 16) : fmtTime(t).slice(0, 5), x, H - 8);
    }
    // area + line (2px, the accent)
    const grad = ctx.createLinearGradient(0, y0, 0, y1); grad.addColorStop(0, 'rgba(80,210,193,0.22)'); grad.addColorStop(1, 'rgba(80,210,193,0.0)');
    ctx.beginPath();
    pts.forEach((p, i) => { const x = X(p.t), y = Y(prices[i]); if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); });
    const lastX = X(pts[pts.length - 1].t);
    ctx.lineTo(lastX, y1); ctx.lineTo(X(pts[0].t), y1); ctx.closePath(); ctx.fillStyle = grad; ctx.fill();
    ctx.beginPath(); ctx.strokeStyle = '#50d2c1'; ctx.lineWidth = 2; ctx.lineJoin = 'round';
    pts.forEach((p, i) => { const x = X(p.t), y = Y(prices[i]); if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); });
    ctx.stroke();
    // last price label
    const lp = prices[prices.length - 1], ly = Y(lp);
    ctx.fillStyle = '#50d2c1'; ctx.beginPath(); ctx.arc(lastX, ly, 3.5, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = '#50d2c1'; ctx.fillRect(x1 + 2, ly - 9, padR - 4, 18);
    ctx.fillStyle = '#052e29'; ctx.textAlign = 'left'; ctx.fillText(lp.toFixed(dp), x1 + 6, ly + 4);
    // hover crosshair + tooltip
    if (this.hover && this.hover.x >= x0 && this.hover.x <= x1) {
      let best = 0, bd = Infinity;
      pts.forEach((p, i) => { const d = Math.abs(X(p.t) - this.hover.x); if (d < bd) { bd = d; best = i; } });
      const hx = X(pts[best].t), hy = Y(prices[best]);
      ctx.strokeStyle = 'rgba(233,244,242,0.35)'; ctx.lineWidth = 1; ctx.setLineDash([3, 3]);
      ctx.beginPath(); ctx.moveTo(hx + 0.5, y0); ctx.lineTo(hx + 0.5, y1); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(x0, hy + 0.5); ctx.lineTo(x1, hy + 0.5); ctx.stroke(); ctx.setLineDash([]);
      ctx.fillStyle = '#e9f4f2'; ctx.beginPath(); ctx.arc(hx, hy, 4, 0, Math.PI * 2); ctx.fill();
      this.tip.hidden = false; this.tip.style.left = Math.min(Math.max(hx, 70), W - 70) + 'px'; this.tip.style.top = Math.max(hy, 30) + 'px';
      this.tip.textContent = fmtDateTime(pts[best].t) + ':' + fmtTime(pts[best].t).slice(5) + '  ' + fmtPrice(pts[best].p) + ' MSK';
    } else this.tip.hidden = true;
  }
}

// ============================================================================================
// 9b. the seed panel (ADR-0090): a market opens only by a seed, locked for good
// ============================================================================================
function seedMinFor(m) { return (m && m.seedMinSompi) || (db.constsFromChain && db.constsFromChain.seedMinSompi) || CURVE_DEFAULTS.seedMinSompi; }
function supplyFor(m) { return (m && m.supplyUnits) || (db.constsFromChain && db.constsFromChain.supplyUnits) || CURVE_DEFAULTS.supplyUnits; }
// what a seed of `sompi` makes: the row after, and its first price (seed / supply)
function seedPreview(sompi, m) {
  const c = curve.consts(m || {}); c.supplyUnits = supplyFor(m); c.seedMinSompi = seedMinFor(m);
  const row = curve.seed(sompi, c);
  return { row, price: curve.price(row, c), supply: c.supplyUnits, min: c.seedMinSompi };
}
function classStatusOf(rec) {
  const s = rec && rec.market && rec.market.classStatus ? parseClassStatus(rec.market.classStatus) : null;
  if (s) return s;
  const c = rec && rec.classId && db.classes.get(rec.classId);
  return c && c.statusLabel ? { head: c.statusLabel, activationDaa: null, sinceDaa: null, raw: 'registry classRow' } : null;
}
function seedReasonNotToSend(rec, sompi, balance) {
  const m = rec && rec.market;
  if (MOCK && !wallet.provider) return 'Mock wallet missing.';
  if (!wallet.provider) return 'No EIP-1193 wallet found: install MetaMask and reload.';
  if (!wallet.account) return 'Connect a wallet to seed.';
  if (!wallet.onChain()) return 'Switch the wallet to the MISAKA chain (' + CFG.CHAIN_ID + ').';
  if (db.armed === false) return 'The market is not armed on this network: the facade is an empty account, so a seed would be refused.';
  if (db.armed === null) return 'EVM RPC unreachable: the transaction cannot be built.';
  if (!rec) return 'Enter a line id.';
  if (rec.notFound) return 'The node reports no line with this id.';
  if (!m) return 'The market row has not been read yet.';
  if (m.seeded) return 'This line is already seeded (one seed a line).';
  const cs = classStatusOf(rec);
  if (cs && cs.head === 'Frozen') return 'The class is frozen: a frozen class takes no seed.';
  if (cs && cs.head === 'Dormant') return 'The class is dormant: it must be registered again before it takes a seed.';
  if (rec.facadeSource !== 'registry') return 'Facade address not confirmed by the registry window yet.';
  if (sompi == null) return 'Enter the seed in MSK (whole sompi, at most 8 decimals).';
  if (sompi < seedMinFor(m)) return 'Under the least seed of ' + fmtMsk(seedMinFor(m), 8) + ' MSK: the writer reverts SeedTooSmall at the call.';
  if (balance != null && sompiToWei(sompi) > balance) return 'Insufficient MSK balance in the EVM account.';
  return null;
}
// Renders the panel into `box`. `st` = { msk, busy, balance }; `getRec` returns the line record
// (may be null); `onSent(hash)` runs after the wallet accepted the transaction.
function renderSeedPanel(box, st, getRec, onSent) {
  const rec = getRec(); const m = rec && rec.market;
  const min = seedMinFor(m);
  if (st.msk == null) st.msk = fmtScaled(min, 8, 8).replace(/,/g, '');
  const sompi = parseDec(st.msk, 8);
  const pv = seedPreview(sompi != null && sompi > 0n ? sompi : min, m);
  const cs = classStatusOf(rec);
  const reason = seedReasonNotToSend(rec, sompi, st.balance);
  const label = rec ? db.label(rec) : 'this line';
  box.innerHTML = h`
    <div class="panel-b seedp">
      <div class="seed-h"><span class="tag warn">Not seeded</span> Seed this market</div>
      <p class="small">No curve exists for <b>${label}</b> yet. A market opens only when someone locks at least <b>${fmtMsk(min, 0)} MSK</b> into the line:</p>
      <ul class="small seed-facts">
        <li>the whole seed becomes the reserve, fee-free, and is <b>locked for good</b>: no object pays a seed out, and the curve's product never falls, so a sell can never drain the reserve under the seed;</li>
        <li>the seeder gets <b>no position</b> and nothing back, ever; exposure is bought like anyone else's;</li>
        <li><b>${fmtPos(pv.supply)}</b> whole positions open in the curve at a first price of <b>seed / ${fmtPos(pv.supply)}</b>;</li>
        <li>one seed a line (a second is refused); a frozen class takes none; a class still waiting for its activation takes the seed, and its buys wait for Active.</li>
      </ul>
      <div class="field">
        <label for="seedAmt">Seed (MSK, locked for good)</label>
        <div class="inp"><input id="seedAmt" inputmode="decimal" autocomplete="off" value="${st.msk}" aria-describedby="seedQuote"><span class="unit">MSK</span></div>
      </div>
      <div class="quote" id="seedQuote">
        <div class="r"><span>Least seed</span><span class="v">${fmtMsk(min, 8)} MSK</span></div>
        <div class="r"><span>First price</span><span class="v">${pv.price != null ? fmtPrice(pv.price) : '—'} MSK</span></div>
        <div class="r"><span>Positions in the curve</span><span class="v">${fmtPos(pv.supply)}</span></div>
        <div class="r"><span>Positions you receive</span><span class="v">0</span></div>
        <div class="r"><span>Fee on the seed</span><span class="v">none</span></div>
        <div class="r"><span>Class status</span><span class="v">${cs ? cs.head + (cs.activationDaa != null ? ' (activates at DAA ' + fmtInt(cs.activationDaa) + ')' : '') : '—'}</span></div>
        <div class="r tot"><span>Available (EVM)</span><span class="v" id="availV">${st.balance != null ? fmtWeiMsk(st.balance) + ' MSK' : wallet.account ? '—' : 'connect a wallet'}</span></div>
      </div>
      <div class="field"><button id="seedBtn" class="btn btn-lg btn-accent" ${reason || st.busy ? 'disabled' : ''}>${st.busy ? 'Confirm in wallet…' : 'Seed ' + (rec ? rec.symbol : '')}</button><div class="reason" id="seedWhy">${reason || ''}</div></div>
      <div class="note tiny">Sent as <code>seed()</code> on the line's facade with value = seed × 10<sup>10</sup> wei. The call queues the action (<code>ActionQueued</code>) in the block that carries it; the fold makes the seed the reserve after that block, and the facade emits <code>Seeded</code> (or <code>Refused</code>: already seeded, class closed) one block later, when the escrow is burned into the line's sink. A value under the least seed reverts at the call (<code>SeedTooSmall</code>).</div>
    </div>`.s;
  const amt = $('#seedAmt', box);
  amt.addEventListener('input', () => { st.msk = amt.value; const pos = amt.selectionStart; renderSeedPanel(box, st, getRec, onSent); const a2 = $('#seedAmt', box); a2.focus(); try { a2.setSelectionRange(pos, pos); } catch (e) { /* ignore */ } });
  $('#seedBtn', box).addEventListener('click', async () => {
    const r = getRec(); const s = parseDec(st.msk, 8);
    const why = seedReasonNotToSend(r, s, st.balance); if (why) { toast(why, 'warn'); return; }
    st.busy = true; renderSeedPanel(box, st, getRec, onSent);
    try {
      const hash = await wallet.sendTx({ from: wallet.account, to: r.facade, value: toHex(sompiToWei(s)), data: ABI.call(SIG.seed) });
      txlog.add({ hash, from: wallet.account, lineId: r.lineId, label: r.symbol, kind: 'seed', amount: s.toString(), min: '0', sentAt: Date.now(), status: 'sent' });
      toast('Seed sent: ' + shortId(hash, 10) + '. Queued at the next block; the market opens one block after that.', 'ok', 'Seed ' + r.symbol);
      if (onSent) onSent(hash);
    } catch (e) { toast((e && e.message) || String(e), 'bad', 'Wallet'); }
    st.busy = false; renderSeedPanel(box, st, getRec, onSent);
  });
}

// ============================================================================================
// 10. the trade page
// ============================================================================================
const DEPTH_SIZES = [1n, 10n, 100n, 1000n, 10000n];
const RANGES = [['1h', 3600000], ['6h', 6 * 3600000], ['24h', 86400000], ['7d', 7 * 86400000], ['All', 0]];

function sortedLines() {
  return Array.from(db.lines.values()).sort((a, b) => {
    const ra = a.market ? Number(a.market.mskReserve) : -1, rb = b.market ? Number(b.market.mskReserve) : -1;
    if (rb !== ra) return rb - ra;
    return db.label(a).localeCompare(db.label(b));
  });
}
function firstLineId() { const l = sortedLines(); return l.length ? l[0].lineId : null; }
function ownerLabel(row) {
  if (!row) return null;
  if (row.ownerPayoutPayload) return { text: shortId(row.ownerPayoutPayload), title: row.ownerPayoutPayload };
  if (row.owner && row.owner.transactionId) return { text: shortId(row.owner.transactionId) + ':' + row.owner.index, title: 'owner bond ' + row.owner.transactionId + ':' + row.owner.index };
  if (row.hasRow === false || row.source === 'evm' || row.lineId) return { text: 'unowned (genesis)', title: 'A genesis class has no registrant bond; its owner leg is burned.' };
  return null;
}
function statusTag(rec) {
  const m = rec.market, row = rec.row;
  const retired = row && /retired/i.test(row.status || '');
  if (retired) return h`<span class="tag bad">Retired</span>`;
  if (m && !m.seeded) return h`<span class="tag warn" title="No market yet: a seed of at least the least seed opens it">Not seeded</span>`;
  if (m && m.closedToBuys) return h`<span class="tag warn">Closed to buys</span>`;
  if (m) return h`<span class="tag ok">Seeded</span>`;
  return h`<span class="tag">—</span>`;
}
function changeCell(rec) {
  if (!rec.market || rec.market.price == null) return h`<span class="dim">—</span>`;
  const c = history.change24h(rec.lineId, rec.market.price);
  if (!c) return h`<span class="dim" title="Fewer than two price samples in this browser yet">—</span>`;
  const cls = c.pct > 0 ? 'up' : c.pct < 0 ? 'down' : '';
  const title = c.partial ? 'Since ' + fmtDateTime(c.since) + ' (this browser has no older sample)' : 'Against the sample nearest to 24 h ago (' + fmtDateTime(c.since) + ')';
  return h`<span class="${cls}" title="${title}">${fmtPct(c.pct)}${c.partial ? raw('<span class="dim">*</span>') : ''}</span>`;
}

async function pageTrade(arg) {
  const gen = pageState.gen, alive = () => gen === pageState.gen;
  const main = $('#main');
  let lineId = normId(arg);
  if (lineId && !db.lines.has(lineId)) db.upsertLine(lineId, {});
  if (!lineId) lineId = normId(store.get('lastLine')) || firstLineId();
  if (lineId) store.set('lastLine', lineId);
  const rec = lineId ? db.upsertLine(lineId, {}) : null;
  const entry = { side: 'buy', amount: '', slippage: String(store.get('slippage', '2')), quote: null, nodeQuote: null, quoteSeq: 0, busy: false, balance: null, position: null, positionSrc: null };
  const view = { chartTab: 'chart', bottomTab: 'positions', range: store.get('range', 86400000), positions: null, settlements: null, mySettlements: null, versions: null, err: null };

  main.innerHTML = h`
  <section class="trade" aria-label="Trade">
    <div class="panel a-bar mkt-bar" id="mktBar"></div>
    <div class="panel a-chart">
      <div class="tabs" id="chartTabs"><button data-t="chart" class="on">Chart</button><button data-t="versions">Versions</button><button data-t="usage">Usage</button></div>
      <div class="chart-tools" id="chartTools"></div>
      <div class="chart-box" id="chartBox"></div>
      <div class="tab-body" id="chartAlt" hidden></div>
    </div>
    <div class="panel a-depth depth" id="depth"></div>
    <aside class="panel a-entry entry" id="entry" aria-label="Order entry"></aside>
    <div class="panel a-bottom">
      <div class="tabs" id="bottomTabs"><button data-t="positions" class="on">Positions</button><button data-t="history">Order history</button><button data-t="settlements">Settlements</button><button data-t="info">Line info</button></div>
      <div class="tab-body" id="bottomBody"></div>
    </div>
  </section>`.s;

  const chart = new PriceChart($('#chartBox'));
  onCleanup(() => chart.destroy());

  // ---- market bar -------------------------------------------------------------------------
  function renderBar() {
    if (!alive()) return;
    const bar = $('#mktBar'); if (!bar) return;
    const m = rec && rec.market, row = rec && rec.row, info = rec && rec.info;
    const price = m && m.price != null ? fmtPrice(m.price) : '—';
    const owner = ownerLabel(row);
    const held = m ? m.supplyUnits - m.positionUnits : null;
    const stats = [
      ['Price (MSK)', price, m && !m.seeded ? 'Not seeded yet: no reserve, no curve, no price' : 'reserve / positions in the curve, sompi per whole position, from the market row'],
      ['24h change', changeCell(rec || {}), null],
      ['Reserve (MSK)', m ? fmtMsk(m.mskReserve, 2) : '—', 'MSK the curve holds (never a spendable output); never under the seed'],
      ['Seed (locked)', m ? (m.seeded ? (m.seedSompi != null ? fmtMsk(m.seedSompi, 2) : raw('<span class="dim" title="The AMM window does not carry the seed; the wRPC does">—</span>')) : raw('<span class="dim">none</span>')) : '—', m && m.seeded ? 'The MSK the market opened with, locked for good (ADR-0090)' : 'No seed yet: at least ' + fmtMsk(seedMinFor(m), 0) + ' MSK opens the market'],
      ['Seeded by', m && m.seeded && m.seededBy ? raw(idCell(m.seededBy, 8).s) : '—', 'The seeder\'s payout payload, kept for the record; the seeder holds nothing'],
      ['Sold / Supply', m && !m.legacy ? fmtPos(held) + ' / ' + fmtPos(m.supplyUnits) : '—', m && m.legacy ? 'The node serves the pre-ADR-0090 row (units of a millionth of a position); not shown' : m ? 'Whole positions outside the curve now. Ever bought: ' + fmtPos(m.soldUnits) : null],
      ['Bought by mining', m && m.buybackSompi != null ? fmtMsk(m.buybackSompi, 2) + (bi(m.retiredUnits || 0n) > 0n ? ' · ' + fmtPos(m.retiredUnits) + ' retired' : '') : m ? raw('<span class="dim" title="This node is from before ADR-0091 and serves no buyback">—</span>') : '—', 'ADR-0091: 5 % of every block\'s mining reward on this line buys from the pair and stays in it; what the curve gives up is retired, held by the chain for good. Nothing is paid to holders.'],
      ['Owner', owner ? owner.text : '—', owner ? owner.title : null],
      ['Current version', row && row.current ? 'v' + row.current + (row.previews && row.previews.length ? ' +' + row.previews.length + ' preview' : '') : (row ? 'v1' : '—'), row ? row.versionsPublished + ' published' : null],
      ['Roots in force', info && info.rootsInForce ? String(info.rootsInForce.length) : '—', 'Roots an attempt claim may name for this class (ADR-0088 D3)'],
    ];
    bar.innerHTML = h`
      <button class="mkt-select" id="mktSelect" aria-haspopup="listbox" aria-expanded="false">
        <span><span class="mkt-name">${rec ? db.label(rec) : 'No line selected'}${rec && db.named(rec).source === 'catalogue' ? raw('<span class="dim tiny" title="' + esc(CATALOGUE_NOTE) + '"> \u24d8</span>') : ''}</span><br><span class="mkt-sym">${rec && modelSub(rec) ? modelSub(rec) + ' · ' : ''}${rec ? rec.symbol : ''} ${rec ? raw('<span class="dim">· ' + esc(shortId(rec.lineId)) + '</span>') : ''}</span></span>
        <span class="caret">▾</span>
      </button>
      <div class="mkt-stats">${stats.map(([k, v, t]) => h`<div class="stat" title="${t || ''}"><div class="k">${k}</div><div class="v">${v}</div></div>`)}</div>`.s;
    $('#mktSelect').addEventListener('click', toggleMenu);
  }
  function toggleMenu(ev) {
    ev.stopPropagation();
    let menu = $('#mktMenu');
    if (menu) { menu.remove(); return; }
    const lines = sortedLines();
    menu = document.createElement('div'); menu.className = 'mkt-menu'; menu.id = 'mktMenu'; menu.setAttribute('role', 'listbox');
    menu.innerHTML = lines.length ? lines.map((r) => h`<div class="item ${r.lineId === lineId ? 'on' : ''}" role="option" data-id="${r.lineId}"><span class="n">${db.label(r)}</span><span class="p">${r.market && r.market.price != null ? fmtPrice(r.market.price) : r.market && !r.market.seeded ? raw('<span class="tag warn">not seeded</span>') : '—'}</span><span class="s">${r.symbol} · ${shortId(r.lineId)}</span><span class="c">class ${shortId(r.classId || '', 8)}${r.market && r.market.seeded ? ' · reserve ' + fmtMsk(r.market.mskReserve, 2) + ' MSK' : ''}</span></div>`).join('') : '<div class="item"><span class="n dim">No lines known. Configure CLASS_IDS in config.js or reach a node.</span></div>';
    $('#mktSelect').appendChild(menu);
    $('#mktSelect').setAttribute('aria-expanded', 'true');
    menu.addEventListener('click', (e) => { const it = e.target.closest('[data-id]'); if (it) navigate('#/trade/' + it.dataset.id); });
    const close = () => { if (menu.parentNode) menu.remove(); document.removeEventListener('click', close); };
    setTimeout(() => document.addEventListener('click', close), 0);
  }

  // ---- chart panel ----------------------------------------------------------------------------
  function renderChartTools() {
    if (!alive()) return;
    const n = rec ? history.load(rec.lineId).length : 0;
    $('#chartTools').innerHTML = h`<span class="rng">${RANGES.map(([l, ms]) => h`<button data-ms="${ms}" class="${ms === view.range ? 'on' : ''}">${l}</button>`)}</span><span class="legend">${rec ? db.label(rec) : ''} price, MSK per position · ${n} sample${n === 1 ? '' : 's'} in this browser</span>`.s;
    $$('#chartTools button').forEach((b) => b.addEventListener('click', () => { view.range = Number(b.dataset.ms); store.set('range', view.range); chart.setRange(view.range); renderChartTools(); }));
  }
  function renderChart() {
    if (!alive()) return;
    if (rec) { chart.hint = rec.market && !rec.market.seeded ? 'Not seeded yet: there is no price until someone seeds this line with at least ' + fmtMsk(seedMinFor(rec.market), 0) + ' MSK.' : null; chart.setRange(view.range); chart.setPoints(history.points(rec.lineId)); }
    renderChartTools();
  }
  function versionRows(list) {
    if (!list) return h`<div class="empty">Loading versions…</div>`.s;
    if (!list.length) return h`<div class="empty">No version rows are held by the node for this line${rec && rec.row && rec.row.hasRow === false ? ' (a founding line nothing touched has its registration root as version 1)' : ''}.</div>`.s;
    return h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th>v</th><th>Status</th><th>Root</th><th>Published DAA</th><th>Parent</th><th>Adopted</th><th>Attempt claims</th><th>FP claims</th><th>Work leaves</th><th>Evaluations</th></tr></thead><tbody>
      ${list.map((v) => h`<tr><td class="num">${v.version}</td><td class="l">${v.status || '—'}${v.inForce ? raw(' <span class="tag ok">in force</span>') : ''}${v.untilDaa ? raw(' <span class="dim tiny">until ' + esc(fmtInt(v.untilDaa)) + '</span>') : ''}</td><td class="l">${idCell(v.root)}</td><td class="num">${v.publishedDaa != null ? fmtInt(v.publishedDaa) : '—'}</td><td class="num">${v.parent || '—'}</td><td class="l">${v.adoptedFrom ? idCell(v.adoptedFrom) : '—'}</td><td class="num">${fmtInt(v.attemptClaims)}</td><td class="num">${fmtInt(v.fpClaims)}</td><td class="num">${fmtInt(v.workLeaves)}</td><td class="num">${(v.evaluations || []).length ? raw(esc(String(v.evaluations.length)) + ' <span class="tag declared">declared</span>') : '0'}</td></tr>`)}
    </tbody></table></div>`.s;
  }
  function renderChartAlt() {
    if (!alive()) return;
    const alt = $('#chartAlt'), box = $('#chartBox'), tools = $('#chartTools');
    const showChart = view.chartTab === 'chart';
    alt.hidden = showChart; box.hidden = !showChart; tools.hidden = !showChart;
    if (showChart) { chart.draw(); return; }
    if (!rec) { alt.innerHTML = '<div class="empty">No line selected.</div>'; return; }
    if (!rec.versions) { refreshVersions(rec.lineId).then(() => { if (view.chartTab !== 'chart') renderChartAlt(); }).catch(() => {}); }
    if (view.chartTab === 'versions') { alt.innerHTML = h`<div class="note">Every version the node still holds (the last 64 per line). Roots, DAA scores and usage are chain facts; the four declared hashes and every evaluation are declarations the chain records and never reads.</div>`.s + versionRows(rec.versions); return; }
    const u = rec.usage;
    alt.innerHTML = h`<div class="note">Usage is the one measurement the chain makes: accepted claims that named this line's roots, counted by the fold (ADR-0088 D4). It says what was used, never how good it was.</div>
      <div class="grid3">
        <div class="tile"><div class="k">Current version</div><div class="v">${u ? 'v' + u.version : '—'}</div></div>
        <div class="tile"><div class="k">Attempt-lane claims</div><div class="v">${u ? fmtInt(u.attempt) : '—'}</div></div>
        <div class="tile"><div class="k">Free-prompt claims</div><div class="v">${u ? fmtInt(u.fp) : '—'}</div></div>
        <div class="tile"><div class="k">Work leaves</div><div class="v">${u ? fmtInt(u.workLeaves) : '—'}</div></div>
        <div class="tile"><div class="k">Last used (DAA)</div><div class="v">${u && u.lastUsedDaa ? fmtInt(u.lastUsedDaa) : '—'}</div></div>
      </div>
      <div class="section">${rec.versions ? versionRows(rec.versions) : ''}</div>`.s;
  }
  $('#chartTabs').addEventListener('click', (e) => { const b = e.target.closest('button'); if (!b) return; view.chartTab = b.dataset.t; $$('#chartTabs button').forEach((x) => x.classList.toggle('on', x === b)); renderChartAlt(); });

  // ---- depth ----------------------------------------------------------------------------------
  function renderDepth() {
    if (!alive()) return;
    const m = rec && rec.market;
    const c = m ? curve.consts(m) : null;
    const seeded = !!(m && m.seeded);
    const buys = [], sells = [];
    if (seeded) {
      for (const n of DEPTH_SIZES) {
        const r = curve.buyCostForUnits(m, n, c);
        buys.push({ n, cost: r ? r.gross : null, avg: r ? r.gross / n : null, after: r ? r.quote.priceAfter : null });
        const held = m.supplyUnits - m.positionUnits;
        const q = n <= held ? curve.sellQuote(m, n, c) : null;
        sells.push({ n, net: q ? q.fees.net : null, avg: q ? q.fees.net / n : null, after: q ? q.priceAfter : null });
      }
    }
    const maxCost = buys.reduce((a, b) => (b.cost != null && b.cost > a ? b.cost : a), 0n);
    const maxNet = sells.reduce((a, b) => (b.net != null && b.net > a ? b.net : a), 0n);
    const bar = (v, max, cls) => h`<td class="num bar ${cls}"><i style="width:${v != null && max > 0n ? Math.max(2, Math.round(ratio(v, max) * 100)) : 0}%"></i><span>${v != null ? fmtMsk(v, 4) : '—'}</span></td>`;
    const ev = view.settlements;
    const noCurve = m && !seeded ? raw('<tr><td colspan="4" class="empty">Not seeded yet: no reserve, so no quote. At least ' + esc(fmtMsk(seedMinFor(m), 0)) + ' MSK opens the market.</td></tr>') : raw('<tr><td colspan="4" class="empty">—</td></tr>');
    $('#depth').innerHTML = h`
      <div class="panel-h">Curve depth <span class="spacer"></span><span class="dim tiny" title="Every row is the chain's own curve arithmetic (ADR-0087 as amended by ADR-0090) applied to the market row as last read">on the ${m ? (m.source === 'wrpc' ? 'node' : 'EVM window') : 'market'} row</span></div>
      <div class="sub">Buy: cost of N whole positions (gross MSK, fees included)</div>
      <div class="tbl-wrap"><table class="tbl"><thead><tr><th>Size</th><th>Cost</th><th>Avg</th><th>After</th></tr></thead><tbody>
        ${buys.length ? buys.map((b) => h`<tr><td class="num">${fmtInt(b.n)}</td>${bar(b.cost, maxCost, 'buy')}<td class="num">${b.avg != null ? fmtPx(b.avg) : '—'}</td><td class="num">${b.after != null ? fmtPx(b.after) : '—'}</td></tr>`) : noCurve}
      </tbody></table></div>
      <div class="sub">Sell: net MSK returned for N positions</div>
      <div class="tbl-wrap"><table class="tbl"><thead><tr><th>Size</th><th>Returns</th><th>Avg</th><th>After</th></tr></thead><tbody>
        ${sells.length ? sells.map((s) => h`<tr><td class="num">${fmtInt(s.n)}</td>${bar(s.net, maxNet, 'sell')}<td class="num">${s.avg != null ? fmtPx(s.avg) : '—'}</td><td class="num">${s.after != null ? fmtPx(s.after) : '—'}</td></tr>`) : noCurve}
      </tbody></table></div>
      <div class="sub">Recent settlements <span class="dim">(facade events, both doors' EVM side)</span></div>
      <div class="tbl-wrap"><table class="tbl"><thead><tr><th>Block</th><th>Event</th><th>Positions</th><th>MSK</th><th>Price after</th></tr></thead><tbody>
        ${ev === null || ev === undefined ? raw('<tr><td colspan="5" class="empty">' + (db.armed ? 'Loading…' : db.armed === false ? 'The market doors are dormant on this network.' : 'EVM RPC unreachable.') + '</td></tr>') : ev.length ? ev.slice(0, 8).map((e) => h`<tr><td class="num">${fmtInt(e.blockNumber)}</td><td class="l ${e.kind === 'Bought' ? 'up' : e.kind === 'Sold' ? 'down' : e.kind === 'Seeded' ? '' : 'dim'}">${e.kind}${e.kind === 'Refused' ? raw(' <span class="dim tiny">' + esc(ACTION_NAME[e.actionId] || '') + ': ' + esc(REFUSAL[e.reason] || String(e.reason)) + '</span>') : ''}</td><td class="num">${e.units != null ? fmtPos(e.units) : e.kind === 'Seeded' ? raw('<span class="dim">seed</span>') : '—'}</td><td class="num">${e.kind === 'Bought' || e.kind === 'Seeded' ? fmtMsk(e.mskIn, 2) : e.kind === 'Sold' ? fmtMsk(e.mskOut, 2) : e.actionId !== 2 ? fmtMsk(e.amount, 2) : '—'}</td><td class="num">${e.priceAfter != null ? fmtPx(e.priceAfter) : '—'}</td></tr>`) : raw('<tr><td colspan="5" class="empty">No settlements in the last ' + esc(String(CFG.LOG_LOOKBACK_BLOCKS)) + ' blocks.</td></tr>')}
      </tbody></table></div>`.s;
  }

  // ---- order entry ----------------------------------------------------------------------------
  function slipBps() { const s = Number(entry.slippage); return isFinite(s) && s >= 0 && s <= 50 ? BigInt(Math.round(s * 100)) : 100n; }
  function localQuote() {
    const m = rec && rec.market; if (!m) return null;
    if (!m.seeded) return { invalid: 'Not seeded yet: no curve to quote against.' };
    if (entry.side === 'buy') {
      const sompi = parseDec(entry.amount, 8); if (!sompi || sompi <= 0n) return null;
      if (m.closedToBuys) {
        // the wRPC reports closedToBuys for a retired line AND for a class that is not Active; say which
        const cs = classStatusOf(rec);
        if (cs && cs.head !== 'Active') return { invalid: 'The class is ' + cs.head + (cs.activationDaa != null ? ' (activates at DAA ' + fmtInt(cs.activationDaa) + ')' : '') + ': buys wait for Active; sells still queue.' };
        return { invalid: 'This line is closed to buys (retired); sells still queue.' };
      }
      const q = curve.buyQuote(m, sompi); if (!q) return { invalid: 'The curve releases no whole position for this amount (a position is whole: the least buy releases exactly one).' };
      return { kind: 'buy', sompi, unitsOut: q.unitsOut, fees: q.fees, priceAfter: q.priceAfter, source: 'local' };
    }
    if (String(entry.amount).trim() === '') return null;
    const units = parseDec(entry.amount, 0);
    if (units == null) return { invalid: 'Positions are whole numbers: enter an integer.' };
    if (units <= 0n) return null;
    const q = curve.sellQuote(m, units); if (!q) return { invalid: 'The curve pays nothing for this size (more than the positions outside the curve).' };
    return { kind: 'sell', units, fees: q.fees, priceAfter: q.priceAfter, source: 'local' };
  }
  async function nodeQuote(q) {
    if (!db.armed || !q || q.invalid) return;
    const seq = ++entry.quoteSeq;
    try {
      const r = q.kind === 'buy' ? await nodeQuoteBuy(rec.lineId, q.sompi) : await nodeQuoteSell(rec.lineId, q.units);
      if (seq !== entry.quoteSeq || !r) return;
      if (q.kind === 'buy' && r.unitsOut === 0n) return;
      if (q.kind === 'sell' && r.fees.net === 0n) return;
      entry.nodeQuote = Object.assign({ kind: q.kind, sompi: q.sompi, units: q.units }, r);
      renderQuote();
    } catch (e) { /* the local arithmetic stands */ }
  }
  function effectiveQuote() {
    const q = entry.quote; if (!q || q.invalid) return q;
    const n = entry.nodeQuote;
    if (n && n.kind === q.kind && ((q.kind === 'buy' && n.sompi === q.sompi) || (q.kind === 'sell' && n.units === q.units))) return Object.assign({}, q, n, { source: 'node' });
    return q;
  }
  function mins(q) {
    const bps = slipBps();
    if (q.kind === 'buy') return { minUnits: (q.unitsOut * (10000n - bps)) / 10000n };
    return { minMsk: (q.fees.net * (10000n - bps)) / 10000n };
  }
  function reasonNotToSend(q) {
    if (MOCK && !wallet.provider) return 'Mock wallet missing.';
    if (!wallet.provider) return 'No EIP-1193 wallet found: install MetaMask and reload.';
    if (!wallet.account) return 'Connect a wallet to trade.';
    if (!wallet.onChain()) return 'Switch the wallet to the MISAKA chain (' + CFG.CHAIN_ID + ').';
    if (db.armed === false) return 'The market is not armed on this network: the facade is an empty account.';
    if (db.armed === null) return 'EVM RPC unreachable: the transaction cannot be built.';
    if (!rec) return 'No line selected.';
    if (rec.market && !rec.market.seeded) return 'Not seeded yet: seed the market first.';
    if (rec.facadeSource !== 'registry') return 'Facade address not confirmed by the registry window yet.';
    if (!q) return 'Enter an amount.';
    if (q.invalid) return q.invalid;
    if (q.kind === 'buy' && rec.market && rec.market.closedToBuys) return 'This line is closed to buys (retired); sells still queue.';
    if (q.kind === 'buy') { const cs = classStatusOf(rec); if (cs && cs.head !== 'Active') return 'The class is ' + cs.head + (cs.activationDaa != null ? ' (activates at DAA ' + fmtInt(cs.activationDaa) + ')' : '') + ': buys wait for Active.'; }
    if (q.kind === 'buy' && entry.balance != null && sompiToWei(q.sompi) > entry.balance) return 'Insufficient MSK balance in the EVM account.';
    if (q.kind === 'sell' && entry.position != null && q.units > entry.position) return 'That is more than the EVM-namespace position this account holds.';
    return null;
  }
  function renderQuote() {
    if (!alive()) return;
    const box = $('#quoteBox'); if (!box) return;
    const q = effectiveQuote();
    const m = rec && rec.market;
    const price = m ? m.price : null;
    let rows = '';
    if (q && !q.invalid) {
      const mn = mins(q);
      const impact = price && q.priceAfter != null ? Number(((q.priceAfter - price) * 10000n) / price) / 100 : null;
      if (q.kind === 'buy') {
        const avg = q.unitsOut > 0n ? q.sompi / q.unitsOut : null;
        rows = h`
          <div class="r"><span>Positions out (whole)</span><span class="v">${fmtPos(q.unitsOut)}</span></div>
          <div class="r"><span>Average price</span><span class="v">${avg != null ? fmtPrice(avg) : '—'} MSK</span></div>
          <div class="r"><span>Price after</span><span class="v">${fmtPrice(q.priceAfter)} MSK</span></div>
          <div class="r"><span>Price impact</span><span class="v ${impact > 5 ? 'down' : ''}">${fmtPct(impact)}</span></div>
          <div class="r"><span>Burned (5 %)</span><span class="v">${fmtMsk(q.fees.burn)} MSK</span></div>
          <div class="r"><span>Owner leg (1 %)</span><span class="v">${fmtMsk(q.fees.leg)} MSK</span></div>
          <div class="r"><span>To the reserve (94 %)</span><span class="v">${fmtMsk(q.fees.net)} MSK</span></div>
          <div class="r tot"><span>Min positions (floor)</span><span class="v">${fmtPos(mn.minUnits)}</span></div>`.s;
      } else {
        const avg = q.units > 0n ? q.fees.net / q.units : null;
        rows = h`
          <div class="r"><span>Gross from the curve</span><span class="v">${fmtMsk(q.fees.gross)} MSK</span></div>
          <div class="r"><span>Burned (5 %)</span><span class="v">${fmtMsk(q.fees.burn)} MSK</span></div>
          <div class="r"><span>Owner leg (1 %)</span><span class="v">${fmtMsk(q.fees.leg)} MSK</span></div>
          <div class="r"><span>You receive (94 %)</span><span class="v">${fmtMsk(q.fees.net)} MSK</span></div>
          <div class="r"><span>Average price</span><span class="v">${avg != null ? fmtPrice(avg) : '—'} MSK</span></div>
          <div class="r"><span>Price after</span><span class="v">${fmtPrice(q.priceAfter)} MSK</span></div>
          <div class="r"><span>Price impact</span><span class="v ${impact < -5 ? 'down' : ''}">${fmtPct(impact)}</span></div>
          <div class="r tot"><span>Min MSK (floor)</span><span class="v">${fmtMsk(mn.minMsk)} MSK</span></div>`.s;
      }
      rows += h`<div class="r dim tiny"><span>Quoted by</span><span>${q.source === 'node' ? 'the node (AMM precompile)' : 'local arithmetic on the market row'}</span></div>`.s;
    } else if (q && q.invalid) rows = h`<div class="err">${q.invalid}</div>`.s;
    else rows = h`<div class="dim small">Enter an amount to see the quote. ${m && price != null ? 'Spot price ' + fmtPrice(price) + ' MSK per position.' : ''}</div>`.s;
    box.innerHTML = rows;
    const btn = $('#sendBtn'), why = $('#sendWhy');
    if (!btn) return;
    const reason = reasonNotToSend(q);
    btn.disabled = !!reason || entry.busy;
    btn.textContent = entry.busy ? 'Confirm in wallet…' : (entry.side === 'buy' ? 'Buy ' : 'Sell ') + (rec ? rec.symbol : '');
    btn.className = 'btn btn-lg ' + (entry.side === 'buy' ? 'btn-accent' : 'btn-sell');
    why.textContent = reason && q && !q.invalid ? reason : (reason && !q ? '' : reason || '');
    if (!wallet.account && !reason) why.textContent = '';
    if (reason && (!q || q.invalid)) why.textContent = (!wallet.account || !wallet.onChain() || db.armed !== true || (rec && rec.facadeSource !== 'registry')) ? reason : '';
  }
  const seedState = { msk: null, busy: false, balance: null };
  function renderEntry() {
    if (!alive()) return;
    const m = rec && rec.market;
    const acct = wallet.account;
    // ADR-0090: an unseeded line has no curve to trade on; the panel becomes the seed panel
    if (m && !m.seeded) {
      seedState.balance = entry.balance;
      renderSeedPanel($('#entry'), seedState, () => rec, () => { renderBottom(); });
      return;
    }
    $('#entry').innerHTML = h`
      <div class="panel-b">
        <div class="seg wide" role="tablist"><button class="${entry.side === 'buy' ? 'on buy' : ''}" data-side="buy" role="tab">Buy</button><button class="${entry.side === 'sell' ? 'on sell' : ''}" data-side="sell" role="tab">Sell</button></div>
        <div class="avail" style="margin-top:10px"><span class="muted">Available (EVM)</span><span class="v" id="availV">${entry.balance != null ? fmtWeiMsk(entry.balance) + ' MSK' : acct ? '—' : 'connect a wallet'}</span></div>
        <div class="avail"><span class="muted">Your position</span><span class="v" id="posV">${entry.position != null ? fmtPos(entry.position) + ' ' + (rec ? rec.symbol : '') : acct ? '—' : '—'}</span></div>
        <div class="field">
          <label for="amt">${entry.side === 'buy' ? 'Amount to pay (MSK, gross)' : 'Positions to sell (whole)'}</label>
          <div class="inp"><input id="amt" inputmode="${entry.side === 'buy' ? 'decimal' : 'numeric'}" autocomplete="off" placeholder="0" value="${entry.amount}" aria-describedby="quoteBox"><span class="unit">${entry.side === 'buy' ? 'MSK' : 'positions'}</span></div>
          <div class="pcts">${[25, 50, 75, 100].map((p) => h`<button data-pct="${p}" ${entry.side === 'buy' ? (entry.balance == null ? 'disabled' : '') : (entry.position == null ? 'disabled' : '')}>${p}%</button>`)}</div>
        </div>
        <div class="field">
          <label for="slip">Slippage tolerance (%)</label>
          <div class="inp"><input id="slip" inputmode="decimal" value="${entry.slippage}" aria-label="Slippage tolerance in percent"><span class="unit">${entry.side === 'buy' ? 'min positions' : 'min MSK'}</span></div>
        </div>
        <div class="quote" id="quoteBox"></div>
        <div class="field"><button id="sendBtn" class="btn btn-lg btn-accent" disabled>Buy</button><div class="reason" id="sendWhy"></div></div>
        <div class="note tiny">Your action is applied by the fold after the block that carries it and settles one block later; a fill worse than your floor is refused (never partial) and a refused buy is refunded. A position is a whole number. Positions bought here live in the EVM namespace and can only be sold from it.</div>
      </div>`.s;
    $$('#entry .seg button').forEach((b) => b.addEventListener('click', () => { entry.side = b.dataset.side; entry.amount = ''; entry.nodeQuote = null; renderEntry(); }));
    const amt = $('#amt'), slip = $('#slip');
    amt.addEventListener('input', () => { entry.amount = amt.value; entry.nodeQuote = null; entry.quote = localQuote(); renderQuote(); nodeQuote(entry.quote); });
    slip.addEventListener('input', () => { entry.slippage = slip.value; store.set('slippage', slip.value); renderQuote(); });
    $$('#entry .pcts button').forEach((b) => b.addEventListener('click', () => {
      const p = BigInt(b.dataset.pct);
      if (entry.side === 'buy' && entry.balance != null) { const reserve = 10n ** 16n; const wei = entry.balance > reserve ? entry.balance - reserve : 0n; entry.amount = fmtScaled(((wei / NATIVE_SCALE_WEI) * p) / 100n, 8, 8).replace(/,/g, ''); }
      if (entry.side === 'sell' && entry.position != null) entry.amount = ((entry.position * p) / 100n).toString();
      amt.value = entry.amount; entry.nodeQuote = null; entry.quote = localQuote(); renderQuote(); nodeQuote(entry.quote);
    }));
    $('#sendBtn').addEventListener('click', submit);
    entry.quote = localQuote(); renderQuote(); nodeQuote(entry.quote);
  }
  async function submit() {
    const q = effectiveQuote(); const reason = reasonNotToSend(q); if (reason) { toast(reason, 'warn'); return; }
    const mn = mins(q);
    const tx = { from: wallet.account, to: rec.facade };
    if (q.kind === 'buy') { tx.value = toHex(sompiToWei(q.sompi)); tx.data = ABI.call(SIG.buy, ABI.word(mn.minUnits)); }
    else { tx.value = '0x0'; tx.data = ABI.call(SIG.sell, ABI.word(q.units), ABI.word(mn.minMsk)); }
    entry.busy = true; renderQuote();
    try {
      const hash = await wallet.sendTx(tx);
      txlog.add({ hash, from: wallet.account, lineId: rec.lineId, label: rec.symbol, kind: q.kind, amount: (q.kind === 'buy' ? q.sompi : q.units).toString(), min: (q.kind === 'buy' ? mn.minUnits : mn.minMsk).toString(), sentAt: Date.now(), status: 'sent' });
      toast('Transaction sent: ' + shortId(hash, 10) + '. Waiting for the block that carries it.', 'ok', (q.kind === 'buy' ? 'Buy ' : 'Sell ') + rec.symbol);
      entry.amount = ''; entry.nodeQuote = null;
    } catch (e) { toast((e && e.message) || String(e), 'bad', 'Wallet'); }
    entry.busy = false; renderEntry(); renderBottom();
  }

  // ---- bottom tabs ----------------------------------------------------------------------------
  function statusCell(t) {
    const map = { sent: ['Sent', 'tag'], queued: ['Queued', 'tag warn'], reverted: ['Reverted', 'tag bad'], settled: ['Settled', 'tag ok'], refused: ['Refused', 'tag bad'], lost: ['Not found', 'tag bad'] };
    const [txt, cls] = map[t.status] || [t.status, 'tag'];
    return h`<span class="${cls}">${txt}</span>${t.status === 'refused' && t.reason ? raw(' <span class="dim tiny">' + esc(REFUSAL[t.reason] || String(t.reason)) + '</span>') : ''}`;
  }
  function renderBottom() {
    if (!alive()) return;
    const body = $('#bottomBody'); if (!body) return;
    const acct = wallet.account;
    if (view.bottomTab === 'positions') {
      if (!acct) { body.innerHTML = '<div class="empty">Connect a wallet to see the positions this account holds across lines.</div>'; return; }
      const p = view.positions;
      if (!p) { body.innerHTML = '<div class="empty">Loading positions…</div>'; return; }
      if (p.error) { body.innerHTML = h`<div class="empty">Positions unavailable: ${p.error}</div>`.s; return; }
      if (!p.positions.length) { body.innerHTML = h`<div class="empty">No positions in the EVM namespace for ${shortAddr(acct)}. <span class="dim">holder id ${shortId(p.holder)}</span></div>`.s; return; }
      body.innerHTML = h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th>Line</th><th>Positions</th><th>Price</th><th>Mark value (sell now, net)</th><th></th></tr></thead><tbody>
        ${p.positions.map((x) => { const r = db.line(x.lineId) || db.upsertLine(x.lineId, {}); const m = r.market; const q = m ? curve.sellQuote(m, x.units) : null; return h`<tr class="row-link" data-href="#/trade/${x.lineId}"><td class="l">${db.label(r)} <span class="dim tiny">${r.symbol}</span></td><td class="num">${fmtPos(x.units)}</td><td class="num">${m && m.price != null ? fmtPrice(m.price) : '—'}</td><td class="num">${q ? fmtMsk(q.fees.net) + ' MSK' : '—'}</td><td><a href="#/trade/${x.lineId}">Trade</a></td></tr>`; })}
      </tbody></table></div><div class="note tiny">Read through ${p.source === 'wrpc' ? 'getPalwModelPositions(holder)' : 'the position window per known line'}; holder id ${shortId(p.holder)}. A position is a whole number. Mark value is the net MSK a sell of the whole position would return right now.</div>`.s;
      return;
    }
    if (view.bottomTab === 'history') {
      const list = txlog.forAccount(acct);
      if (!list.length) { body.innerHTML = '<div class="empty">No transactions sent from this browser' + (acct ? ' by ' + shortAddr(acct) : '') + '.</div>'; return; }
      body.innerHTML = h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th>Time</th><th>Line</th><th>Action</th><th>Amount</th><th>Floor</th><th>Status</th><th>Block</th><th>Settled</th><th>Tx</th></tr></thead><tbody>
        ${list.map((t) => h`<tr><td class="l">${fmtDateTime(t.sentAt)}</td><td class="l">${t.label}</td><td class="l ${t.kind === 'buy' ? 'up' : t.kind === 'sell' ? 'down' : ''}">${t.kind}</td><td class="num">${t.kind === 'sell' ? fmtPos(t.amount) + ' pos' : fmtMsk(t.amount) + ' MSK' + (t.kind === 'seed' ? ' (locked)' : '')}</td><td class="num">${t.kind === 'buy' ? fmtPos(t.min) + ' pos' : t.kind === 'sell' ? fmtMsk(t.min) + ' MSK' : '—'}</td><td class="l">${statusCell(t)}</td><td class="num">${t.blockNumber != null ? fmtInt(t.blockNumber) : '—'}</td><td class="num">${t.status === 'settled' ? (t.kind === 'seed' ? 'seeded ' + fmtMsk(t.msk) + ' MSK, first price ' + fmtPrice(t.priceAfter) : fmtPos(t.units) + ' pos for ' + fmtMsk(t.msk) + ' MSK') : t.settledBlock ? 'block ' + fmtInt(t.settledBlock) : '—'}</td><td class="l">${idCell(t.hash, 10)}</td></tr>`)}
      </tbody></table></div>`.s;
      return;
    }
    if (view.bottomTab === 'settlements') {
      if (!acct) { body.innerHTML = '<div class="empty">Connect a wallet to list its settlement events.</div>'; return; }
      const s = view.mySettlements;
      if (db.armed === false) { body.innerHTML = '<div class="empty">The market doors are dormant on this network; no facade emits events yet.</div>'; return; }
      if (!s) { body.innerHTML = '<div class="empty">' + (db.armed ? 'Loading settlement events…' : 'EVM RPC unreachable.') + '</div>'; return; }
      if (!s.length) { body.innerHTML = '<div class="empty">No settlements for ' + esc(shortAddr(acct)) + ' in the last ' + CFG.LOG_LOOKBACK_BLOCKS + ' blocks of the known lines.</div>'; return; }
      body.innerHTML = h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th>Block</th><th>Line</th><th>Event</th><th>Positions</th><th>MSK</th><th>Price after</th><th>Tx</th></tr></thead><tbody>
        ${s.map((e) => { const r = db.line(e.lineId); return h`<tr><td class="num">${fmtInt(e.blockNumber)}</td><td class="l">${r ? db.label(r) : shortId(e.lineId)}</td><td class="l ${e.kind === 'Bought' ? 'up' : e.kind === 'Sold' ? 'down' : ''}">${e.kind}${e.kind === 'Refused' ? raw(' <span class="dim tiny">' + esc(ACTION_NAME[e.actionId] || '') + ': ' + esc(REFUSAL[e.reason] || String(e.reason)) + '</span>') : ''}</td><td class="num">${e.units != null ? fmtPos(e.units) : e.kind === 'Refused' && e.actionId === 2 ? fmtPos(e.amount) : e.kind === 'Seeded' ? '0 (seed)' : '—'}</td><td class="num">${e.kind === 'Bought' ? fmtMsk(e.mskIn) : e.kind === 'Seeded' ? fmtMsk(e.mskIn) + ' locked' : e.kind === 'Sold' ? fmtMsk(e.mskOut) : e.actionId !== 2 ? fmtMsk(e.amount) + ' refunded' : '—'}</td><td class="num">${e.priceAfter != null ? fmtPrice(e.priceAfter) : '—'}</td><td class="l">${idCell(e.txHash || '', 10)}</td></tr>`; })}
      </tbody></table></div>`.s;
      return;
    }
    if (!rec) { body.innerHTML = '<div class="empty">No line selected.</div>'; return; }
    const row = rec.row || {}, m = rec.market, info = rec.info;
    const cs = classStatusOf(rec);
    body.innerHTML = h`<div class="grid2">
      <dl class="kv">
        <dt>Line id</dt><dd>${idCell(rec.lineId, 16)} <a href="#/line/${rec.lineId}">line page</a></dd>
        <dt>Class id</dt><dd>${rec.classId ? idCell(rec.classId, 16) : '—'}</dd>
        <dt>Name</dt><dd>${rec.name || raw('<span class="dim">— (this node serves no line name)</span>')}</dd>
        ${catalogueRows(rec)}
        <dt>Status</dt><dd>${row.status || '—'} ${row.hasRow === false ? raw('<span class="tag">founding line, no row yet</span>') : ''}</dd>
        <dt>Owner</dt><dd>${row.ownerPayoutPayload ? idCell(row.ownerPayoutPayload, 12) : row.owner ? h`${shortId(row.owner.transactionId)}:${row.owner.index}` : rec.row ? 'unowned (genesis line)' : '—'}</dd>
        <dt>Developer</dt><dd>${row.developerPayoutPayload ? idCell(row.developerPayoutPayload, 12) : rec.row ? 'the owner' : '—'}</dd>
        <dt>Maintainer</dt><dd>${row.maintainerPayoutPayload ? idCell(row.maintainerPayoutPayload, 12) : rec.row ? 'the owner' : '—'}</dd>
        <dt>Founded (DAA)</dt><dd>${row.foundedDaa != null ? fmtInt(row.foundedDaa) : '—'}</dd>
        <dt>Versions</dt><dd>${row.versionsPublished != null ? row.versionsPublished + ' published, current v' + row.current + (row.previews && row.previews.length ? ', previews ' + row.previews.join(', ') : '') : '—'}</dd>
        <dt>Contributor share of the leg</dt><dd>${row.contributorPermilleOfLeg != null ? row.contributorPermilleOfLeg + ' ‰' : '—'}</dd>
        <dt>Current root</dt><dd>${info && info.currentRoot ? idCell(info.currentRoot, 16) : '—'}</dd>
        <dt>Roots in force</dt><dd>${info && info.rootsInForce ? (info.rootsInForce.length ? raw(info.rootsInForce.map((r) => (r ? idCell(r, 12).s : '(id not served by the EVM window)')).join('<br>')) : 'none') : '—'}</dd>
      </dl>
      <dl class="kv">
        <dt>Facade (MRC-20)</dt><dd><span class="mono">${rec.facade}</span> <span class="tag ${rec.facadeSource === 'registry' ? 'ok' : 'warn'}">${rec.facadeSource === 'registry' ? 'from the registry' : 'derived locally, unconfirmed'}</span></dd>
        <dt>Symbol</dt><dd>${rec.symbol}</dd>
        <dt>Market</dt><dd>${m ? (m.seeded ? 'seeded at DAA ' + fmtInt(m.openedDaa) : 'not seeded yet (a seed of at least ' + fmtMsk(seedMinFor(m), 0) + ' MSK opens it)') : '—'}</dd>
        <dt>Seed (locked)</dt><dd>${m && m.seeded ? (m.seedSompi != null ? fmtMsk(m.seedSompi) + ' MSK, locked for good' : raw('<span class="dim">not served by the AMM window</span>')) : m ? 'none' : '—'}</dd>
        <dt>Bought by mining</dt><dd>${m && m.buybackSompi != null ? fmtMsk(m.buybackSompi) + ' MSK · ' + fmtPos(m.retiredUnits || 0n) + ' positions retired' : raw('<span class="dim">not served by this node</span>')}</dd>
        <dt>Seeded by</dt><dd>${m && m.seededBy ? idCell(m.seededBy, 12) : '—'}</dd>
        <dt>Reserve</dt><dd>${m ? fmtMsk(m.mskReserve) + ' MSK' : '—'}</dd>
        <dt>Positions in the curve</dt><dd>${m && !m.legacy ? fmtPos(m.positionUnits) + ' of ' + fmtPos(m.supplyUnits) : m ? raw('<span class="dim">— (the node serves the pre-ADR-0090 row: units of 10<sup>-6</sup> position, a virtual reserve)</span>') : '—'}</dd>
        <dt>Sold (cumulative)</dt><dd>${m && !m.legacy ? fmtPos(m.soldUnits) + ' positions' : '—'}</dd>
        <dt>Burned</dt><dd>${m ? fmtMsk(m.burnedSompi) + ' MSK' : '—'}</dd>
        <dt>Paid to the owner</dt><dd>${m ? fmtMsk(m.ownerPaid) + ' MSK' : '—'}</dd>
        <dt>Paid to a contributor</dt><dd>${m ? fmtMsk(m.contributorPaid) + ' MSK' : '—'}</dd>
        <dt>Closed to buys</dt><dd>${m ? (m.closedToBuys ? 'yes' : 'no') : '—'}</dd>
        <dt>Class status</dt><dd>${cs ? cs.head + (cs.activationDaa != null ? ', activates at DAA ' + fmtInt(cs.activationDaa) : '') + (cs.sinceDaa != null ? ' since DAA ' + fmtInt(cs.sinceDaa) : '') : '—'}</dd>
        <dt>Least seed (network)</dt><dd>${m ? fmtMsk(seedMinFor(m), 0) + ' MSK' : '—'}</dd>
        <dt>Read through</dt><dd>${m ? (m.source === 'wrpc' ? 'wRPC getPalwModelMarket' : 'AMM window (eth_call)') + ', ' + fmtAgo(m.at) : '—'}</dd>
      </dl></div>`.s;
  }
  $('#bottomTabs').addEventListener('click', (e) => { const b = e.target.closest('button'); if (!b) return; view.bottomTab = b.dataset.t; $$('#bottomTabs button').forEach((x) => x.classList.toggle('on', x === b)); renderBottom(); if (view.bottomTab === 'settlements') loadMySettlements(); if (view.bottomTab === 'positions') loadPositions(); });

  // ---- data refresh ---------------------------------------------------------------------------
  async function loadPositions() {
    if (!alive()) return;
    if (!wallet.account) { view.positions = null; return; }
    try {
      const r = await positionsOf(wallet.account);
      view.positions = r;
      await pool(r.positions.filter((p) => !db.line(p.lineId) || !db.line(p.lineId).market), 3, (p) => refreshMarket(p.lineId));
      const mine = r.positions.find((p) => p.lineId === lineId);
      if (rec && r.source === 'wrpc') { entry.position = mine ? mine.units : 0n; entry.positionSrc = 'wrpc'; }
    } catch (e) { view.positions = { error: e.message, positions: [] }; }
    if (view.bottomTab === 'positions') renderBottom();
  }
  async function loadMySettlements() {
    if (!alive()) return;
    if (!wallet.account || !db.armed) { view.mySettlements = null; return; }
    const ids = Array.from(db.lines.keys()).slice(0, 12);
    const all = await pool(ids, 3, (id) => settlementLogs(id, wallet.account));
    view.mySettlements = all.filter(Boolean).flat().sort((a, b) => b.blockNumber - a.blockNumber || b.logIndex - a.logIndex);
    if (view.bottomTab === 'settlements') renderBottom();
  }
  async function loadAccount() {
    if (!alive()) return;
    if (!wallet.account || !rec) { entry.balance = null; entry.position = null; return; }
    try { entry.balance = await balanceOf(wallet.account); } catch (e) { entry.balance = null; }
    try { entry.position = await positionOf(rec.lineId, wallet.account); } catch (e) { entry.position = null; }
    const a = $('#availV'), p = $('#posV');
    if (a) a.textContent = entry.balance != null ? fmtWeiMsk(entry.balance) + ' MSK' : '—';
    if (p) p.textContent = entry.position != null ? fmtPos(entry.position) + ' ' + rec.symbol : '—';
    $$('#entry .pcts button').forEach((b) => { b.disabled = entry.side === 'buy' ? entry.balance == null : entry.position == null; });
    // the seed panel re-checks its "can send" reason against the balance (not while the user types)
    if (rec.market && !rec.market.seeded && seedState.balance !== entry.balance && !seedState.busy && !$('#seedAmt:focus')) { seedState.balance = entry.balance; renderEntry(); }
    renderQuote();
  }
  async function loadSettlements() {
    if (!alive()) return;
    if (!rec || !db.armed) { view.settlements = db.armed ? null : undefined; renderDepth(); return; }
    try { view.settlements = await settlementLogs(rec.lineId); await foldEventsIntoHistory(rec.lineId, view.settlements); renderChart(); }
    catch (e) { view.settlements = []; }
    renderDepth();
  }
  let ticks = 0, wasSeeded = null;
  async function tick() {
    if (!alive()) return;
    ticks++;
    if (rec) {
      await refreshMarket(rec.lineId);
      if (ticks === 1 || ticks % 6 === 0) { await refreshLineInfo(rec.lineId); await refreshFacade(rec.lineId); await refreshUsage(rec.lineId); }
      if (!alive()) return;
      // the panel is the order entry for a seeded line and the seed panel for an unseeded one;
      // it is rebuilt when that fact changes (a seed landed) or when the facade got confirmed
      const seededNow = rec.market ? !!rec.market.seeded : null;
      if ((seededNow !== wasSeeded || ticks === 1) && !$('#seedAmt:focus') && !$('#amt:focus')) { wasSeeded = seededNow; renderEntry(); }
      renderBar(); renderChart(); renderDepth();
      entry.quote = localQuote(); renderQuote(); if (ticks % 3 === 0) nodeQuote(entry.quote);
      if (ticks === 1 || ticks % 3 === 0) loadSettlements();
    }
    loadAccount();
    if (ticks % 3 === 0 && view.bottomTab === 'positions') loadPositions();
    pollTransactions().then(() => { if (view.bottomTab === 'history') renderBottom(); });
  }

  renderBar(); renderChart(); renderDepth(); renderEntry(); renderBottom(); renderChartAlt();
  const onWallet = () => { renderNav(); renderEntry(); loadAccount(); loadPositions(); renderBottom(); };
  wallet.listeners.add(onWallet); onCleanup(() => wallet.listeners.delete(onWallet));
  const onDb = () => { if (rec && !rec.row && db.line(rec.lineId) && db.line(rec.lineId).row) renderBar(); };
  db.listeners.add(onDb); onCleanup(() => db.listeners.delete(onDb));
  if (rec) {
    if (!rec.row) refreshLineInfo(rec.lineId).then(renderBar).catch(() => {});
    tick().catch(() => {});
    every(CFG.POLL_MS, tick);
  } else {
    // nothing to poll for; keep discovery alive so the selector fills in when a node answers
    every(15000, async () => { await discoverClasses(); await discoverLines(); if (firstLineId()) navigate('#/trade/' + firstLineId()); });
  }
  loadPositions();
}

// ============================================================================================
// 11. models (lines), leaderboard, line, portfolio, docs
// ============================================================================================
async function loadAllLines(withUsage) {
  await discoverClasses();
  await discoverLines();
  const ids = Array.from(db.lines.keys());
  await pool(ids, 4, async (id) => { await refreshMarket(id); const r = db.line(id); if (!r.row) await refreshLineInfo(id); });
  if (withUsage) await pool(ids, 4, (id) => refreshUsage(id));
  db.emit();
}
function usageOf(r) { return r.usage ? r.usage.attempt + r.usage.fp : null; }
function lineRowsHtml(list, opts) {
  opts = opts || {};
  return h`<div class="tbl-wrap"><table class="tbl" id="linesTbl"><thead><tr>
    ${opts.rank ? raw('<th>#</th>') : ''}
    <th class="l">Line</th><th class="l">Class</th><th class="sortable ${opts.sort === 'price' ? 'sorted' : ''}" data-sort="price">Price (MSK)</th><th>24h</th><th class="sortable ${opts.sort === 'reserve' ? 'sorted' : ''}" data-sort="reserve">Reserve (MSK)</th><th class="sortable ${opts.sort === 'seed' ? 'sorted' : ''}" data-sort="seed" title="The MSK the market opened with, locked for good">Seed (locked)</th><th class="sortable ${opts.sort === 'sold' ? 'sorted' : ''}" data-sort="sold">Sold / Supply</th><th class="sortable ${opts.sort === 'buyback' ? 'sorted' : ''}" data-sort="buyback" title="MSK the mining reward has bought into the pair (ADR-0091)">Mining bought</th><th class="l">Owner</th><th>Versions</th><th class="sortable ${opts.sort === 'usage' ? 'sorted' : ''}" data-sort="usage">Usage (claims)</th><th class="l">Status</th>
  </tr></thead><tbody>
    ${list.length ? list.map((r, i) => { const m = r.market, row = r.row, o = ownerLabel(row), u = usageOf(r); const held = m ? m.supplyUnits - m.positionUnits : null; const seeded = !!(m && m.seeded); return h`<tr class="row-link" data-href="#/trade/${r.lineId}">
      ${opts.rank ? h`<td class="rank">${i + 1}</td>` : ''}
      <td class="l">${nameCell(r)}<br><span class="dim tiny mono">${shortId(r.lineId, 12)}</span></td>
      <td class="l"><span class="mono tiny" title="${r.classId || ''}">${r.classId ? shortId(r.classId) : '—'}</span></td>
      <td class="num">${seeded && m.price != null ? fmtPrice(m.price) : m && !seeded ? raw('<a class="btn btn-sm btn-accent" href="#/trade/' + esc(r.lineId) + '" title="No market yet: seed it with at least ' + esc(fmtMsk(seedMinFor(m), 0)) + ' MSK">Seed</a>') : '—'}</td>
      <td class="num">${seeded ? changeCell(r) : raw('<span class="dim">—</span>')}</td>
      <td class="num">${seeded ? fmtMsk(m.mskReserve, 2) : m ? raw('<span class="dim">0</span>') : '—'}</td>
      <td class="num">${seeded ? (m.seedSompi != null ? fmtMsk(m.seedSompi, 0) : raw('<span class="dim" title="not served by the AMM window">—</span>')) : m ? raw('<span class="dim">none</span>') : '—'}</td>
      <td class="num" title="${m && m.legacy ? 'pre-ADR-0090 row: not shown' : m ? 'ever bought: ' + fmtPos(m.soldUnits) : ''}">${m && !m.legacy ? fmtPos(held) + ' / ' + fmtPos(m.supplyUnits) : '—'}</td>
      <td class="num" title="${m && m.buybackSompi != null && bi(m.retiredUnits || 0n) > 0n ? fmtPos(m.retiredUnits) + ' positions retired' : '5 % of every block\'s reward on this line'}">${m && m.buybackSompi != null ? fmtMsk(m.buybackSompi, 2) : raw('<span class="dim">—</span>')}</td>
      <td class="l" title="${o ? o.title : ''}">${o ? o.text : '—'}</td>
      <td class="num">${row && row.versionsPublished != null ? row.versionsPublished + ' (v' + row.current + ')' : '—'}</td>
      <td class="num" title="attempt + free-prompt claims on the current version, counted by the fold">${u != null ? fmtInt(u) : '—'}</td>
      <td class="l">${statusTag(r)} <a class="tiny" href="#/line/${r.lineId}">line</a></td>
    </tr>`; }) : raw('<tr><td colspan="13" class="empty">No lines known yet. ' + (db.classes.size ? 'Waiting for a node to answer.' : 'Configure CLASS_IDS in config.js, or reach an EVM RPC whose registry window is armed.') + '</td></tr>')}
  </tbody></table></div>`.s;
}
function sortLines(list, key) {
  const val = (r) => {
    const m = r.market;
    if (key === 'price') return m && m.price != null ? m.price : -1n;
    if (key === 'reserve') return m ? m.mskReserve : -1n;
    if (key === 'seed') return m ? (m.seedSompi != null ? m.seedSompi : m.seeded ? 0n : -1n) : -2n;
    if (key === 'buyback') return m && m.buybackSompi != null ? m.buybackSompi : -1n;
    if (key === 'sold') return m ? m.supplyUnits - m.positionUnits : -1n;
    if (key === 'usage') { const u = usageOf(r); return u == null ? -1n : u; }
    return 0n;
  };
  return list.slice().sort((a, b) => { const x = val(a), y = val(b); return x === y ? db.label(a).localeCompare(db.label(b)) : (y > x ? 1 : -1); });
}

async function pageLines() {
  const gen = pageState.gen, alive = () => gen === pageState.gen;
  const main = $('#main');
  const view = { sort: store.get('lines:sort', 'reserve'), q: '' };
  main.innerHTML = h`<div class="page-h"><h1>Models</h1><span class="sub">every line the node reports across the known classes, priced on its curve</span></div>
    <div class="toolbar"><input type="search" id="q" placeholder="Filter by name, symbol or id" aria-label="Filter lines"><span class="dim small" id="linesNote"></span></div>
    <div id="linesBox"></div>`.s;
  function render() {
    if (!alive()) return;
    let list = sortLines(Array.from(db.lines.values()), view.sort);
    if (view.q) { const q = view.q.toLowerCase(); list = list.filter((r) => (r.name || '').toLowerCase().includes(q) || r.symbol.toLowerCase().includes(q) || r.lineId.includes(q) || (r.classId || '').includes(q)); }
    $('#linesBox').innerHTML = lineRowsHtml(list, { sort: view.sort });
    const seededN = Array.from(db.lines.values()).filter((r) => r.market && r.market.seeded).length;
    $('#linesNote').textContent = db.lines.size + ' line' + (db.lines.size === 1 ? '' : 's') + ' across ' + db.classes.size + ' class' + (db.classes.size === 1 ? '' : 'es') + ' · ' + seededN + ' seeded' + (db.armed === false ? ' · market dormant: no line can be seeded yet' : '') + ' · supply 500,000 whole positions a line';
    $$('#linesTbl th.sortable').forEach((th) => th.addEventListener('click', () => { view.sort = th.dataset.sort; store.set('lines:sort', view.sort); render(); }));
  }
  $('#q').addEventListener('input', (e) => { view.q = e.target.value; render(); });
  render();
  const onDb = () => render(); db.listeners.add(onDb); onCleanup(() => db.listeners.delete(onDb));
  await loadAllLines(true); render();
  every(30000, () => loadAllLines(true));
}

async function pageLeaderboard() {
  const gen = pageState.gen, alive = () => gen === pageState.gen;
  const main = $('#main');
  const view = { sort: store.get('lb:sort', 'reserve') };
  main.innerHTML = h`<div class="page-h"><h1>Leaderboard</h1><span class="sub">lines ranked by what the chain itself measures</span></div>
    <div class="toolbar"><span class="seg" id="lbSort"><button data-s="reserve">Reserve</button><button data-s="seed">Seed</button><button data-s="sold">Sold</button><button data-s="usage">Usage</button></span><span class="dim small">Usage is the fold's count of paid inferences on the current version. Declared evaluations are not ranked: the chain refuses a quality oracle (ADR-0056 D7, ADR-0088 D5). Unseeded lines have no price and rank last.</span></div>
    <div id="lbBox"></div>`.s;
  function render() {
    if (!alive()) return;
    $$('#lbSort button').forEach((b) => b.classList.toggle('on', b.dataset.s === view.sort));
    $('#lbBox').innerHTML = lineRowsHtml(sortLines(Array.from(db.lines.values()), view.sort), { sort: view.sort, rank: true });
    $$('#linesTbl th.sortable').forEach((th) => th.addEventListener('click', () => { if (th.dataset.sort !== 'price') { view.sort = th.dataset.sort; store.set('lb:sort', view.sort); render(); } }));
  }
  $('#lbSort').addEventListener('click', (e) => { const b = e.target.closest('button'); if (b) { view.sort = b.dataset.s; store.set('lb:sort', view.sort); render(); } });
  render();
  const onDb = () => render(); db.listeners.add(onDb); onCleanup(() => db.listeners.delete(onDb));
  await loadAllLines(true); render();
  every(30000, () => loadAllLines(true));
}

async function pageLine(arg) {
  const gen = pageState.gen, alive = () => gen === pageState.gen;
  const main = $('#main');
  const lineId = normId(arg);
  if (!lineId) { main.innerHTML = '<div class="empty">Not a line id (expected 128 hex characters).</div>'; return; }
  const rec = db.upsertLine(lineId, {});
  main.innerHTML = '<div class="empty">Loading line…</div>';
  function render() {
    if (!alive()) return;
    const row = rec.row || {}, m = rec.market, info = rec.info;
    const o = ownerLabel(row);
    const roles = [['Owner', row.owner, row.ownerPayoutPayload, o && !row.ownerPayoutPayload ? o.text : null], ['Developer', row.developer, row.developerPayoutPayload, 'the owner'], ['Maintainer', row.maintainer, row.maintainerPayoutPayload, 'the owner']];
    const versions = rec.versions;
    const inForce = info && info.rootsInForce ? info.rootsInForce : null;
    main.innerHTML = h`
      <div class="page-h"><h1>${db.label(rec)}</h1><span class="sub">${modelSub(rec) ? modelSub(rec) + ' · ' : ''}${raw('<span class="mono">' + esc(rec.symbol) + ' · ' + esc(shortId(lineId, 16)) + '</span>')}</span>${statusTag(rec)} <a class="btn btn-sm btn-accent" href="#/trade/${lineId}">Trade</a></div>
      ${rec.notFound ? raw('<div class="banner bad" style="margin-bottom:8px">The node reports no line and no class with this id.</div>') : ''}
      <div class="grid3">
        <div class="tile"><div class="k">Price (MSK)</div><div class="v">${m && m.price != null ? fmtPrice(m.price) : m && !m.seeded ? raw('<a class="btn btn-sm btn-accent" href="#/trade/' + esc(lineId) + '">Seed this market</a>') : '—'}</div><div class="s">${m ? (m.seeded ? 'seeded at DAA ' + fmtInt(m.openedDaa) : 'not seeded: no reserve, no price; at least ' + fmtMsk(seedMinFor(m), 0) + ' MSK opens it') : ''}</div></div>
        <div class="tile"><div class="k">Reserve (MSK)</div><div class="v">${m ? fmtMsk(m.mskReserve, 2) : '—'}</div><div class="s">burned ${m ? fmtMsk(m.burnedSompi, 2) : '—'} · owner paid ${m ? fmtMsk(m.ownerPaid, 2) : '—'}</div></div>
        <div class="tile"><div class="k">Seed (locked)</div><div class="v">${m && m.seeded ? (m.seedSompi != null ? fmtMsk(m.seedSompi, 2) : '—') : m ? 'none' : '—'}</div><div class="s">${m && m.seededBy ? 'by ' + shortId(m.seededBy, 10) + ' (payout payload, for the record)' : m && m.seeded ? 'seeder not served by the AMM window' : 'locked for good once paid; the seeder holds no position'}</div></div>
        <div class="tile"><div class="k">Sold / Supply</div><div class="v">${m && !m.legacy ? fmtPos(m.supplyUnits - m.positionUnits) + ' / ' + fmtPos(m.supplyUnits) : '—'}</div><div class="s">${m && m.legacy ? 'pre-ADR-0090 row from the node: not shown' : 'whole positions · ever bought ' + (m ? fmtPos(m.soldUnits) : '—')}</div></div>
        <div class="tile"><div class="k">Bought by mining</div><div class="v">${m && m.buybackSompi != null ? fmtMsk(m.buybackSompi, 2) : m ? '—' : '—'}</div><div class="s">${m && m.buybackSompi != null ? (bi(m.retiredUnits || 0n) > 0n ? fmtPos(m.retiredUnits) + ' positions retired (the chain\'s, for good)' : 'no position retired yet: each slice is under one position\'s price') : 'this node serves no buyback (pre-ADR-0091)'}</div></div>
        <div class="tile"><div class="k">Versions</div><div class="v">${row.versionsPublished != null ? row.versionsPublished : '—'}</div><div class="s">current v${row.current || '—'}${row.previews && row.previews.length ? ' · previews ' + row.previews.join(', ') : ''}</div></div>
        <div class="tile"><div class="k">Usage (current version)</div><div class="v">${rec.usage ? fmtInt(rec.usage.attempt + rec.usage.fp) : '—'}</div><div class="s">${rec.usage ? fmtInt(rec.usage.attempt) + ' attempt · ' + fmtInt(rec.usage.fp) + ' free-prompt · ' + fmtInt(rec.usage.workLeaves) + ' leaves' : 'claims counted by the fold'}</div></div>
        <div class="tile"><div class="k">Roots in force</div><div class="v">${inForce ? inForce.length : '—'}</div><div class="s">for the class at DAA ${info && info.tipDaa != null ? fmtInt(info.tipDaa) : '—'}</div></div>
      </div>
      <div class="grid2 section">
        <div class="panel"><div class="panel-h">The line</div><div class="panel-b"><dl class="kv">
          <dt>Line id</dt><dd>${idCell(lineId, 24)}</dd>
          <dt>Class id</dt><dd>${rec.classId ? idCell(rec.classId, 24) : '—'}</dd>
          <dt>Name</dt><dd>${rec.name || raw('<span class="dim">—</span>')} ${row.nameHex ? raw('<span class="dim tiny mono">hex ' + esc(row.nameHex) + '</span>') : ''}</dd>
          ${catalogueRows(rec)}
          <dt>Founding</dt><dd>${rec.classId === lineId ? 'the class’s founding line (line id = class id)' : 'founded on its class'}${row.hasRow === false ? ', no row written yet' : ''}</dd>
          <dt>Founded (DAA)</dt><dd>${row.foundedDaa != null ? fmtInt(row.foundedDaa) : '—'}</dd>
          <dt>Status</dt><dd>${row.status || '—'}${row.retiredDaa ? ' at DAA ' + fmtInt(row.retiredDaa) : ''}</dd>
          <dt>Contributor share of the leg</dt><dd>${row.contributorPermilleOfLeg != null ? row.contributorPermilleOfLeg + ' ‰' : '—'}</dd>
          <dt>Facade</dt><dd><span class="mono">${rec.facade}</span> <span class="tag ${rec.facadeSource === 'registry' ? 'ok' : 'warn'}">${rec.facadeSource === 'registry' ? 'registry' : 'derived'}</span></dd>
          <dt>Current root</dt><dd>${info && info.currentRoot ? idCell(info.currentRoot, 24) : '—'}</dd>
        </dl></div></div>
        <div class="panel"><div class="panel-h">Roles</div><div class="panel-b"><div class="tbl-wrap"><table class="tbl"><thead><tr><th class="l">Role</th><th class="l">Bond (outpoint)</th><th class="l">Payout payload</th></tr></thead><tbody>
          ${roles.map(([name, bond, payload, dflt]) => h`<tr><td class="l">${name}</td><td class="l">${bond && bond.transactionId ? raw(idCell(bond.transactionId, 10).s + ':' + esc(String(bond.index))) : rec.row ? (dflt || '—') : '—'}</td><td class="l">${payload ? idCell(payload, 16) : rec.row ? (dflt || '—') : '—'}</td></tr>`)}
        </tbody></table></div><div class="note tiny">A bond is the chain's address-shaped identity (ML-DSA-87). The owner keeps the cold key; the developer signs versions; nothing here moves a position.</div>
        <div class="panel-h" style="border-top:1px solid var(--border);margin-top:8px">Roots in force</div>
        <div style="padding:6px 0">${inForce ? (inForce.length ? raw(inForce.map((r) => '<div class="hash">' + (r ? idCell(r, 32).s : '(id not served by the EVM window)') + '</div>').join('')) : '<span class="dim">none</span>') : '—'}</div>
        </div></div>
      </div>
      <div class="panel section"><div class="panel-h">Versions <span class="spacer"></span><span class="dim">the node holds the last 64; the explorer keeps the whole history</span></div><div class="panel-b" id="versionsBox">
        ${!versions ? raw('<div class="empty">Loading versions…</div>') : !versions.length ? raw('<div class="empty">No version rows are held for this line' + (row.hasRow === false ? ' (a founding line nothing touched has its registration root as version 1)' : '') + '.</div>') :
          raw(versions.map((v) => h`<details ${v.status === 'Current' ? 'open' : ''}><summary><b>v${v.version}</b> · ${v.status || '—'}${v.inForce ? raw(' <span class="tag ok">in force</span>') : ''} · root ${shortId(v.root, 12)} · published DAA ${fmtInt(v.publishedDaa)} · usage ${fmtInt(bi(v.attemptClaims) + bi(v.fpClaims))} claims · ${(v.evaluations || []).length} evaluation${(v.evaluations || []).length === 1 ? '' : 's'} <span class="tag declared">declared</span></summary>
            <div class="grid2" style="padding:8px 0 4px"><dl class="kv">
              <dt>Root</dt><dd>${idCell(v.root, 32)}</dd>
              <dt>Parent</dt><dd>${v.parent || '—'}</dd>
              <dt>Adopted from</dt><dd>${v.adoptedFrom ? idCell(v.adoptedFrom, 24) : '—'}</dd>
              <dt>Published by</dt><dd>${v.publishedBy && v.publishedBy.transactionId ? raw(idCell(v.publishedBy.transactionId, 10).s + ':' + esc(String(v.publishedBy.index))) : '—'}</dd>
              <dt>Grace until (DAA)</dt><dd>${v.untilDaa ? fmtInt(v.untilDaa) : '—'}</dd>
              <dt>Usage</dt><dd>${fmtInt(v.attemptClaims)} attempt · ${fmtInt(v.fpClaims)} free-prompt · ${fmtInt(v.workLeaves)} leaves${v.firstUsedDaa ? ' · first DAA ' + fmtInt(v.firstUsedDaa) : ''}${v.lastUsedDaa ? ' · last DAA ' + fmtInt(v.lastUsedDaa) : ''}</dd>
            </dl><dl class="kv">
              <dt>Runtime hash <span class="tag declared">declared</span></dt><dd>${v.runtimeHash ? idCell(v.runtimeHash, 16) : '—'}</dd>
              <dt>Dataset commitment <span class="tag declared">declared</span></dt><dd>${v.datasetCommitment ? idCell(v.datasetCommitment, 16) : '—'}</dd>
              <dt>Training config <span class="tag declared">declared</span></dt><dd>${v.trainingConfigHash ? idCell(v.trainingConfigHash, 16) : '—'}</dd>
              <dt>Notes <span class="tag declared">declared</span></dt><dd>${v.notesHash ? idCell(v.notesHash, 16) : '—'}</dd>
            </dl></div>
            ${(v.evaluations || []).length ? h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th class="l">Evaluator (declared)</th><th>Score ‰ (declared)</th><th class="l">Report</th><th class="l">By</th><th>Posted DAA</th></tr></thead><tbody>${v.evaluations.map((e) => h`<tr><td class="l">${idCell(e.evaluatorId, 12)}</td><td class="num">${e.scorePermille}</td><td class="l">${idCell(e.reportHash, 12)}</td><td class="l">${e.by ? raw(idCell(e.by.transactionId, 10).s + ':' + esc(String(e.by.index))) : '—'} ${e.isLinesOwn ? raw('<span class="tag">line’s own</span>') : raw('<span class="tag">stranger</span>')}</td><td class="num">${fmtInt(e.postedDaa)}</td></tr>`)}</tbody></table></div>` : ''}
          </details>`.s).join(''))}
      </div></div>
      <div class="panel section"><div class="panel-h">Proposals <span class="spacer"></span><span class="dim">open research: a root and a note from any bond, paid when adopted</span></div><div class="panel-b">
        ${rec.proposals == null ? raw('<div class="empty">' + (status.wrpc === 'up' ? 'Loading…' : 'Proposals are served by the wRPC only.') + '</div>') : !rec.proposals.length ? raw('<div class="empty">No proposals recorded on this line.</div>') :
          h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th class="l">Proposal</th><th class="l">Root</th><th class="l">Note (declared)</th><th class="l">By</th><th>Posted DAA</th><th>Adopted in</th></tr></thead><tbody>${rec.proposals.map((p) => h`<tr><td class="l">${idCell(p.proposalId, 12)}</td><td class="l">${idCell(p.root, 12)}</td><td class="l">${idCell(p.noteHash, 12)}</td><td class="l">${p.by ? raw(idCell(p.by.transactionId, 10).s + ':' + esc(String(p.by.index))) : '—'}</td><td class="num">${fmtInt(p.postedDaa)}</td><td class="num">${p.adoptedIn ? 'v' + p.adoptedIn : '—'}</td></tr>`)}</tbody></table></div>`}
      </div></div>`.s;
  }
  await Promise.all([refreshLineInfo(lineId), refreshMarket(lineId), refreshFacade(lineId)]);
  render();
  await Promise.all([refreshUsage(lineId), refreshVersions(lineId), refreshProposals(lineId)]);
  render();
  every(30000, async () => { await refreshMarket(lineId); await refreshLineInfo(lineId); render(); });
}

async function pagePortfolio() {
  const gen = pageState.gen, alive = () => gen === pageState.gen;
  const main = $('#main');
  const view = { positions: null, balance: null, settlements: null, err: null };
  function render() {
    if (!alive()) return;
    const acct = wallet.account;
    if (!acct) { main.innerHTML = h`<div class="page-h"><h1>Portfolio</h1></div><div class="panel"><div class="empty">Connect a wallet to see its positions across lines, its MSK balance and its history. <br><button class="btn btn-accent" id="pfConnect" style="margin-top:10px">Connect</button></div></div>`.s; $('#pfConnect').addEventListener('click', () => $('#connectBtn').click()); return; }
    const p = view.positions;
    let total = 0n, totalKnown = true;
    const rows = (p && p.positions || []).map((x) => { const r = db.line(x.lineId) || db.upsertLine(x.lineId, {}); const m = r.market; const q = m ? curve.sellQuote(m, x.units) : null; if (q) total += q.fees.net; else totalKnown = false; return { x, r, m, q }; });
    main.innerHTML = h`
      <div class="page-h"><h1>Portfolio</h1><span class="sub mono">${acct}</span>${wallet.onChain() ? '' : raw('<span class="tag bad">wallet not on MISAKA</span>')}</div>
      <div class="grid3">
        <div class="tile"><div class="k">MSK balance (EVM account)</div><div class="v">${view.balance != null ? fmtWeiMsk(view.balance) : '—'}</div><div class="s">${status.evm === 'up' ? 'eth_getBalance at latest' : 'EVM RPC unreachable'}</div></div>
        <div class="tile"><div class="k">Positions held</div><div class="v">${p ? p.positions.length : '—'}</div><div class="s">EVM namespace only (a bond's positions are not this account's)</div></div>
        <div class="tile"><div class="k">Mark value (sell all now, net)</div><div class="v">${p && totalKnown ? fmtMsk(total, 2) + ' MSK' : '—'}</div><div class="s">the net MSK the curves would pay right now, fees included</div></div>
      </div>
      <div class="panel section"><div class="panel-h">Positions</div><div class="panel-b">
        ${!p ? raw('<div class="empty">' + (view.err ? esc(view.err) : 'Loading…') + '</div>') : !rows.length ? raw('<div class="empty">No positions in the EVM namespace for this account. <span class="dim">holder id ' + esc(shortId(p.holder)) + '</span></div>') :
          h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th class="l">Line</th><th>Positions (whole)</th><th>Price (MSK)</th><th>Mark value (sell quote, net MSK)</th><th>Average sell price</th><th></th></tr></thead><tbody>${rows.map(({ x, r, m, q }) => h`<tr class="row-link" data-href="#/trade/${x.lineId}"><td class="l"><b>${db.label(r)}</b> <span class="dim tiny mono">${r.symbol}</span></td><td class="num">${fmtPos(x.units)}</td><td class="num">${m && m.price != null ? fmtPrice(m.price) : '—'}</td><td class="num">${q ? fmtMsk(q.fees.net) : '—'}</td><td class="num">${q ? fmtPrice(q.fees.net / x.units) : '—'}</td><td><a href="#/trade/${x.lineId}">Trade</a></td></tr>`)}</tbody></table></div>
          <div class="note tiny">holder id ${shortId(p.holder, 16)} (evm_holder_v1 of chain ${CFG.CHAIN_ID} and this address), read through ${p.source === 'wrpc' ? 'getPalwModelPositions' : 'the position window'}. Mark value is the net MSK the curve's sell quote returns for the whole position right now.</div>`}
      </div></div>
      <div class="panel section"><div class="panel-h">Settlements <span class="spacer"></span><span class="dim">facade events for this account, last ${CFG.LOG_LOOKBACK_BLOCKS} blocks</span></div><div class="panel-b">
        ${db.armed === false ? raw('<div class="empty">The market doors are dormant on this network.</div>') : !view.settlements ? raw('<div class="empty">' + (db.armed ? 'Loading…' : 'EVM RPC unreachable.') + '</div>') : !view.settlements.length ? raw('<div class="empty">No settlements found.</div>') :
          h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th>Block</th><th class="l">Line</th><th class="l">Event</th><th>Positions</th><th>MSK</th><th>Price after</th></tr></thead><tbody>${view.settlements.map((e) => { const r = db.line(e.lineId); return h`<tr><td class="num">${fmtInt(e.blockNumber)}</td><td class="l">${r ? db.label(r) : shortId(e.lineId)}</td><td class="l ${e.kind === 'Bought' ? 'up' : e.kind === 'Sold' ? 'down' : ''}">${e.kind}${e.kind === 'Refused' ? ' (' + (ACTION_NAME[e.actionId] || '') + ': ' + (REFUSAL[e.reason] || e.reason) + ')' : ''}</td><td class="num">${e.units != null ? fmtPos(e.units) : e.kind === 'Seeded' ? '0 (seed)' : '—'}</td><td class="num">${e.kind === 'Bought' ? fmtMsk(e.mskIn) : e.kind === 'Seeded' ? fmtMsk(e.mskIn) + ' locked' : e.kind === 'Sold' ? fmtMsk(e.mskOut) : e.actionId !== 2 ? fmtMsk(e.amount) + ' refunded' : '—'}</td><td class="num">${e.priceAfter != null ? fmtPrice(e.priceAfter) : '—'}</td></tr>`; })}</tbody></table></div>`}
      </div></div>
      <div class="panel section"><div class="panel-h">Order history <span class="spacer"></span><span class="dim">transactions sent from this browser</span></div><div class="panel-b">
        ${(() => { const list = txlog.forAccount(acct); return !list.length ? raw('<div class="empty">Nothing sent from this browser yet.</div>') : h`<div class="tbl-wrap"><table class="tbl"><thead><tr><th class="l">Time</th><th class="l">Line</th><th class="l">Action</th><th>Amount</th><th class="l">Status</th><th class="l">Tx</th></tr></thead><tbody>${list.map((t) => h`<tr><td class="l">${fmtDateTime(t.sentAt)}</td><td class="l">${t.label}</td><td class="l ${t.kind === 'buy' ? 'up' : t.kind === 'sell' ? 'down' : ''}">${t.kind}</td><td class="num">${t.kind === 'sell' ? fmtPos(t.amount) + ' pos' : fmtMsk(t.amount) + ' MSK' + (t.kind === 'seed' ? ' (locked)' : '')}</td><td class="l">${t.status}${t.status === 'settled' ? (t.kind === 'seed' ? ' · seeded, first price ' + fmtPrice(t.priceAfter) + ' MSK' : ' · ' + fmtPos(t.units) + ' pos / ' + fmtMsk(t.msk) + ' MSK') : ''}</td><td class="l">${idCell(t.hash, 10)}</td></tr>`)}</tbody></table></div>`; })()}
      </div></div>`.s;
  }
  async function load() {
    if (!alive()) return;
    render();
    if (!wallet.account) return;
    try { view.balance = await balanceOf(wallet.account); } catch (e) { view.balance = null; }
    try { view.positions = await positionsOf(wallet.account); view.err = null; await pool(view.positions.positions, 3, (p) => refreshMarket(p.lineId)); } catch (e) { view.positions = null; view.err = e.message; }
    render();
    if (db.armed) {
      const ids = new Set(Array.from(db.lines.keys()).slice(0, 12)); if (view.positions) for (const p of view.positions.positions) ids.add(p.lineId);
      const all = await pool(Array.from(ids), 3, (id) => settlementLogs(id, wallet.account));
      view.settlements = all.filter(Boolean).flat().sort((a, b) => b.blockNumber - a.blockNumber || b.logIndex - a.logIndex);
      render();
    }
    await pollTransactions();
  }
  const onWallet = () => { renderNav(); load(); }; wallet.listeners.add(onWallet); onCleanup(() => wallet.listeners.delete(onWallet));
  await discoverClasses(); await discoverLines();
  await load();
  every(15000, load);
}

function pageDocs() {
  const c = db.constsFromChain || CURVE_DEFAULTS;
  const seedMin = c.seedMinSompi || CURVE_DEFAULTS.seedMinSompi, supply = c.supplyUnits || CURVE_DEFAULTS.supplyUnits;
  const first = curve.price(curve.seed(seedMin, { supplyUnits: supply }));
  $('#main').innerHTML = h`<div class="docs">
    <div class="page-h"><h1>How the model market works</h1></div>
    <p>MISAKA Options is a window onto the MISAKA chain's <b>model market</b>. The market lives in the chain's PALW state fold; this site reads it through a node's wRPC and the EVM's read precompiles, and sends seeds and trades through the EVM's writer. Nothing on this site is a number of its own: every figure is an RPC reply or the chain's own curve arithmetic applied to a market row, and a dash means the value is not available.</p>
    <h2>A pair is made by a seed</h2>
    <p>A <b>line</b> is a model in the market's sense: a class (a registered model graph), an owner (a bond, the chain's post-quantum identity) and a name. Every class has a founding line whose id is the class id. A line has <b>no market until someone seeds it</b> (ADR-0090): at least <b>${fmtMsk(seedMin, 0)} MSK</b> is locked into the line's sink, on the EVM through the facade's <code>seed()</code> (or the writer's action 3) or on the UTXO side with <code>misaka palw model-seed</code>. The whole seed becomes the curve's reserve, fee-free. It is <b>locked for good</b>: there is no unseed, no LP token, no admin, and no object ever pays a seed out. The seeder receives <b>no position</b> and nothing back; the seeder's payout payload is kept on the row for the record only. One seed a line: a second is refused. A class still waiting for its activation takes a seed (the pair is made when the model is added, before approval); a frozen class takes none.</p>
    <h2>Positions are whole</h2>
    <p>Every line opens with <b>${fmtPos(supply)} positions</b> in the curve, fixed at the seed, and a position is a <b>whole number</b>: one unit, no fraction (<code>decimals() = 0</code>). A buy that would release less than one position releases nothing and is refused; a sell names whole positions. A position is bought from the protocol's curve and sold back to it and grants nothing but the right to sell it back: no weight, no vote, no fee discount. <b>There is no transfer</b> between holders, on the chain or on the EVM: the facade's <code>transfer</code>, <code>approve</code> and <code>allowance</code> revert, and a contract can never hold a position.</p>
    <h2>The curve</h2>
    <p>Each line's market is a constant-product curve over its real MSK reserve, with <b>no virtual reserve</b>: <code>reserve × positions = K</code>, where <code>K</code> is taken from the row as it stands at every move, so the product can only stay or grow. The price at any moment is <code>reserve / positions in the curve</code>; there is no other price. The first price is <code>seed / ${fmtPos(supply)}</code> (${fmtPrice(first)} MSK at the least seed). A buy adds its net leg to the reserve and releases <code>positions − ⌈K / reserve′⌉</code>; a sell returns positions and pays <code>reserve − ⌈K / positions′⌉</code>, never more than the reserve. Buying raises the price, selling lowers it.</p>
    <p><b>The seed floor invariant.</b> Because the product never falls, with every position back in the curve the reserve is at the seed or above: <b>the reserve never falls under the seed</b>, by any sequence of buys and sells. A holder's sell is the only thing that moves MSK out of the curve, and it cannot reach the seed.</p>
    <h2>Mining buys the pair</h2>
    <p>When a block does PALW work with a model, the worker reward it earns is <b>escrowed</b> and named for its miner only when the block's claim reaches <code>Final</code>. At that moment <b>5 % of that reward buys from the model's pair</b> and stays in it, and the miner is named the other <b>95 %</b> (ADR-0091). The slice takes no fee and gives nobody a position: the positions the curve gives up for it are <b>retired</b> — the chain holds them for good, no object can sell them, and <code>positions in the curve + every holder's + retired = ${fmtPos(supply)}</code>. Nothing is ever distributed to holders; what mining does for a holder is <b>raise the price</b> the curve will pay them when they sell.</p>
    <p>A line with <b>no pair</b> (unseeded), or one closed to buys, pays its miner in full — nothing is burned for a model. Because the slice enters the reserve, the product rises and the price rises even when the slice is worth less than one position, which at the least seed is every ordinary block: on testnet-11's subsidy the escrow is 2.29690373 MSK, the slice 0.11484518 MSK and the miner's 2.18205855 MSK, which lifts a freshly seeded pair from 0.2 to 0.20000022 MSK a position, and about +2.5 % over a month of such blocks.</p>
    <h2>Fees</h2>
    <table><tr><th>on every MSK leg of a buy or a sell</th><th>share</th><th>where it goes</th></tr>
      <tr><td>burn</td><td>5 %</td><td>destroyed; supply only ever falls</td></tr>
      <tr><td>owner leg</td><td>1 %</td><td>the line's owner bond (shared with an adopted contributor when the owner says so); burned for an unowned genesis line</td></tr>
      <tr><td>net</td><td>94 %</td><td>into the reserve on a buy; paid to the seller on a sell</td></tr>
      <tr><td>the seed</td><td>none</td><td>liquidity, not a trade: every sompi of it enters the reserve</td></tr>
      <tr><td>the mining slice</td><td>none</td><td>liquidity too: 5 % of a block's reward, whole, into the reserve — what it buys is retired, not held (ADR-0091)</td></tr></table>
    <p>A round trip therefore costs 12 % plus the curve's slippage. Worked from a market seeded with exactly ${fmtMsk(seedMin, 0)} MSK (first price ${fmtPrice(first)} MSK): a buy of 1,000 MSK burns 50, pays 10 to the owner, puts 940 in the reserve and releases 4,656 positions (price 0.20377757); a second 1,000 MSK buy releases 4,570 more (reserve 101,880); selling all 9,226 back pays 1,879.88976 MSK gross and 1,767.0963744 MSK net, puts 500,000 positions back and leaves the reserve at 100,000.11024 MSK, above the seed. A buy of 0.1 MSK releases nothing; 0.22 MSK releases exactly one position. The <a href="?selftest=1#/docs">self-test</a> checks this site's arithmetic against those golden numbers from the chain's own tests.</p>
    <h2>Adding a model</h2>
    <p>The <a href="#/add">Add model</a> page is the checklist: (1) register the class from a node that holds the artifact (a bond, its key and a fee; not something a browser can do), (2) seed the pair with at least ${fmtMsk(seedMin, 0)} MSK, (3) approval, which is the class reaching <code>Active</code> at its activation DAA and its lanes being certified (ADR-0054, ADR-0075; a clock and a court, not a vote), (4) the line trades: buys move the price. The seed may precede the approval; buys wait for <code>Active</code>.</p>
    <h2>Two doors, one curve</h2>
    <p>A move can be a carrier transaction on the UTXO side signed by an ML-DSA-87 key, or an EVM transaction from an ordinary account. Both reach the same curve and the same fee table. This site uses the EVM door: the line's <b>MRC-20 facade</b> (address <code>0x4d50…</code>, read from the registry) exposes ERC-20's read half plus <code>buy(minUnitsOut)</code> payable, <code>sell(unitsIn, minMskOutSompi)</code> and <code>seed()</code> payable, which route to the writer at <code>0x…F013</code> (actions 1, 2 and 3).</p>
    <h2>Settlement timing</h2>
    <ol><li><b>Emit.</b> Your transaction is included in chain block B. The writer validates the call, escrows a buy's or a seed's value and emits <code>ActionQueued</code>. A seed under the least seed reverts here (<code>SeedTooSmall</code>). Nothing else happens yet.</li>
      <li><b>Apply.</b> The fold applies the action after block B, after every carrier-borne move of B, quoted on the row as it then stands. Your floor (<code>minUnitsOut</code> or <code>minMskOutSompi</code>) is checked: a worse fill is refused, never partial. A seed on an already seeded line, or on a frozen class, is refused.</li>
      <li><b>Settle.</b> In block C, the selected child of B, a system op burns a filled buy's or seed's escrow into the line's sink, refunds a refused one, or credits a filled sell's net MSK to your account. The facade emits <code>Bought</code>, <code>Sold</code>, <code>Seeded</code> or <code>Refused</code> there.</li></ol>
    <p>So a position bought at B is readable, and settled, one chain block later, and a market seeded at B opens one block later. Order history on this site follows exactly that sequence: sent, queued, then settled or refused.</p>
    <h2>Two holder namespaces</h2>
    <p>A carrier-side holder is a bond's payout payload. An EVM account's holder id is <code>evm_holder_v1(chain id, address)</code>, a keyed BLAKE2b-512 that this site reads from the position precompile (or derives locally when the EVM RPC is down). A position bought from the EVM is sold from the EVM; one bought by a carrier is sold by a carrier; nothing moves units between the two. An EVM-held position carries the <code>CLASSICAL-ECC</code> security label: it is guarded by a secp256k1 key, not by the chain's post-quantum domain.</p>
    <h2>What the chain measures, and what it does not</h2>
    <p>Usage counters are the fold's own count of paid inferences per version; they say what was used, never how good it was. Evaluations, dataset commitments and runtime hashes are <b>declarations</b> the chain records, labels by signer and never reads. The leaderboard ranks reserve, seed, positions sold and chain-counted usage only; there is no on-chain quality oracle by design.</p>
    <h2>Units on the wire</h2>
    <p>MSK amounts in the views, quotes and events are in sompi (1 MSK = 10<sup>8</sup> sompi). A buy's or a seed's <code>msg.value</code> is in wei and must be a multiple of 10<sup>10</sup> (1 sompi). Position amounts are whole numbers (<code>decimals() = 0</code>; the market row's <code>positionUnits</code> is a count of positions).</p>
    <h2>Read the design</h2>
    <ul><li><a href="${CFG.ADR_URL}/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md" target="_blank" rel="noopener">ADR-0090: the pair is seeded with real MSK, locked for good, and a position is whole</a></li>
      <li><a href="${CFG.ADR_URL}/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md" target="_blank" rel="noopener">ADR-0091: the reward buys the pair, and no holder is paid</a></li>
      <li><a href="${CFG.ADR_URL}/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md" target="_blank" rel="noopener">ADR-0087: a position is bought from the curve and sold back to it</a></li>
      <li><a href="${CFG.ADR_URL}/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md" target="_blank" rel="noopener">ADR-0088: the class keeps its graph; a line keeps its owner, and the owner keeps publishing</a></li>
      <li><a href="${CFG.ADR_URL}/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md" target="_blank" rel="noopener">ADR-0089: the fold is the truth; the EVM is its window and its hand</a></li>
      <li><a href="https://github.com/MISAKA-BTC/misakas" target="_blank" rel="noopener">github.com/MISAKA-BTC/misakas</a></li></ul>
    <p class="dim tiny">Endpoints in use: wRPC ${wrpc.url || '(not configured)'} · EVM RPC ${evm.url || '(not configured)'} · chain id ${CFG.CHAIN_ID} · network ${CFG.NETWORK_NAME}. Curve constants ${db.constsFromChain ? 'read from the AMM window' : 'are the built-in defaults (the AMM window has not answered)'}.</p>
  </div>`.s;
}

// ============================================================================================
// 11b. the add-model page (ADR-0090 D5): register, seed, approval, trade, with live status
// ============================================================================================
function stepTag(state, text) { return h`<span class="tag ${state === 'ok' ? 'ok' : state === 'warn' ? 'warn' : state === 'bad' ? 'bad' : ''}">${text}</span>`; }
async function pageAdd(arg) {
  const gen = pageState.gen, alive = () => gen === pageState.gen;
  const main = $('#main');
  // #/add/<class id> opens the checklist on that class; otherwise the last one checked in this browser
  const fromHash = normId(arg);
  const st = { id: fromHash || normId(store.get('add:id')) || '', lineId: '', facts: null, factsAt: 0, err: null, loading: false, balance: null, seed: { msk: null, busy: false, balance: null } };
  st.lineId = (fromHash ? fromHash : normId(store.get('add:line'))) || st.id;
  if (fromHash) { store.set('add:id', fromHash); store.set('add:line', fromHash); store.set('add:line:prevClass', fromHash); }
  const docs = String(CFG.DOCS_URL || '').replace(/\/$/, '');
  const adr = String(CFG.ADR_URL || '').replace(/\/$/, '');
  main.innerHTML = h`
    <div class="page-h"><h1>Add a model</h1><span class="sub">from a registered class to a traded pair: register, seed, approval, trade (ADR-0090)</span></div>
    <div class="panel"><div class="panel-b">
      <div class="add-id">
        <label for="addId" class="muted small">Class id (128 hex). The founding line of a class has the class id as its line id, so this is also the line the pair is made on.</label>
        <div class="inp-row"><input id="addId" class="idinp mono" spellcheck="false" autocomplete="off" placeholder="class id, 128 hex" value="${st.id}"><button class="btn btn-accent" id="addCheck">Check</button></div>
        <div class="small dim" id="addKnown"></div>
      </div>
    </div></div>
    <ol class="steps" id="steps"></ol>`.s;

  function known() {
    const list = Array.from(db.classes.keys());
    $('#addKnown').innerHTML = list.length ? raw('Known classes: ' + list.map((id) => { const r = db.line(id); return '<a href="#" data-pick="' + esc(id) + '" class="mono">' + esc(shortId(id, 10)) + '</a>' + (r && r.name ? ' <span class="dim">(' + esc(r.name) + ')</span>' : ''); }).join(' · ')) : 'No class is known yet: configure CLASS_IDS in config.js or reach a node whose registry window is armed.';
    $$('#addKnown [data-pick]').forEach((a) => a.addEventListener('click', (e) => { e.preventDefault(); $('#addId').value = a.dataset.pick; check(); }));
  }
  function stepStates() {
    const f = st.facts, rec = st.lineId ? db.line(st.lineId) : null, m = rec && rec.market;
    const cs = f && f.status;
    const reg = !st.id ? ['', 'enter a class id'] : f == null ? ['', 'checking'] : f.exists === false ? ['bad', 'no class with this id on this chain'] : f.exists ? ['ok', 'registered' + (f.registeredDaa != null ? ' at DAA ' + fmtInt(f.registeredDaa) : '')] : ['', 'the chain did not answer'];
    const seed = !st.lineId ? ['', 'enter a line id'] : rec && rec.notFound ? ['bad', 'no such line'] : !m ? ['', 'reading the market row'] : m.seeded ? ['ok', 'seeded' + (m.seedSompi != null ? ': ' + fmtMsk(m.seedSompi, 0) + ' MSK locked' : '')] : ['warn', 'not seeded'];
    const appr = !st.id ? ['', 'enter a class id'] : !cs ? (f && f.exists === false ? ['bad', 'no class'] : ['', 'status not answered']) : cs.head === 'Active' ? ['ok', 'Active: trading open'] : cs.head === 'Registered' ? ['warn', 'Registered, activates at DAA ' + (cs.activationDaa != null ? fmtInt(cs.activationDaa) : '(not served)') + (db.chain.daa != null && cs.activationDaa != null ? ' (now ' + fmtInt(db.chain.daa) + ')' : '')] : cs.head === 'Frozen' ? ['bad', 'Frozen: no seed, no buys'] : cs.head === 'Dormant' ? ['bad', 'Dormant: register again'] : ['', cs.head];
    const trade = m && m.seeded && cs && cs.head === 'Active' ? ['ok', 'trading'] : m && m.seeded ? ['warn', 'seeded; buys wait for Active'] : ['', 'not yet'];
    return { reg, seed, appr, trade, f, rec, m, cs };
  }
  function renderSteps() {
    if (!alive()) return;
    const s = stepStates();
    const f = s.f, rec = s.rec, m = s.m, cs = s.cs;
    const seedMin = seedMinFor(m);
    const cert = (v) => (v == null ? raw('<span class="dim">—</span>') : v ? raw('<span class="tag ok">certified</span>') : raw('<span class="tag">not certified</span>'));
    const ol = $('#steps');
    ol.innerHTML = h`
      <li class="step">
        <div class="step-h"><span class="n">1</span><b>Register the class</b> ${stepTag(...s.reg)}</div>
        <div class="step-b">
          <p class="small">A class is a registered model graph: its artifact's shape profile and canonical job ride in a <code>ClassRegistered</code> carrier signed by an <b>active bond</b> (ML-DSA-87), with a fee. <b>This step cannot be done from a browser</b>: the node that holds the artifact builds and submits it. The commands, from the runbooks in <code>docs/</code>:</p>
          <pre class="cmd"># 0. a bonded key with funds (docs/testnet11-join-mining.md, section 3); keep the outpoint the node prints
kaspad --testnet --netsuffix=11 --appdir=~/.t11 --palw-register-bond --palw-producer-key=~/.misaka/miner.seed

# 1. pair the artifact with a class and run the admission gate: no node, no key, no coin (docs/palw-model-onboarding-sdk.md)
palw-class inspect   --network ${CFG.NETWORK_NAME} /path/to/artifact.palwart
palw-class preflight --network ${CFG.NETWORK_NAME} /path/to/artifact.palwart --model-id "&lt;model id&gt;"

# 2. register from the node that holds the artifact: ONE ClassRegistered, needs the bond, its key and a funded fee outpoint
kaspad --testnet --netsuffix=11 --appdir=~/.t11 \\
  --palw-class-artifact /path/to/artifact.palwart --palw-register-class "&lt;model id&gt;" \\
  --palw-producer-key=~/.misaka/miner.seed --palw-producer-bond=&lt;txid&gt;:&lt;index&gt; --palw-fee-outpoint=&lt;txid&gt;:&lt;index&gt;</pre>
          <p class="small">The class registers <b>weightless</b> (<code>Registered</code>) at a share of 1 ‰ pending, with an activation DAA (ADR-0056 D6); the class id the node prints is the founding line's id. Its share then follows its own production (ADR-0054). A new architecture, one whose graph reaches a kernel no shipped family drills, needs a build whose court serves it; a new checkpoint of a known lineage is a table row plus the conversion (the onboarding SDK).</p>
          <p class="small dim">Runbooks: <a href="${docs}/palw-model-onboarding-sdk.md" target="_blank" rel="noopener">the model-onboarding SDK</a> · <a href="${docs}/testnet11-join-mining.md" target="_blank" rel="noopener">joining testnet-11 (bond, key, funds)</a> · <a href="${docs}/palw-certify-a-new-model.md" target="_blank" rel="noopener">certifying a new model (ADR-0075)</a> · <a href="${adr}/0056-palw-permissionless-class-admission-and-share-economy.md" target="_blank" rel="noopener">ADR-0056</a> · <a href="${adr}/0054-palw-share-follows-production.md" target="_blank" rel="noopener">ADR-0054</a>.</p>
          <dl class="kv small">
            <dt>Class on this chain</dt><dd>${f == null ? (st.id ? 'checking' : '—') : f.exists === false ? 'no' : f.exists ? 'yes' : '—'}${f && f.source && f.source.length ? raw(' <span class="dim">(' + esc(f.source.join(', ')) + ')</span>') : ''}</dd>
            <dt>Status</dt><dd>${cs ? cs.head : '—'}</dd>
            <dt>Registered at (DAA)</dt><dd>${f && f.registeredDaa != null ? fmtInt(f.registeredDaa) : '—'}</dd>
          </dl>
        </div>
      </li>
      <li class="step">
        <div class="step-h"><span class="n">2</span><b>Seed the pair</b> ${stepTag(...s.seed)}</div>
        <div class="step-b">
          <p class="small">The pair is made by locking at least <b>${fmtMsk(seedMin, 0)} MSK</b> into the line. The whole seed becomes the reserve, fee-free and for good; the seeder gets no position; ${fmtPos(supplyFor(m))} whole positions open in the curve at a first price of seed / ${fmtPos(supplyFor(m))}. A class still waiting for its activation takes a seed; its buys wait for Active. From a bond instead of a wallet: <code>misaka palw model-seed --line &lt;line id&gt; --msk ${fmtScaled(seedMin, 8, 0)} --key-file &lt;seed&gt; --yes</code>.</p>
          <div class="add-id">
            <label for="addLine" class="muted small">Line id to seed (defaults to the class id, the founding line)</label>
            <div class="inp-row"><input id="addLine" class="idinp mono" spellcheck="false" autocomplete="off" placeholder="line id, 128 hex" value="${st.lineId}"><button class="btn" id="addLineUse">Use</button></div>
          </div>
          <div class="seed-slot entry" id="seedSlot"></div>
        </div>
      </li>
      <li class="step">
        <div class="step-h"><span class="n">3</span><b>Approval</b> ${stepTag(...s.appr)}</div>
        <div class="step-b">
          <p class="small">"Approved as a model that earns mining rewards" is the class reaching <b>Active</b> at its activation DAA (a clock: nobody submits it) and its lanes being <b>certified</b> by <code>ClassLaneCertified</code> objects the court grades (ADR-0075; <code>palw-certify drill</code> / <code>bind</code> and <code>misaka palw submit-object</code>, in the runbook above). Nothing on this page can do either. <b>Trading opens when Active</b>; until then a buy is refused (reason 3) and the seed stays. From then on the class also earns: every block it produces escrows a worker reward, and at that claim's <code>Final</code> <b>5 % of it buys from this pair</b> and stays in it (the other 95 % is the miner's) — the price rises with the model's own mining, and nothing is paid out to holders (ADR-0091).</p>
          <dl class="kv small">
            <dt>Class status</dt><dd>${cs ? cs.head + (cs.activationDaa != null ? ' (activates at DAA ' + fmtInt(cs.activationDaa) + ')' : '') + (cs.sinceDaa != null ? ' (since DAA ' + fmtInt(cs.sinceDaa) + ')' : '') : '—'}${cs ? raw(' <span class="dim tiny">' + esc(cs.raw === 'registry classRow' ? 'from the registry window' : 'from the wRPC') + '</span>') : ''}</dd>
            <dt>Chain DAA now</dt><dd>${db.chain.daa != null ? fmtInt(db.chain.daa) : '—'}${cs && cs.activationDaa != null && db.chain.daa != null && cs.activationDaa > db.chain.daa ? ' (' + fmtInt(cs.activationDaa - db.chain.daa) + ' to go)' : ''}</dd>
            <dt>Attempt lane</dt><dd>${cert(f ? f.certifiedAttempt : null)} <span class="dim tiny">${f && f.certifiedAttempt == null ? 'served by the registry window only' : ''}</span></dd>
            <dt>Free-prompt lane</dt><dd>${cert(f ? f.certifiedFp : null)}</dd>
            <dt>Share</dt><dd>${f && f.sharePermille != null ? f.sharePermille + ' ‰' : '—'}</dd>
          </dl>
        </div>
      </li>
      <li class="step">
        <div class="step-h"><span class="n">4</span><b>Trade</b> ${stepTag(...s.trade)}</div>
        <div class="step-b">
          <p class="small">Once seeded, the line appears in <a href="#/lines">Models</a> with its seed and price, and buys move the price up the curve; sells move it down and can never drain the reserve under the seed.${st.lineId ? raw(' <a class="btn btn-sm btn-accent" href="#/trade/' + esc(st.lineId) + '">Trade ' + esc(rec ? db.label(rec) : shortId(st.lineId)) + '</a> <a class="btn btn-sm" href="#/line/' + esc(st.lineId) + '">Line page</a>') : ''}</p>
          <dl class="kv small">
            <dt>Price</dt><dd>${m && m.price != null ? fmtPrice(m.price) + ' MSK per position' : m && !m.seeded ? 'none (not seeded)' : '—'}</dd>
            <dt>Reserve</dt><dd>${m ? fmtMsk(m.mskReserve, 2) + ' MSK' : '—'}</dd>
            <dt>Positions</dt><dd>${m && !m.legacy ? fmtPos(m.supplyUnits - m.positionUnits) + ' sold of ' + fmtPos(m.supplyUnits) : '—'}</dd>
          </dl>
        </div>
      </li>`.s;
    $('#addLineUse').addEventListener('click', () => { const v = normId($('#addLine').value.trim()); if (!v) { toast('A line id is 128 hex characters.', 'warn'); return; } st.lineId = v; store.set('add:line', v); loadLine().then(renderSteps); });
    $('#addLine').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('#addLineUse').click(); });
    renderSeedSlot();
  }
  function renderSeedSlot() {
    const slot = $('#seedSlot'); if (!slot) return;
    const rec = st.lineId ? db.line(st.lineId) : null, m = rec && rec.market;
    if (!st.lineId) { slot.innerHTML = '<div class="empty">Enter a class or line id above.</div>'; return; }
    if (rec && rec.notFound) { slot.innerHTML = h`<div class="empty">The node reports no line ${shortId(st.lineId, 12)} (and no class of that id).</div>`.s; return; }
    if (!m) { slot.innerHTML = '<div class="empty">' + (st.err ? esc(st.err) : 'Reading the market row…') + '</div>'; return; }
    if (m.seeded) { slot.innerHTML = h`<div class="seeded-box"><span class="tag ok">Seeded</span> ${m.seedSompi != null ? fmtMsk(m.seedSompi) + ' MSK locked' : 'reserve ' + fmtMsk(m.mskReserve) + ' MSK'}${m.seededBy ? raw(' by ' + idCell(m.seededBy, 12).s) : ''} at DAA ${fmtInt(m.openedDaa)} · first price was seed / ${fmtPos(m.supplyUnits)}; price now ${m.price != null ? fmtPrice(m.price) : '—'} MSK. One seed a line: a second is refused.</div>`.s; return; }
    st.seed.balance = st.balance;
    renderSeedPanel(slot, st.seed, () => db.line(st.lineId), () => { renderSteps(); });
  }
  async function loadLine() {
    if (!st.lineId) return;
    st.err = null;
    try { await Promise.all([refreshMarket(st.lineId), refreshLineInfo(st.lineId), refreshFacade(st.lineId)]); } catch (e) { st.err = e.message; }
  }
  async function loadFacts() {
    if (!st.id) { st.facts = null; return; }
    try { st.facts = await classFacts(st.id); st.factsAt = Date.now(); } catch (e) { st.facts = { classId: st.id, exists: null, status: null, source: [] }; }
  }
  async function loadBalance() {
    if (!wallet.account) { st.balance = null; return; }
    try { st.balance = await balanceOf(wallet.account); } catch (e) { st.balance = null; }
  }
  async function check() {
    const v = normId($('#addId').value.trim());
    if (!v) { toast('A class id is 128 hex characters.', 'warn'); return; }
    st.id = v; store.set('add:id', v);
    if (!st.lineId || st.lineId === normId(store.get('add:line:prevClass'))) { st.lineId = v; store.set('add:line', v); }
    store.set('add:line:prevClass', v);
    st.facts = null; renderSteps();
    await Promise.all([loadFacts(), loadLine(), loadBalance()]);
    renderSteps();
  }
  $('#addCheck').addEventListener('click', check);
  $('#addId').addEventListener('keydown', (e) => { if (e.key === 'Enter') check(); });
  known();
  renderSteps();
  const onDb = () => { known(); }; db.listeners.add(onDb); onCleanup(() => db.listeners.delete(onDb));
  const onWallet = () => { renderNav(); loadBalance().then(() => { if (!$('#seedAmt:focus')) renderSteps(); }); }; wallet.listeners.add(onWallet); onCleanup(() => wallet.listeners.delete(onWallet));
  if (st.id) { await Promise.all([loadFacts(), loadLine(), loadBalance()]); renderSteps(); }
  every(15000, async () => { await Promise.all([loadFacts(), loadLine(), loadBalance()]); await pollTransactions(); if (!$('#seedAmt:focus') && !$('#addId:focus') && !$('#addLine:focus')) renderSteps(); });
}

// ============================================================================================
// 12. router, boot, self-test
// ============================================================================================
const PAGES = { trade: pageTrade, portfolio: pagePortfolio, lines: pageLines, line: pageLine, leaderboard: pageLeaderboard, add: pageAdd, docs: pageDocs };
async function route() {
  pageState.gen = (pageState.gen || 0) + 1;
  clearPage();
  const { name, arg } = parseHash();
  pageState.name = name; pageState.arg = arg;
  renderNav();
  const page = PAGES[name] || pageTrade;
  try { await page(arg); } catch (e) { console.error(e); $('#main').innerHTML = h`<div class="panel"><div class="empty">This page failed to render: ${e.message}</div></div>`.s; }
  if (SELFTEST) { const box = document.createElement('pre'); box.className = 'selftest'; box.textContent = selfTestReport(); $('#main').prepend(box); }
}

function selfTestReport() {
  const lines = []; let pass = 0, fail = 0;
  const eq = (name, a, b) => { const ok = String(a) === String(b); if (ok) pass++; else fail++; lines.push((ok ? 'ok   ' : 'FAIL ') + name + (ok ? '' : ': got ' + a + ', expected ' + b)); };
  eq('keccak256("")', bytesToHex(keccak256(new Uint8Array(0))), 'c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470');
  eq('selector balanceOf(address)', ABI.selector('balanceOf(address)'), '70a08231');
  eq('selector transfer(address,uint256)', ABI.selector('transfer(address,uint256)'), 'a9059cbb');
  eq('blake2b-512("abc")', bytesToHex(blake2b(utf8('abc'))), 'ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923');
  eq('blake2b-512 keyed("k", "")', bytesToHex(blake2b(new Uint8Array(0), utf8('k'))), 'a393a0e4093eea8bfd03ebe262849654a10fbf67afc7f4f533efc0f992b33cbc574f32066446c2447ef23d5e86fabfd213b9eed79173ee8900909f2da52269cc');
  eq('facade of 1c866e31…', facadeDerived('1c866e31d50411237ce87f710c0defaca7d0bd0f5744ce6e49ce37893c93dc644918a67c22d10e555cb38a5c4f053fd07c90e74b0b4349b639c042698e9140bc'), '0x4d508171946bf26851195b2eb687e764187d0dda');
  eq('evm_holder_v1(0x4D534B, 0x0011…2233)', holderIdDerived('0x4D534B', '0x00112233445566778899aabbccddeeff00112233'), 'a08aa80f963d00cab91278c9b7be8ed347366112aaf3c8cdc85bc9cd417391865e4c0a57001e32a6090d54eb3d815a0df681dc79ee4dc6a8d6f319c9e5d41faf');
  // ---- ADR-0090: the constants, the seed, the goldens of palw_model_market_v1.rs's tests ----
  const MSK = SOMPI_PER_MSK;
  eq('a position is the unit (PALW_MODEL_POSITION_UNITS_V1 = 1)', CURVE_DEFAULTS.unitsPerPosition, 1n);
  eq('supply is 500,000 whole positions', CURVE_DEFAULTS.supplyUnits, 500000n);
  eq('least seed is 100,000 MSK (10,000,000,000,000 sompi)', CURVE_DEFAULTS.seedMinSompi, 10000000000000n);
  const z = curve.unseeded();
  eq('an unseeded row has no price', curve.price(z), null); eq('an unseeded row quotes no buy', curve.buyQuote(z, 1000n * MSK), null); eq('an unseeded row quotes no sell', curve.sellQuote(z, 1n), null);
  const m0 = curve.seed(CURVE_DEFAULTS.seedMinSompi);
  eq('the seed is the reserve, fee-free', m0.mskReserve, 100000n * MSK); eq('500,000 positions in the curve at the seed', m0.positionUnits, 500000n);
  eq('first price = seed / 500,000 = 0.2 MSK', curve.price(m0), 20000000n);
  const b1 = curve.buyQuote(m0, 1000n * MSK);
  eq('buy 1,000 MSK: burn 50', b1.fees.burn, 50n * MSK); eq('buy 1,000 MSK: owner leg 10', b1.fees.leg, 10n * MSK); eq('buy 1,000 MSK: net 940', b1.fees.net, 940n * MSK);
  eq('buy 1,000 MSK: 4,656 whole positions out', b1.unitsOut, 4656n); eq('reserve after: 100,940 MSK', b1.after.mskReserve, 100940n * MSK); eq('price after: 20,377,757 sompi', b1.priceAfter, 20377757n);
  const b2 = curve.buyQuote(b1.after, 1000n * MSK);
  eq('second buy: 4,570 positions out', b2.unitsOut, 4570n); eq('reserve after: 101,880 MSK', b2.after.mskReserve, 101880n * MSK); eq('buying raises the price', b2.priceAfter > b1.priceAfter, true);
  const s = curve.sellQuote(b2.after, b1.unitsOut + b2.unitsOut);
  eq('sell all 9,226: gross 187,988,976,000 sompi', s.fees.gross, 187988976000n); eq('sell all: net 176,709,637,440 sompi', s.fees.net, 176709637440n); eq('sell all: the split is exact', s.fees.burn + s.fees.leg + s.fees.net, s.fees.gross);
  eq('sell all: supply back in the curve', s.after.positionUnits, 500000n); eq('sell all: reserve stays above the seed (100,000.11024 MSK)', s.after.mskReserve, 10000011024000n); eq('selling lowers the price', s.priceAfter < b2.priceAfter, true);
  eq('M2: nothing minted, nothing vanishes', 2000n * MSK + m0.seedSompi, s.after.mskReserve + s.fees.net + s.after.burnedSompi + b1.fees.leg + b2.fees.leg + s.fees.leg);
  let k = curve.k(m0), m = m0, okK = true, okSeed = true;
  for (const x of [1n, 999n, MSK, 37n * MSK, 1000n * MSK, 123456789012n]) { const q = curve.buyQuote(m, x); if (!q) continue; if (curve.k(q.after) < k) okK = false; k = curve.k(q.after); m = q.after; }
  const bought = CURVE_DEFAULTS.supplyUnits - m.positionUnits;
  for (const u of [1n, 12345n, bought / 3n, bought / 2n]) { const q = curve.sellQuote(m, u); if (!q) continue; if (curve.k(q.after) < k) okK = false; if (q.after.mskReserve < m0.seedSompi) okSeed = false; k = curve.k(q.after); m = q.after; }
  const rest = curve.sellQuote(m, CURVE_DEFAULTS.supplyUnits - m.positionUnits);
  eq('P1: the product never falls', okK && curve.k(rest.after) >= k, true);
  eq('P1: the reserve never falls under the seed', okSeed && rest.after.mskReserve >= m0.seedSompi && rest.after.positionUnits === 500000n, true);
  eq('P3: a buy of 0.1 MSK releases nothing', curve.buyQuote(m0, MSK / 10n), null);
  const one = curve.buyQuote(m0, (22n * MSK) / 100n);
  eq('P3: a buy of 0.22 MSK releases exactly one', one && one.unitsOut, 1n);
  const rb = curve.buyQuote(m0, 100n * MSK), rs = curve.sellQuote(rb.after, rb.unitsOut);
  eq('round trip returns at most 0.94² of the gross', rs.fees.net <= (94n * 94n * MSK) / 100n && rs.fees.net > 80n * MSK, true);
  const closed = Object.assign({}, b1.after, { closedToBuys: true });
  eq('a closed market refuses buys', curve.buyQuote(closed, 10n * MSK), null); eq('a closed market honours sells', !!curve.sellQuote(closed, b1.unitsOut), true);
  // ADR-0091 B6: the reward's slice, the miner's rest, and the move's effect on the row.
  eq('B1: five percent of a 229,690,373 sompi escrow is 11,484,518', curve.buybackSlice(229690373n), 11484518n);
  eq('B1: the miner is named the other ninety-five percent', 229690373n - curve.buybackSlice(229690373n), 218205855n);
  const bb1 = curve.buyback(m0, 11484518n);
  eq('B1: the whole slice is reserve, no leg', bb1 && bb1.after.mskReserve, 10000011484518n);
  eq('B1: a slice under one position retires none and still raises the price', bb1 && bb1.retired === 0n && curve.price(bb1.after) === 20000022n, true);
  const bb2 = curve.buyback(m0, 310000000n);
  eq('B5: a 3.1 MSK slice retires 15 positions', bb2 && bb2.retired, 15n);
  eq('B5: M1 counts the retired', bb2 && bb2.after.positionUnits + bb2.after.retiredUnits === CURVE_DEFAULTS.supplyUnits, true);
  eq('B3: a closed pair takes no slice', curve.buyback(Object.assign({}, m0, { closedToBuys: true }), 11484518n), null);
  eq('B3: an unseeded row takes no slice', curve.buyback(curve.unseeded(CURVE_DEFAULTS), 11484518n), null);
  let bbm = m0, bbk = curve.k(m0);
  for (let i = 0; i < 720; i++) { bbm = curve.buyback(bbm, 11484518n).after; }
  eq('B6: a day of blocks lifts the reserve and the price as the ADR says', bbm.mskReserve === 10008268852960n && curve.price(bbm) === 20016537n && curve.k(bbm) > bbk, true);
  const c1 = curve.buyCostForUnits(m0, 1n);
  eq('cost of one position from the seed is minimal', c1 && c1.quote.unitsOut >= 1n && curve.buyQuote(m0, c1.gross - 1n) === null, true);
  const c10 = curve.buyCostForUnits(b2.after, 10n);
  eq('cost of ten positions re-quotes to at least ten', c10 && c10.quote.unitsOut >= 10n && (curve.buyQuote(b2.after, c10.gross - 1n) || { unitsOut: 0n }).unitsOut < 10n, true);
  // ---- the seed on the wire ----
  eq('seed() selector', ABI.selector(SIG.seed), '7d94792a');
  eq('Seeded(address,uint256,uint256) topic', ABI.topic(EVT.Seeded), '0xce9a26839d9b9ac2579f3ab333c5dd44665820297641bfc2cee826856a8f44e3');
  eq('decimals() selector', ABI.selector(SIG.decimals), '313ce567');
  const lid = '1c866e31d50411237ce87f710c0defaca7d0bd0f5744ce6e49ce37893c93dc644918a67c22d10e555cb38a5c4f053fd07c90e74b0b4349b639c042698e9140bc';
  eq('writer action 3 data = 0x01 || 000003 || lineA || lineB', seedActionData(lid), '0x01000003' + lid);
  eq('seed value: 100,000 MSK = 10^23 wei', sompiToWei(CURVE_DEFAULTS.seedMinSompi), 10n ** 23n);
  eq('seed preview: first price at the least seed', seedPreview(CURVE_DEFAULTS.seedMinSompi).price, 20000000n);
  eq('parseClassStatus reads Registered { activation_daa }', JSON.stringify(parseClassStatus('Registered { activation_daa: 118800, pending_share_permille: 1 }'), (kk, v) => (typeof v === 'bigint' ? v.toString() : v)), '{"head":"Registered","activationDaa":"118800","sinceDaa":null,"raw":"Registered { activation_daa: 118800, pending_share_permille: 1 }"}');
  const nz = normMarket({ found: true, opened: false, mskReserve: 0, positionUnits: 500000, soldUnits: 0, priceSompiPerPosition: 0, seedSompi: 0, seededBy: '', seedMinSompi: 10000000000000, classStatus: 'Active' }, 'wrpc');
  eq('normMarket: reserve 0 is not seeded and has no price', !nz.seeded && nz.price === null && nz.seedMinSompi === 10000000000000n, true);
  const ns = normMarket({ found: true, opened: true, openedDaa: 7, mskReserve: 10000000000000, positionUnits: 500000, soldUnits: 0, priceSompiPerPosition: 20000000, seedSompi: 10000000000000, seededBy: lid, classStatus: 'Active' }, 'wrpc');
  eq('normMarket: a seeded row keeps its seed, its seeder and its price', ns.seeded && ns.seedSompi === 10000000000000n && ns.seededBy === lid && ns.price === 20000000n, true);
  eq('safeParse keeps u64 exact', safeParse('{"a":12345678901234567890}').a, '12345678901234567890');
  lines.unshift('MISAKA Options self-test: ' + pass + ' passed, ' + fail + ' failed');
  const report = lines.join('\n');
  console[fail ? 'error' : 'info'](report);
  return report;
}

async function boot() {
  bindNav();
  status.listeners.add(renderNav);
  db.listeners.add(() => { renderNav(); renderBanner(); });
  wallet.listeners.add(renderNav);
  window.addEventListener('hashchange', route);
  window.MO = { curve, ABI, SIG, EVT, blake2b, keccak256, utf8, hexToBytes, bytesToHex, facadeDerived, holderIdDerived, CURVE_DEFAULTS, bi, toHex, SOMPI_PER_MSK, NATIVE_SCALE_WEI, normMarket, db, history, store, txlog, seedActionData, ACTION_SEED, sompiToWei, parseClassStatus, REFUSAL };
  if (MOCK) await new Promise((resolve) => { const s = document.createElement('script'); s.src = 'mock.js'; s.onload = resolve; s.onerror = () => { toast('mock.js failed to load', 'bad'); resolve(); }; document.head.appendChild(s); });
  if (MOCK && window.MISAKA_MOCK && window.MISAKA_MOCK.init) window.MISAKA_MOCK.init(window.MO);
  await wallet.init();
  renderNav();
  await refreshChainInfo();
  await discoverClasses();
  await discoverLines();
  renderBanner();
  await route();
  setInterval(async () => { await refreshChainInfo(); renderBanner(); }, 15000);
  setInterval(() => pollTransactions(), 12000);
}
if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', () => boot().catch((e) => console.error(e)));
else boot().catch((e) => console.error(e));
})();
