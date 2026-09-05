/* mock.js - a simulated node and wallet for MISAKA Options, loaded only with ?mock=1.
 *
 * NOTHING HERE IS A CHAIN FACT. The mock keeps a small world in memory (two classes, three
 * lines, a few holders), prices every trade with the site's own port of the ADR-0087 curve
 * (window.MO.curve, which the self-test pins to the chain's golden numbers), answers the wRPC
 * and EVM JSON-RPC methods the site uses, and plays an EIP-1193 wallet that signs instantly.
 * Blocks tick every 6 seconds; an action sent in block B settles in block B+1, as on the chain.
 */
(() => {
'use strict';
const M = {};
window.MISAKA_MOCK = M;
let MO, C, ABI, SIG, curve;
const T0 = Date.now();
const BLOCK_MS = 6000;
const START_BLOCK = 41200, START_DAA = 118500;
const CHAIN_ID = '0x4d534b';
const ACCOUNT = '0xa11ce4d5f0b2c8e97a3d6f1b2c3d4e5f60718293';
const OTHERS = ['0x0b0b5c1a9d2e3f4a5b6c7d8e9f0a1b2c3d4e5f60', '0x0c4a7e1f2b3c4d5e6f708192a3b4c5d6e7f80912', '0x0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e', '0x0e5f6a7b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5', '0x0f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f70', '0x0a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d'];
const CLASS_A = '4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7';
const CLASS_B = '5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e';

let seedState = 0x9e3779b9;
function rnd() { seedState |= 0; seedState = (seedState + 0x6d2b79f5) | 0; let t = Math.imul(seedState ^ (seedState >>> 15), 1 | seedState); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }
const pick = (arr) => arr[Math.floor(rnd() * arr.length)];
const h128 = (label) => MO.bytesToHex(MO.blake2b(MO.utf8('misaka-options-mock/' + label)));
const h64 = (label) => MO.bytesToHex(MO.blake2b(MO.utf8('misaka-options-mock/' + label))).slice(0, 64);
const hex = (v) => '0x' + BigInt(v).toString(16);
const strip = (s) => String(s || '').replace(/^0x/, '').toLowerCase();
const now = () => Date.now();
const blockOf = (ts) => START_BLOCK + Math.floor((ts - T0) / BLOCK_MS);
const tsOf = (block) => T0 + (block - START_BLOCK) * BLOCK_MS;

const world = { block: START_BLOCK, daa: START_DAA, classes: new Map(), lines: new Map(), markets: new Map(), positions: new Map(), logs: [], receipts: new Map(), pendingTx: [], queued: [], balances: new Map(), logSeq: 0, facades: new Map() };
const key = (line, holder) => line + ':' + holder;
const holderOf = (addr) => MO.holderIdDerived(CHAIN_ID, addr);
function posGet(line, holder) { return world.positions.get(key(line, holder)) || 0n; }
function posAdd(line, holder, d) { world.positions.set(key(line, holder), posGet(line, holder) + d); }

function addLog(block, address, topics, data, txHash) {
  world.logs.push({ blockNumber: hex(block), logIndex: hex(world.logSeq++), transactionHash: txHash || ('0x' + h64('tx/' + block + '/' + world.logSeq)), transactionIndex: '0x0', blockHash: '0x' + h64('block/' + block), address, topics, data, removed: false });
}
function settlementLog(block, line, account, outcome, txHash) {
  const facade = world.facades.get(line);
  const T = { bought: ABI.topic(MO.EVT.Bought), sold: ABI.topic(MO.EVT.Sold), refused: ABI.topic(MO.EVT.Refused) };
  const holder = '0x' + ABI.addrWord(account);
  if (outcome.kind === 'Bought') addLog(block, facade, [T.bought, holder], '0x' + ABI.word(outcome.mskIn) + ABI.word(outcome.units) + ABI.word(outcome.priceAfter), txHash);
  else if (outcome.kind === 'Sold') addLog(block, facade, [T.sold, holder], '0x' + ABI.word(outcome.units) + ABI.word(outcome.mskOut) + ABI.word(outcome.priceAfter), txHash);
  else addLog(block, facade, [T.refused, holder], '0x' + ABI.word(outcome.actionId) + ABI.word(outcome.amount) + ABI.word(outcome.reason), txHash);
}
// apply a buy/sell to a line's market with the site's curve; returns the outcome
function applyBuy(line, account, sompi, minUnits) {
  const m = world.markets.get(line);
  const q = curve.buyQuote(m.row, sompi);
  if (!q) return { kind: 'Refused', actionId: 1, amount: sompi, reason: m.row.closedToBuys ? 3 : 4 };
  if (q.unitsOut < minUnits) return { kind: 'Refused', actionId: 1, amount: sompi, reason: 5 };
  m.row = q.after; m.row.ownerPaid = (m.row.ownerPaid || 0n) + q.fees.leg; if (!m.opened) { m.opened = true; m.openedDaa = world.daa; }
  posAdd(line, holderOf(account), q.unitsOut);
  return { kind: 'Bought', mskIn: sompi, units: q.unitsOut, priceAfter: q.priceAfter };
}
function applySell(line, account, units, minMsk) {
  const m = world.markets.get(line);
  const held = posGet(line, holderOf(account));
  if (units > held) return { kind: 'Refused', actionId: 2, amount: units, reason: 7 };
  const q = curve.sellQuote(m.row, units);
  if (!q) return { kind: 'Refused', actionId: 2, amount: units, reason: 8 };
  if (q.fees.net < minMsk) return { kind: 'Refused', actionId: 2, amount: units, reason: 5 };
  m.row = q.after; m.row.ownerPaid = (m.row.ownerPaid || 0n) + q.fees.leg;
  posAdd(line, holderOf(account), -units);
  return { kind: 'Sold', units, mskOut: q.fees.net, priceAfter: q.priceAfter };
}

function seed() {
  const bond = (label) => ({ transactionId: h128('bond/' + label), index: 0 });
  const payload = (label) => h128('payload/' + label);
  world.classes.set(CLASS_A, { classId: CLASS_A, status: 0, sharePermille: 489, isBase: false, registrant: payload('A'), registeredDaa: 0 });
  world.classes.set(CLASS_B, { classId: CLASS_B, status: 0, sharePermille: 511, isBase: false, registrant: null, registeredDaa: 0 });
  const LINE_C = h128('line/QWEN25-B');
  const mkLine = (id, classId, name, ownerLabel, founded, versions, hasRow) => {
    const row = { lineId: id, classId, hasRow, owner: ownerLabel ? bond(ownerLabel) : null, ownerPayoutPayload: ownerLabel ? payload(ownerLabel) : null, developer: ownerLabel ? bond(ownerLabel + '/dev') : null, developerPayoutPayload: ownerLabel ? payload(ownerLabel + '/dev') : null, maintainer: null, maintainerPayoutPayload: null, name, nameHex: MO.bytesToHex(MO.utf8(name)), foundedDaa: founded, current: versions.find((v) => v.status === 'Current').version, previews: versions.filter((v) => v.status === 'Preview').map((v) => v.version), versionsPublished: versions.length, contributorPermilleOfLeg: 0, status: 'Active', retiredDaa: null };
    world.lines.set(id, { row, versions, proposals: [], evaluations: new Map() });
    world.markets.set(id, { opened: false, openedDaa: 0, row: curve.open() });
    world.facades.set(id, MO.facadeDerived(id));
  };
  const ver = (line, n, status, daa, parent, extra) => Object.assign({ lineId: line, version: n, root: h128('root/' + line + '/' + n), parent, adoptedFrom: null, runtimeHash: h128('rt/' + n), datasetCommitment: null, trainingConfigHash: h128('cfg/' + line + '/' + n), notesHash: null, publishedDaa: daa, publishedBy: bond('A/dev'), status, untilDaa: null, inForce: status !== 'Withdrawn', attemptClaims: 0n, fpClaims: 0n, workLeaves: 0n, firstUsedDaa: daa + 3, lastUsedDaa: daa + 3 }, extra || {});
  mkLine(CLASS_A, CLASS_A, 'Qwen/Qwen2.5-1.5B/graph-v5', 'A', 0, [
    ver(CLASS_A, 1, 'Superseded', 0, null, { untilDaa: 108000, inForce: false, attemptClaims: 1412n, fpClaims: 388n, workLeaves: 21872512n * 388n, lastUsedDaa: 107990 }),
    ver(CLASS_A, 2, 'Current', 104000, 1, { attemptClaims: 3907n, fpClaims: 1266n, workLeaves: 21872512n * 1266n }),
    ver(CLASS_A, 3, 'Preview', 117200, 2, { attemptClaims: 42n, fpClaims: 9n, workLeaves: 21872512n * 9n }),
  ], true);
  mkLine(CLASS_B, CLASS_B, 'Qwen/Qwen3.6-35B-A3B/graph-v3', null, 0, [ver(CLASS_B, 1, 'Current', 0, null, { publishedBy: null, attemptClaims: 2210n, fpClaims: 731n, workLeaves: 33554432n * 731n })], false);
  mkLine(LINE_C, CLASS_A, 'QWEN25-B', 'C', 96000, [ver(LINE_C, 1, 'Superseded', 96000, null, { untilDaa: 116500, attemptClaims: 120n, fpClaims: 31n, workLeaves: 21872512n * 31n }), ver(LINE_C, 2, 'Current', 112500, 1, { attemptClaims: 64n, fpClaims: 12n, workLeaves: 21872512n * 12n, adoptedFrom: h128('proposal/C/1') })], true);
  world.lines.get(LINE_C).row.contributorPermilleOfLeg = 250;
  world.lines.get(LINE_C).proposals.push({ proposalId: h128('proposal/C/1'), lineId: LINE_C, root: h128('root/' + LINE_C + '/2'), noteHash: h128('note/1'), by: bond('P1'), postedDaa: 110900, adoptedIn: 2 }, { proposalId: h128('proposal/C/2'), lineId: LINE_C, root: h128('root/prop/2'), noteHash: h128('note/2'), by: bond('P2'), postedDaa: 117900, adoptedIn: null });
  world.lines.get(LINE_C).evaluations.set(2, [{ evaluatorId: h128('eval/mmlu-ish'), scorePermille: 612, reportHash: h128('report/1'), postedDaa: 113100, by: bond('C/dev'), isLinesOwn: true }, { evaluatorId: h128('eval/stranger'), scorePermille: 540, reportHash: h128('report/2'), postedDaa: 114400, by: bond('S1'), isLinesOwn: false }]);
  world.lines.get(CLASS_A).evaluations.set(2, [{ evaluatorId: h128('eval/harness-x'), scorePermille: 705, reportHash: h128('report/3'), postedDaa: 105000, by: bond('A/dev'), isLinesOwn: true }]);
  // balances: the mock wallet and the background accounts
  world.balances.set(ACCOUNT, 5000n * MO.SOMPI_PER_MSK * MO.NATIVE_SCALE_WEI);
  for (const a of OTHERS) world.balances.set(a, 20000n * MO.SOMPI_PER_MSK * MO.NATIVE_SCALE_WEI);
  // a trade tape over the past ~26 hours, priced by the curve; each fill leaves a settlement log and a price sample
  const tape = [];
  const span = 26 * 3600000;
  const plan = [[CLASS_A, 64, 3n, 45n], [CLASS_B, 26, 20n, 160n], [LINE_C, 9, 1n, 12n]];
  for (const [line, n, lo, hi] of plan) for (let i = 0; i < n; i++) tape.push({ line, ts: T0 - span + Math.floor(rnd() * (span - 120000)), r: rnd(), amt: lo + BigInt(Math.floor(rnd() * Number(hi - lo + 1n))) });
  tape.push({ line: CLASS_A, ts: T0 - 20 * 3600000, r: 0, amt: 60n, who: ACCOUNT }, { line: CLASS_A, ts: T0 - 5 * 3600000, r: 0, amt: 25n, who: ACCOUNT }, { line: LINE_C, ts: T0 - 9 * 3600000, r: 0, amt: 4n, who: ACCOUNT });
  tape.sort((a, b) => a.ts - b.ts);
  for (const t of tape) {
    const block = blockOf(t.ts);
    const who = t.who || pick(OTHERS);
    let out;
    const held = posGet(t.line, holderOf(who));
    if (t.r > 0.68 && held > 0n) { const units = (held * BigInt(10 + Math.floor(rnd() * 40))) / 100n; out = applySell(t.line, who, units, 0n); }
    else out = applyBuy(t.line, who, t.amt * MO.SOMPI_PER_MSK, 0n);
    if (out.kind !== 'Refused') { settlementLog(block, t.line, who, out); MO.history.add(t.line, out.priceAfter, t.ts, 'event'); }
  }
}

// ---- blocks --------------------------------------------------------------------------------
function tickBlock() {
  world.block++; world.daa++;
  const B = world.block;
  // settle what was queued in the previous block (fold(B-1) decided it; EVM(B) carries it)
  for (const a of world.queued.splice(0)) {
    let out;
    if (a.kind === 'buy') { out = applyBuy(a.line, a.from, a.sompi, a.minUnits); if (out.kind === 'Refused') world.balances.set(a.from, world.balances.get(a.from) + a.sompi * MO.NATIVE_SCALE_WEI); }
    else { out = applySell(a.line, a.from, a.units, a.minMsk); if (out.kind === 'Sold') world.balances.set(a.from, world.balances.get(a.from) + out.mskOut * MO.NATIVE_SCALE_WEI); }
    settlementLog(B, a.line, a.from, out);
  }
  // include pending transactions: receipts carry ActionQueued; a buy's value is escrowed now
  for (const tx of world.pendingTx.splice(0)) {
    const bal = world.balances.get(tx.from) || 0n;
    const okValue = tx.kind !== 'buy' || (tx.sompi > 0n && tx.sompi * MO.NATIVE_SCALE_WEI <= bal);
    const okSell = tx.kind !== 'sell' || tx.units > 0n;
    const closed = tx.kind === 'buy' && world.markets.get(tx.line).row.closedToBuys;
    const ok = okValue && okSell && !closed;
    const logs = [];
    if (ok) {
      if (tx.kind === 'buy') world.balances.set(tx.from, bal - tx.sompi * MO.NATIVE_SCALE_WEI);
      addLog(B, '0x000000000000000000000000000000000000f013', [ABI.topic(MO.EVT.ActionQueued), '0x' + ABI.addrWord(tx.from)], '0x' + ABI.word(tx.kind === 'buy' ? 1 : 2) + ABI.word(64) + ABI.word(0), tx.hash);
      logs.push(world.logs[world.logs.length - 1]);
      world.queued.push(tx);
    }
    world.receipts.set(tx.hash, { transactionHash: tx.hash, blockNumber: hex(B), blockHash: '0x' + h64('block/' + B), transactionIndex: '0x0', from: tx.from, to: tx.to, status: ok ? '0x1' : '0x0', gasUsed: '0xb798', cumulativeGasUsed: '0xb798', logs, logsBloom: '0x' + '0'.repeat(512), effectiveGasPrice: '0x3b9aca00', type: '0x2' });
  }
  // background flow from other accounts, so the curve keeps moving
  if (rnd() < 0.6) {
    const line = pick([CLASS_A, CLASS_A, CLASS_B, [...world.lines.keys()][2]]);
    const who = pick(OTHERS);
    const held = posGet(line, holderOf(who));
    const out = rnd() > 0.6 && held > 0n ? applySell(line, who, (held * BigInt(5 + Math.floor(rnd() * 30))) / 100n, 0n) : applyBuy(line, who, BigInt(1 + Math.floor(rnd() * 24)) * MO.SOMPI_PER_MSK, 0n);
    if (out.kind !== 'Refused') settlementLog(B, line, who, out);
  }
  // usage keeps being counted on current versions
  for (const l of world.lines.values()) { const cur = l.versions.find((v) => v.status === 'Current'); if (cur && rnd() < 0.7) { cur.attemptClaims += BigInt(Math.floor(rnd() * 3)); cur.fpClaims += rnd() < 0.4 ? 1n : 0n; cur.lastUsedDaa = world.daa; } }
}

// ---- wRPC --------------------------------------------------------------------------------
const num = (v) => Number(v);
function marketResponse(line) {
  const l = world.lines.get(line), m = world.markets.get(line);
  if (!l || !m) return { found: false, lineId: line };
  const r = m.row;
  return { found: true, lineId: line, opened: m.opened, openedDaa: m.openedDaa, mskReserve: num(r.mskReserve), positionUnits: num(r.positionUnits), soldUnits: num(r.soldUnits), burnedSompi: num(r.burnedSompi), registrantPaidSompi: num(r.ownerPaid || 0n), closedToBuys: !!r.closedToBuys, priceSompiPerPosition: num(curve.price(r)), supplyUnits: num(C.supplyUnits), virtualSompi: num(C.virtualSompi), classStatus: 'Active', contributorPaidSompi: num(r.contributorPaid || 0n) };
}
const rootsInForce = (classId) => { const roots = []; for (const l of world.lines.values()) if (l.row.classId === classId) for (const v of l.versions) if (v.inForce) roots.push(v.root); return roots; };
const versionResponse = (v) => Object.assign({}, v, { attemptClaims: num(v.attemptClaims), fpClaims: num(v.fpClaims), workLeaves: v.workLeaves.toString() });
M.wrpc = async (method, params) => {
  await new Promise((r) => setTimeout(r, 40 + Math.random() * 80));
  switch (method) {
    case 'getBlockDagInfo': return { network: 'testnet-11', blockCount: world.block, headerCount: world.block, tipHashes: ['0x' + h64('tip')], difficulty: 1234567.8, pastMedianTime: now() - 30000, virtualParentHashes: [], pruningPointHash: h64('pp'), virtualDaaScore: world.daa, sink: h64('sink') };
    case 'getInfo': return { p2pId: 'mock', mempoolSize: 0, serverVersion: 'mock-0.0.0', isUtxoIndexed: true, isSynced: true, hasNotifyCommand: true, hasMessageId: true };
    case 'getPalwModelMarket': return marketResponse(strip(params.lineId));
    case 'getPalwModelLines': { const id = strip(params.classId); if (!world.classes.has(id)) return { exists: false, classId: id, lines: [] }; return { exists: true, classId: id, lines: [...world.lines.values()].filter((l) => l.row.classId === id).map((l) => l.row) }; }
    case 'getPalwModelLine': { const id = strip(params.lineId); const l = world.lines.get(id); if (!l) return { exists: false, lineId: id, line: null, currentRoot: null, rootsInForce: [], tipDaa: world.daa }; const cur = l.versions.find((v) => v.status === 'Current'); return { exists: true, lineId: id, line: l.row, currentRoot: cur ? cur.root : null, rootsInForce: rootsInForce(l.row.classId), tipDaa: world.daa }; }
    case 'getPalwModelVersion': { const id = strip(params.lineId); const l = world.lines.get(id); const v = l && l.versions.find((x) => x.version === Number(params.version)); if (!v) return { exists: false, lineId: id, versionNumber: Number(params.version), version: null, evaluations: [], tipDaa: world.daa }; return { exists: true, lineId: id, versionNumber: v.version, version: versionResponse(v), evaluations: l.evaluations.get(v.version) || [], tipDaa: world.daa }; }
    case 'getPalwModelProposals': { const id = strip(params.lineId); const l = world.lines.get(id); return l ? { exists: true, lineId: id, proposals: l.proposals } : { exists: false, lineId: id, proposals: [] }; }
    case 'getPalwModelPositions': { const holder = strip(params.holder); const positions = []; for (const [k, units] of world.positions) { const [line, hld] = k.split(':'); if (hld === holder && units > 0n) positions.push({ lineId: line, units: num(units) }); } return { holder, positions }; }
    default: throw new Error('mock wRPC: unknown method ' + method);
  }
};

// ---- EVM JSON-RPC --------------------------------------------------------------------------
const W = (v) => ABI.word(v);
const out = (...words) => '0x' + words.join('');
const encStr = (s) => { const b = MO.utf8(s); return out(W(32), W(b.length), MO.bytesToHex(b).padEnd(Math.ceil(b.length / 32) * 64, '0')); };
const argWords = (data) => ABI.words('0x' + strip(data).slice(8));
function ethCall(to, data) {
  to = strip(to); const sel = strip(data).slice(0, 8); const a = argWords(data);
  const id2 = (i) => (a[i] || '').padStart(64, '0') + (a[i + 1] || '').padStart(64, '0');
  const S = (name) => ABI.selector(SIG[name]);
  if (to === '000000000000000000000000000000000000f010') {
    const classes = [...world.classes.keys()];
    if (sel === S('chainDaa')) return out(W(world.daa));
    if (sel === S('classCount')) return out(W(classes.length));
    if (sel === S('classAt')) { const c = classes[Number(ABI.u(a[0]))]; return c ? out(c.slice(0, 64), c.slice(64)) : out(W(0), W(0)); }
    if (sel === S('classRow')) { const c = world.classes.get(id2(0)); return c ? out(W(c.status), W(c.sharePermille), W(1000), W(4096), W(c.isBase ? 1 : 0), c.registrant ? c.registrant.slice(0, 64) : W(0), c.registrant ? c.registrant.slice(64) : W(0), W(c.registeredDaa)) : out(...Array(8).fill(W(0))); }
    if (sel === S('lineCount')) return out(W([...world.lines.values()].filter((l) => l.row.hasRow).length));
    if (sel === S('linesOfCount')) return out(W([...world.lines.values()].filter((l) => l.row.classId === id2(0) && l.row.hasRow).length));
    if (sel === S('lineOfClassAt')) { const ls = [...world.lines.values()].filter((l) => l.row.classId === id2(0) && l.row.hasRow); const l = ls[Number(ABI.u(a[2]))]; return l ? out(l.row.lineId.slice(0, 64), l.row.lineId.slice(64)) : out(W(0), W(0)); }
    if (sel === S('line')) { const l = world.lines.get(id2(0)); if (!l) return out(...Array(14).fill(W(0))); const r = l.row; const p = (x) => (x ? [x.slice(0, 64), x.slice(64)] : [W(0), W(0)]); return out(r.classId.slice(0, 64), r.classId.slice(64), ...p(r.ownerPayoutPayload), ...p(r.developerPayoutPayload || r.ownerPayoutPayload), ...p(r.maintainerPayoutPayload || r.ownerPayoutPayload), W(r.current), W(r.versionsPublished), W(r.previews.length), W(r.contributorPermilleOfLeg), W(r.status === 'Retired' ? 1 : 0), MO.bytesToHex(MO.keccak256(MO.utf8(r.name)))); }
    if (sel === S('rootsInForceCount')) return out(W(rootsInForce(id2(0)).length));
    if (sel === S('facadeOf')) { const f = world.facades.get(id2(0)); return out(f ? ABI.addrWord(f) : W(0)); }
    if (sel === S('usage')) { const l = world.lines.get(id2(0)); const v = l && l.versions.find((x) => x.version === Number(ABI.u(a[2]))); return v ? out(W(v.attemptClaims), W(v.fpClaims), W(v.workLeaves), W(v.firstUsedDaa || 0), W(v.lastUsedDaa || 0)) : out(...Array(5).fill(W(0))); }
    return '0x';
  }
  if (to === '000000000000000000000000000000000000f011') {
    if (sel === S('constants')) return out(W(C.supplyUnits), W(C.unitsPerPosition), W(C.virtualSompi), W(C.burnPermille), W(C.legPermille));
    const m = world.markets.get(id2(0));
    if (sel === S('market')) return m ? out(W(m.openedDaa), W(m.row.mskReserve), W(m.row.positionUnits), W(m.row.soldUnits), W(m.row.burnedSompi), W(m.row.ownerPaid || 0n), W(m.row.contributorPaid || 0n), W(m.row.closedToBuys ? 1 : 0), W(m.opened ? 1 : 0)) : out(...Array(9).fill(W(0)));
    if (sel === S('price')) return out(W(m ? curve.price(m.row) : 0));
    if (sel === S('quoteBuy')) { const q = m && curve.buyQuote(m.row, ABI.u(a[2])); return q ? out(W(q.unitsOut), W(q.fees.burn), W(q.fees.leg), W(q.fees.net), W(q.priceAfter)) : out(...Array(5).fill(W(0))); }
    if (sel === S('quoteSell')) { const q = m && curve.sellQuote(m.row, ABI.u(a[2])); return q ? out(W(q.fees.gross), W(q.fees.burn), W(q.fees.leg), W(q.fees.net), W(q.priceAfter)) : out(...Array(5).fill(W(0))); }
    return '0x';
  }
  if (to === '000000000000000000000000000000000000f012') {
    if (sel === S('balanceOfAddress')) return out(W(posGet(id2(0), holderOf(ABI.addr(a[2])))));
    if (sel === S('holderIdOf')) { const hid = holderOf(ABI.addr(a[0])); return out(hid.slice(0, 64), hid.slice(64)); }
    return '0x';
  }
  for (const [line, facade] of world.facades) {
    if (strip(facade) !== to) continue;
    const m = world.markets.get(line);
    if (sel === S('name')) return encStr('MISAKA Model Position ' + line.slice(0, 8));
    if (sel === S('symbol')) return encStr('MP-' + line.slice(0, 8));
    if (sel === ABI.selector('decimals()')) return out(W(6));
    if (sel === ABI.selector('totalSupply()')) return out(W(C.supplyUnits));
    if (sel === S('balanceOf')) return out(W(posGet(line, holderOf(ABI.addr(a[0])))));
    if (sel === ABI.selector('lineId()')) return out(line.slice(0, 64), line.slice(64));
    if (sel === ABI.selector('circulating()')) return out(W(m.row.soldUnits));
    if (sel === ABI.selector('price()')) return out(W(curve.price(m.row)));
    if (sel === ABI.selector('quoteBuy(uint256)')) { const q = curve.buyQuote(m.row, ABI.u(a[0])); return q ? out(W(q.unitsOut), W(q.priceAfter)) : out(W(0), W(0)); }
    if (sel === ABI.selector('quoteSell(uint256)')) { const q = curve.sellQuote(m.row, ABI.u(a[0])); return q ? out(W(q.fees.net), W(q.priceAfter)) : out(W(0), W(0)); }
    return '0x';
  }
  return '0x';
}
function matchTopics(log, filter) {
  if (!filter) return true;
  for (let i = 0; i < filter.length; i++) {
    const want = filter[i]; if (want == null) continue;
    const have = (log.topics[i] || '').toLowerCase();
    const opts = Array.isArray(want) ? want : [want];
    if (!opts.some((o) => String(o).toLowerCase() === have)) return false;
  }
  return true;
}
M.evm = async (method, params) => {
  await new Promise((r) => setTimeout(r, 30 + Math.random() * 60));
  switch (method) {
    case 'eth_chainId': return CHAIN_ID;
    case 'net_version': return String(parseInt(CHAIN_ID, 16));
    case 'eth_blockNumber': return hex(world.block);
    case 'eth_getBalance': return hex(world.balances.get(strip0x(params[0])) || 0n);
    case 'eth_gasPrice': return '0x3b9aca00';
    case 'eth_maxPriorityFeePerGas': return '0x3b9aca00';
    case 'eth_estimateGas': return '0x11170';
    case 'eth_getTransactionCount': return '0x1';
    case 'eth_call': return ethCall(params[0].to, params[0].data);
    case 'eth_getBlockByNumber': { const n = params[0] === 'latest' ? world.block : Number(BigInt(params[0])); if (n > world.block || n < 0) return null; return { number: hex(n), hash: '0x' + h64('block/' + n), parentHash: '0x' + h64('block/' + (n - 1)), timestamp: hex(Math.floor(tsOf(n) / 1000)), transactions: [], gasUsed: '0x0', gasLimit: '0x1c9c380', baseFeePerGas: '0x3b9aca00', miner: '0x' + '0'.repeat(40) }; }
    case 'eth_getTransactionReceipt': return world.receipts.get(String(params[0]).toLowerCase()) || null;
    case 'eth_getLogs': {
      const f = params[0] || {};
      const from = f.fromBlock == null || f.fromBlock === 'earliest' ? 0 : f.fromBlock === 'latest' ? world.block : Number(BigInt(f.fromBlock));
      const to = f.toBlock == null || f.toBlock === 'latest' ? world.block : Number(BigInt(f.toBlock));
      if (to - from > 10000) throw new Error('eth_getLogs block range too large (max 10000 blocks)');
      const addrs = f.address == null ? null : (Array.isArray(f.address) ? f.address : [f.address]).map((x) => strip(x));
      return world.logs.filter((l) => { const n = Number(BigInt(l.blockNumber)); return n >= from && n <= to && (!addrs || addrs.includes(strip(l.address))) && matchTopics(l, f.topics); });
    }
    default: throw new Error('mock EVM: the method ' + method + ' does not exist / is not available');
  }
};
const strip0x = (s) => String(s || '').toLowerCase();

// ---- the wallet (EIP-1193) ------------------------------------------------------------------
const handlers = {};
M.ethereum = {
  isMisakaMock: true,
  on(ev, fn) { (handlers[ev] = handlers[ev] || []).push(fn); },
  removeListener(ev, fn) { handlers[ev] = (handlers[ev] || []).filter((f) => f !== fn); },
  async request({ method, params }) {
    switch (method) {
      case 'eth_requestAccounts': case 'eth_accounts': return [ACCOUNT];
      case 'eth_chainId': return CHAIN_ID;
      case 'wallet_switchEthereumChain': case 'wallet_addEthereumChain': return null;
      case 'eth_sendTransaction': {
        const tx = params[0];
        const to = strip0x(tx.to);
        let line = null; for (const [l, f] of world.facades) if (strip0x(f) === to) line = l;
        if (!line) throw Object.assign(new Error('mock wallet: the recipient is not a line facade'), { code: -32000 });
        const sel = strip(tx.data).slice(0, 8); const a = argWords(tx.data);
        const rec = { hash: '0x' + h64('tx/' + world.block + '/' + Math.random()), from: strip0x(tx.from), to, line, sentAt: now() };
        if (sel === ABI.selector(SIG.buy)) { const wei = BigInt(tx.value || '0x0'); if (wei === 0n || wei % MO.NATIVE_SCALE_WEI !== 0n) throw Object.assign(new Error('execution reverted: BadValue()'), { code: -32000 }); Object.assign(rec, { kind: 'buy', sompi: wei / MO.NATIVE_SCALE_WEI, minUnits: ABI.u(a[0]) }); }
        else if (sel === ABI.selector(SIG.sell)) Object.assign(rec, { kind: 'sell', units: ABI.u(a[0]), minMsk: ABI.u(a[1]) });
        else throw Object.assign(new Error('execution reverted: NonTransferable()'), { code: -32000 });
        await new Promise((r) => setTimeout(r, 600));    // "confirm in wallet"
        world.pendingTx.push(rec);
        return rec.hash;
      }
      default: return M.evm(method, params);
    }
  },
};

M.init = (mo) => {
  MO = mo; C = MO.CURVE_DEFAULTS; ABI = MO.ABI; SIG = MO.SIG; curve = MO.curve;
  seed();
  setInterval(tickBlock, BLOCK_MS);
  console.info('[mock] MISAKA Options mock world ready: ' + world.lines.size + ' lines, ' + world.logs.length + ' settlement logs, account ' + ACCOUNT);
};
})();
