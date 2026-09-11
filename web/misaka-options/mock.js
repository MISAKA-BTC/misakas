/* mock.js - a simulated node and wallet for MISAKA Options, loaded only with ?mock=1.
 *
 * NOTHING HERE IS A CHAIN FACT. The mock keeps a small world in memory (three classes, four
 * lines, a few holders), prices every trade with the site's own port of the ADR-0087/0090 curve
 * (window.MO.curve, which the self-test pins to the chain's golden numbers), answers the wRPC
 * and EVM JSON-RPC methods the site uses, and plays an EIP-1193 wallet that signs instantly.
 * Blocks tick every 6 seconds; an action sent in block B settles in block B+1, as on the chain.
 *
 * ADR-0090 in the mock: three lines are SEEDED with real seeds (the reserve is the seed plus the
 * net legs since), one founding line of a class that is still `Registered` is UNSEEDED so the
 * seed panel and the "Add model" checklist have something to act on; a seed sent from the mock
 * wallet goes through the real code path (ActionQueued in the next block, Seeded or Refused one
 * block later), and the registered class flips to Active at its activation DAA.
 *
 * Orders: the mock keeps an EVM pool with per-account nonces, replacement by (sender, nonce) at a 10 %
 * fee bump (as the node does), cancels (a zero-value transfer to yourself), eth_getTransactionByHash,
 * misaka_getEvmTxStatus and eth_sendRawTransaction. `?mockslow=N` holds a transaction N blocks before a
 * block carries it (testnet-11's wait, compressed); `?mockdrop=1` makes the pool forget each transaction
 * once, two blocks after it arrived, so the page's re-broadcast is exercised. Only MISAKA Wallet signs with
 * the nonce the site names; the other mock wallets pick their own, as real ones may.
 */
(() => {
'use strict';
const M = {};
window.MISAKA_MOCK = M;
let MO, C, ABI, SIG, curve;
const T0 = Date.now();
const BLOCK_MS = 6000;
const TICK_MS = Number(new URLSearchParams(location.search).get('mockblockms')) || BLOCK_MS;   // ?mockblockms=: blocks tick faster (tests); timestamps keep BLOCK_MS
const START_BLOCK = 41200, START_DAA = 118500;
const CHAIN_ID = '0x4d534b';
const ACCOUNT = '0xa11ce4d5f0b2c8e97a3d6f1b2c3d4e5f60718293';
// `?mockwallet=`: the wallets this mock browser has, so the site's wallet chooser can be exercised without
// extensions. Absent (or empty): one plain wallet on window.ethereum that announces nothing, as it always
// was. Otherwise a comma list, announced in that order (EIP-6963, the way the real extensions do it):
//   misaka          MISAKA Wallet (rdns com.misakachain.wallet). It holds MSK on the post-quantum lane and
//                   moves what a payment is short of to the EVM account inside the same confirmation, so
//                   the store's top-up path (`wallet.topsUp`) is what a join with it exercises
//   misaka-legacy   MISAKA Wallet reachable only through its global (window.misaka; here `M.misaka`)
//   phantom         announced as Phantom (app.phantom); it cannot top up
//   metamask        announced as MetaMask (io.metamask); it cannot top up
//   injected        a wallet on window.ethereum (here `M.ethereum`) that announces nothing: "Browser wallet"
//   none            no wallet at all
// e.g. `?mock=1&mockwallet=misaka,phantom,metamask`. Every one of them signs for the same mock account, which
// holds 5 MSK on the EVM lane when MISAKA Wallet is in the list (the rest is "post-quantum"), else 150,000.
const MOCK_QS = new URLSearchParams(location.search);
const SLOW_BLOCKS = Math.max(0, Number(MOCK_QS.get('mockslow') || 0) | 0);
const DROP_ONCE = MOCK_QS.get('mockdrop') === '1';
const MOCK_BASE_FEE = 1000000000n;
const WALLET_KINDS = (MOCK_QS.get('mockwallet') || 'injected').toLowerCase().split(',').map((k) => k.trim()).filter((k) => k && k !== 'none');
const MOCK_MISAKA = WALLET_KINDS.some((k) => k === 'misaka' || k === 'misaka-legacy');
const OTHERS = ['0x0b0b5c1a9d2e3f4a5b6c7d8e9f0a1b2c3d4e5f60', '0x0c4a7e1f2b3c4d5e6f708192a3b4c5d6e7f80912', '0x0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e', '0x0e5f6a7b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5', '0x0f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f70', '0x0a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d'];
const CLASS_A = '4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7';
const CLASS_B = '5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e';
let CLASS_R = null;                 // a class registered on the mock chain and not yet Active (derived at init)
const ACTIVATION_DAA = START_DAA + 300;
const WRITER = '0x000000000000000000000000000000000000f013';

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

const world = { block: START_BLOCK, daa: START_DAA, classes: new Map(), lines: new Map(), markets: new Map(), positions: new Map(), logs: [], receipts: new Map(), pool: new Map(), mined: new Map(), dropped: new Map(), nonces: new Map(), queued: [], balances: new Map(), logSeq: 0, facades: new Map() };
const key = (line, holder) => line + ':' + holder;
const holderOf = (addr) => MO.holderIdDerived(CHAIN_ID, addr);
function posGet(line, holder) { return world.positions.get(key(line, holder)) || 0n; }
function posAdd(line, holder, d) { world.positions.set(key(line, holder), posGet(line, holder) + d); }
const classStatusString = (c) => (c.status === 0 ? 'Active' : c.status === 1 ? 'Frozen { since_daa: ' + c.sinceDaa + ' }' : c.status === 2 ? 'Registered { activation_daa: ' + c.activationDaa + ', pending_share_permille: ' + c.pendingShare + ' }' : 'Dormant { since_daa: ' + c.sinceDaa + ' }');

function addLog(block, address, topics, data, txHash) {
  world.logs.push({ blockNumber: hex(block), logIndex: hex(world.logSeq++), transactionHash: txHash || ('0x' + h64('tx/' + block + '/' + world.logSeq)), transactionIndex: '0x0', blockHash: '0x' + h64('block/' + block), address, topics, data, removed: false });
}
function settlementLog(block, line, account, outcome, txHash) {
  const facade = world.facades.get(line);
  const T = { bought: ABI.topic(MO.EVT.Bought), sold: ABI.topic(MO.EVT.Sold), seeded: ABI.topic(MO.EVT.Seeded), refused: ABI.topic(MO.EVT.Refused) };
  const holder = '0x' + ABI.addrWord(account);
  if (outcome.kind === 'Bought') addLog(block, facade, [T.bought, holder], '0x' + ABI.word(outcome.mskIn) + ABI.word(outcome.units) + ABI.word(outcome.priceAfter), txHash);
  else if (outcome.kind === 'Sold') addLog(block, facade, [T.sold, holder], '0x' + ABI.word(outcome.units) + ABI.word(outcome.mskOut) + ABI.word(outcome.priceAfter), txHash);
  else if (outcome.kind === 'Seeded') addLog(block, facade, [T.seeded, holder], '0x' + ABI.word(outcome.mskIn) + ABI.word(outcome.priceAfter), txHash);
  else addLog(block, facade, [T.refused, holder], '0x' + ABI.word(outcome.actionId) + ABI.word(outcome.amount) + ABI.word(outcome.reason), txHash);
}
// apply a buy/sell/seed to a line's market with the site's curve; returns the outcome (the fold's decision)
function applyBuy(line, account, sompi, minUnits) {
  const m = world.markets.get(line);
  const cls = world.classes.get(world.lines.get(line).row.classId);
  if (!m.seeded) return { kind: 'Refused', actionId: 1, amount: sompi, reason: 6 };
  if (cls.status !== 0) return { kind: 'Refused', actionId: 1, amount: sompi, reason: 3 };
  const q = curve.buyQuote(m.row, sompi);
  if (!q) return { kind: 'Refused', actionId: 1, amount: sompi, reason: m.row.closedToBuys ? 3 : 4 };
  if (q.unitsOut < minUnits) return { kind: 'Refused', actionId: 1, amount: sompi, reason: 5 };
  m.row = q.after; m.row.ownerPaid = (m.row.ownerPaid || 0n) + q.fees.leg;
  posAdd(line, holderOf(account), q.unitsOut);
  return { kind: 'Bought', mskIn: sompi, units: q.unitsOut, priceAfter: q.priceAfter };
}
function applySell(line, account, units, minMsk) {
  const m = world.markets.get(line);
  if (!m.seeded) return { kind: 'Refused', actionId: 2, amount: units, reason: 6 };
  const held = posGet(line, holderOf(account));
  if (units > held) return { kind: 'Refused', actionId: 2, amount: units, reason: 7 };
  const q = curve.sellQuote(m.row, units);
  if (!q) return { kind: 'Refused', actionId: 2, amount: units, reason: 8 };
  if (q.fees.net < minMsk) return { kind: 'Refused', actionId: 2, amount: units, reason: 5 };
  m.row = q.after; m.row.ownerPaid = (m.row.ownerPaid || 0n) + q.fees.leg;
  posAdd(line, holderOf(account), -units);
  return { kind: 'Sold', units, mskOut: q.fees.net, priceAfter: q.priceAfter };
}
// ADR-0090: one seed a line, at least the floor (the writer already reverted under it), none on a frozen class
function applySeed(line, account, sompi) {
  const m = world.markets.get(line);
  const cls = world.classes.get(world.lines.get(line).row.classId);
  if (m.seeded) return { kind: 'Refused', actionId: 3, amount: sompi, reason: 10 };
  if (sompi < C.seedMinSompi) return { kind: 'Refused', actionId: 3, amount: sompi, reason: 11 };
  if (cls.status === 1) return { kind: 'Refused', actionId: 3, amount: sompi, reason: 12 };
  m.row = curve.seed(sompi, C, holderOf(account), world.daa); m.seeded = true; m.openedDaa = world.daa;
  return { kind: 'Seeded', mskIn: sompi, priceAfter: curve.price(m.row) };
}

function seedWorld() {
  const bond = (label) => ({ transactionId: h128('bond/' + label), index: 0 });
  const payload = (label) => h128('payload/' + label);
  CLASS_R = h128('class/registered');
  world.classes.set(CLASS_A, { classId: CLASS_A, status: 0, sharePermille: 489, isBase: false, registrant: payload('A'), registeredDaa: 0, certifiedAttempt: true, certifiedFp: true });
  world.classes.set(CLASS_B, { classId: CLASS_B, status: 0, sharePermille: 510, isBase: false, registrant: null, registeredDaa: 0, certifiedAttempt: true, certifiedFp: true });
  world.classes.set(CLASS_R, { classId: CLASS_R, status: 2, activationDaa: ACTIVATION_DAA, pendingShare: 1, sharePermille: 0, isBase: false, registrant: payload('R'), registeredDaa: START_DAA - 700, certifiedAttempt: false, certifiedFp: false });
  const LINE_C = h128('line/QWEN25-B');
  const mkLine = (id, classId, name, ownerLabel, founded, versions, hasRow, seedMsk, seederLabel) => {
    const row = { lineId: id, classId, hasRow, owner: ownerLabel ? bond(ownerLabel) : null, ownerPayoutPayload: ownerLabel ? payload(ownerLabel) : null, developer: ownerLabel ? bond(ownerLabel + '/dev') : null, developerPayoutPayload: ownerLabel ? payload(ownerLabel + '/dev') : null, maintainer: null, maintainerPayoutPayload: null, name, nameHex: MO.bytesToHex(MO.utf8(name)), foundedDaa: founded, current: versions.find((v) => v.status === 'Current').version, previews: versions.filter((v) => v.status === 'Preview').map((v) => v.version), versionsPublished: versions.length, contributorPermilleOfLeg: 0, status: 'Active', retiredDaa: null };
    world.lines.set(id, { row, versions, proposals: [], evaluations: new Map() });
    if (seedMsk) world.markets.set(id, { seeded: true, openedDaa: founded + 40, row: curve.seed(seedMsk * MO.SOMPI_PER_MSK, C, payload(seederLabel), founded + 40) });
    else world.markets.set(id, { seeded: false, openedDaa: 0, row: curve.unseeded(C) });
    world.facades.set(id, MO.facadeDerived(id));
  };
  const ver = (line, n, status, daa, parent, extra) => Object.assign({ lineId: line, version: n, root: h128('root/' + line + '/' + n), parent, adoptedFrom: null, runtimeHash: h128('rt/' + n), datasetCommitment: null, trainingConfigHash: h128('cfg/' + line + '/' + n), notesHash: null, publishedDaa: daa, publishedBy: bond('A/dev'), status, untilDaa: null, inForce: status !== 'Withdrawn', attemptClaims: 0n, fpClaims: 0n, workLeaves: 0n, firstUsedDaa: daa + 3, lastUsedDaa: daa + 3 }, extra || {});
  mkLine(CLASS_A, CLASS_A, 'Qwen/Qwen2.5-1.5B/graph-v5', 'A', 0, [
    ver(CLASS_A, 1, 'Superseded', 0, null, { untilDaa: 108000, inForce: false, attemptClaims: 1412n, fpClaims: 388n, workLeaves: 21872512n * 388n, lastUsedDaa: 107990 }),
    ver(CLASS_A, 2, 'Current', 104000, 1, { attemptClaims: 3907n, fpClaims: 1266n, workLeaves: 21872512n * 1266n }),
    ver(CLASS_A, 3, 'Preview', 117200, 2, { attemptClaims: 42n, fpClaims: 9n, workLeaves: 21872512n * 9n }),
  ], true, 250000n, 'A');
  mkLine(CLASS_B, CLASS_B, 'Qwen/Qwen3.6-35B-A3B/graph-v3', null, 0, [ver(CLASS_B, 1, 'Current', 0, null, { publishedBy: null, attemptClaims: 2210n, fpClaims: 731n, workLeaves: 33554432n * 731n })], false, 100000n, 'S');
  mkLine(LINE_C, CLASS_A, 'QWEN25-B', 'C', 96000, [ver(LINE_C, 1, 'Superseded', 96000, null, { untilDaa: 116500, attemptClaims: 120n, fpClaims: 31n, workLeaves: 21872512n * 31n }), ver(LINE_C, 2, 'Current', 112500, 1, { attemptClaims: 64n, fpClaims: 12n, workLeaves: 21872512n * 12n, adoptedFrom: h128('proposal/C/1') })], true, 120000n, 'C');
  // the registered class's founding line: no row yet, no seed yet (the Add model page's subject)
  mkLine(CLASS_R, CLASS_R, 'Example/NewModel-3B/graph-v1', 'R', START_DAA - 700, [ver(CLASS_R, 1, 'Current', START_DAA - 700, null, { publishedBy: bond('R/dev'), firstUsedDaa: 0, lastUsedDaa: 0 })], false, null, null);
  world.lines.get(LINE_C).row.contributorPermilleOfLeg = 250;
  world.lines.get(LINE_C).proposals.push({ proposalId: h128('proposal/C/1'), lineId: LINE_C, root: h128('root/' + LINE_C + '/2'), noteHash: h128('note/1'), by: bond('P1'), postedDaa: 110900, adoptedIn: 2 }, { proposalId: h128('proposal/C/2'), lineId: LINE_C, root: h128('root/prop/2'), noteHash: h128('note/2'), by: bond('P2'), postedDaa: 117900, adoptedIn: null });
  world.lines.get(LINE_C).evaluations.set(2, [{ evaluatorId: h128('eval/mmlu-ish'), scorePermille: 612, reportHash: h128('report/1'), postedDaa: 113100, by: bond('C/dev'), isLinesOwn: true }, { evaluatorId: h128('eval/stranger'), scorePermille: 540, reportHash: h128('report/2'), postedDaa: 114400, by: bond('S1'), isLinesOwn: false }]);
  world.lines.get(CLASS_A).evaluations.set(2, [{ evaluatorId: h128('eval/harness-x'), scorePermille: 705, reportHash: h128('report/3'), postedDaa: 105000, by: bond('A/dev'), isLinesOwn: true }]);
  // balances: the mock wallet holds enough to seed a line (150,000 MSK) and the background accounts trade
  world.balances.set(ACCOUNT, (MOCK_MISAKA ? 5n : 150000n) * MO.SOMPI_PER_MSK * MO.NATIVE_SCALE_WEI);
  for (const a of OTHERS) world.balances.set(a, 60000n * MO.SOMPI_PER_MSK * MO.NATIVE_SCALE_WEI);
  // ADR-0091: every block these Active classes produced since the seed put 5 % of its worker
  // reward into the pair. Applied through the same move the fold performs, so the row stays
  // consistent (reserve, product, retired, M1) — a few thousand blocks' worth, per line's share.
  for (const [id, m] of world.markets) {
    if (!m.seeded) continue;
    const blocks = id === CLASS_A ? 5200 : id === CLASS_B ? 5400 : 900;
    for (let i = 0; i < blocks; i++) { const q = curve.buyback(m.row, curve.buybackSlice(MOCK_ESCROW_SOMPI, C)); if (q) m.row = q.after; }
  }
  // a trade tape over the past ~26 hours on the seeded lines, priced by the curve; each fill leaves a settlement log and a price sample
  const tape = [];
  const span = 26 * 3600000;
  const plan = [[CLASS_A, 64, 20n, 400n], [CLASS_B, 26, 50n, 800n], [LINE_C, 9, 10n, 120n]];
  for (const [line, n, lo, hi] of plan) for (let i = 0; i < n; i++) tape.push({ line, ts: T0 - span + Math.floor(rnd() * (span - 120000)), r: rnd(), amt: lo + BigInt(Math.floor(rnd() * Number(hi - lo + 1n))) });
  tape.push({ line: CLASS_A, ts: T0 - 20 * 3600000, r: 0, amt: 600n, who: ACCOUNT }, { line: CLASS_A, ts: T0 - 5 * 3600000, r: 0, amt: 250n, who: ACCOUNT }, { line: LINE_C, ts: T0 - 9 * 3600000, r: 0, amt: 40n, who: ACCOUNT });
  tape.sort((a, b) => a.ts - b.ts);
  for (const t of tape) {
    const block = blockOf(t.ts);
    const who = t.who || pick(OTHERS);
    let out;
    const held = posGet(t.line, holderOf(who));
    if (t.r > 0.68 && held > 0n) { const units = (held * BigInt(10 + Math.floor(rnd() * 40))) / 100n; out = units > 0n ? applySell(t.line, who, units, 0n) : { kind: 'Refused' }; }
    else out = applyBuy(t.line, who, t.amt * MO.SOMPI_PER_MSK, 0n);
    if (out.kind !== 'Refused') { settlementLog(block, t.line, who, out); MO.history.add(t.line, out.priceAfter, t.ts, 'event'); }
  }
  // the seeds themselves, as Seeded events at their opening blocks (before the tape)
  for (const [id, m] of world.markets) if (m.seeded) { const b = START_BLOCK - Math.floor(span / BLOCK_MS) - 20; settlementLog(b, id, OTHERS[0], { kind: 'Seeded', mskIn: m.row.seedSompi, priceAfter: m.row.seedSompi / C.supplyUnits }); }
  world.logs.sort((a, b) => Number(BigInt(a.blockNumber)) - Number(BigInt(b.blockNumber)));
}

// The escrowed worker reward of one block on testnet-11's pre-deflationary subsidy (370,468,345
// sompi at a 620 permille carve) — what ADR-0091 takes its five percent of.
const MOCK_ESCROW_SOMPI = 229690373n;

// ---- blocks --------------------------------------------------------------------------------
function tickBlock() {
  world.block++; world.daa++;
  const B = world.block;
  // the registered class activates at its DAA (a clock, not an object)
  for (const c of world.classes.values()) if (c.status === 2 && world.daa >= c.activationDaa) { c.status = 0; c.sharePermille = c.pendingShare; }
  // ADR-0091: a block of an Active class escrows a worker reward and, at its Final, 5 % of it
  // buys from that line's pair — the other 95 % is the miner's, and no holder is paid.
  for (const [id, m] of world.markets) {
    const cls = world.classes.get(world.lines.get(id).row.classId);
    if (!m.seeded || !cls || cls.status !== 0) continue;
    const q = curve.buyback(m.row, curve.buybackSlice(MOCK_ESCROW_SOMPI, C));
    if (q) m.row = q.after;
  }
  // settle what was queued in the previous block (fold(B-1) decided it; EVM(B) carries it)
  for (const a of world.queued.splice(0)) {
    let out;
    if (a.kind === 'buy') { out = applyBuy(a.line, a.from, a.sompi, a.minUnits); if (out.kind === 'Refused') world.balances.set(a.from, world.balances.get(a.from) + a.sompi * MO.NATIVE_SCALE_WEI); }
    else if (a.kind === 'seed') { out = applySeed(a.line, a.from, a.sompi); if (out.kind === 'Refused') world.balances.set(a.from, world.balances.get(a.from) + a.sompi * MO.NATIVE_SCALE_WEI); }
    else { out = applySell(a.line, a.from, a.units, a.minMsk); if (out.kind === 'Sold') world.balances.set(a.from, world.balances.get(a.from) + out.mskOut * MO.NATIVE_SCALE_WEI); }
    settlementLog(B, a.line, a.from, out);
  }
  // the pool forgets (?mockdrop=1): each transaction once, two blocks after it arrived
  if (DROP_ONCE) for (const tx of Array.from(world.pool.values())) if (!tx.droppedOnce && B - tx.addedBlock >= 2) { tx.droppedOnce = true; world.pool.delete(tx.hash); world.dropped.set(tx.hash, tx); }
  // include pending transactions: per sender the contiguous run from its nonce, each at least
  // SLOW_BLOCKS old; receipts carry ActionQueued; a buy's or a seed's value is escrowed now
  const carried = [];
  for (const tx of Array.from(world.pool.values()).sort((a, b) => a.nonce - b.nonce)) {
    if (B - tx.addedBlock <= SLOW_BLOCKS || tx.nonce !== (world.nonces.get(tx.from) || 0)) continue;
    world.pool.delete(tx.hash); world.nonces.set(tx.from, tx.nonce + 1); carried.push(tx);
  }
  for (const tx of carried) {
    world.mined.set(tx.hash, Object.assign(tx, { blockNumber: B }));
    if (tx.kind === 'cancel') { world.receipts.set(tx.hash, { transactionHash: tx.hash, blockNumber: hex(B), blockHash: '0x' + h64('block/' + B), transactionIndex: '0x0', from: tx.from, to: tx.from, status: '0x1', gasUsed: '0x5208', cumulativeGasUsed: '0x5208', logs: [], logsBloom: '0x' + '0'.repeat(512), effectiveGasPrice: hex(MOCK_BASE_FEE), type: '0x2' }); continue; }
    const bal = world.balances.get(tx.from) || 0n;
    const escrow = tx.kind === 'buy' || tx.kind === 'seed';
    const okValue = !escrow || (tx.sompi > 0n && tx.sompi * MO.NATIVE_SCALE_WEI <= bal);
    const okSell = tx.kind !== 'sell' || tx.units > 0n;
    const closed = tx.kind === 'buy' && world.markets.get(tx.line).row.closedToBuys;
    const ok = okValue && okSell && !closed;
    const logs = [];
    if (ok) {
      if (escrow) world.balances.set(tx.from, bal - tx.sompi * MO.NATIVE_SCALE_WEI);
      const actionId = tx.kind === 'buy' ? 1 : tx.kind === 'sell' ? 2 : 3;
      addLog(B, WRITER, [ABI.topic(MO.EVT.ActionQueued), '0x' + ABI.addrWord(tx.from)], '0x' + ABI.word(actionId) + ABI.word(64) + ABI.word(0), tx.hash);
      logs.push(world.logs[world.logs.length - 1]);
      world.queued.push(tx);
    }
    world.receipts.set(tx.hash, { transactionHash: tx.hash, blockNumber: hex(B), blockHash: '0x' + h64('block/' + B), transactionIndex: '0x0', from: tx.from, to: tx.to, status: ok ? '0x1' : '0x0', gasUsed: '0xb798', cumulativeGasUsed: '0xb798', logs, logsBloom: '0x' + '0'.repeat(512), effectiveGasPrice: '0x3b9aca00', type: '0x2' });
  }
  // background flow from other accounts on the seeded, active lines, so the curve keeps moving
  if (rnd() < 0.6) {
    const candidates = [...world.markets].filter(([id, m]) => m.seeded && world.classes.get(world.lines.get(id).row.classId).status === 0).map(([id]) => id);
    if (candidates.length) {
      const line = pick(candidates);
      const who = pick(OTHERS);
      const held = posGet(line, holderOf(who));
      const sellUnits = (held * BigInt(5 + Math.floor(rnd() * 30))) / 100n;
      const out = rnd() > 0.6 && sellUnits > 0n ? applySell(line, who, sellUnits, 0n) : applyBuy(line, who, BigInt(5 + Math.floor(rnd() * 120)) * MO.SOMPI_PER_MSK, 0n);
      if (out.kind !== 'Refused') settlementLog(B, line, who, out);
    }
  }
  // usage keeps being counted on current versions of active classes
  for (const l of world.lines.values()) { const cur = l.versions.find((v) => v.status === 'Current'); if (cur && world.classes.get(l.row.classId).status === 0 && rnd() < 0.7) { cur.attemptClaims += BigInt(Math.floor(rnd() * 3)); cur.fpClaims += rnd() < 0.4 ? 1n : 0n; cur.lastUsedDaa = world.daa; } }
}

// ---- wRPC --------------------------------------------------------------------------------
const num = (v) => Number(v);
function marketResponse(line) {
  const l = world.lines.get(line), m = world.markets.get(line);
  if (!l || !m) return { found: false, lineId: line };
  const r = m.row, cls = world.classes.get(l.row.classId);
  const price = curve.price(r);
  return { found: true, lineId: line, opened: m.seeded, openedDaa: m.openedDaa, mskReserve: num(r.mskReserve), positionUnits: num(r.positionUnits), soldUnits: num(r.soldUnits), burnedSompi: num(r.burnedSompi), registrantPaidSompi: num(r.ownerPaid || 0n), closedToBuys: !!r.closedToBuys || cls.status !== 0, priceSompiPerPosition: price == null ? 0 : num(price), supplyUnits: num(C.supplyUnits), virtualSompi: 0, classStatus: classStatusString(cls), contributorPaidSompi: num(r.contributorPaid || 0n), seedSompi: m.seeded ? num(r.seedSompi) : 0, seededBy: m.seeded ? r.seededBy : '', seedMinSompi: num(C.seedMinSompi), buybackSompi: num(r.buybackSompi || 0n), retiredUnits: num(r.retiredUnits || 0n) };
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
    case 'getPalwProducerFacts': { const id = strip(params.classId); const c = world.classes.get(id); if (!c) return { available: false, classId: id, fpCertified: false, isBaseClass: false }; return { available: true, chainPoint: h64('tip'), daaScore: world.daa, classId: id, artifactRoot: h128('root/' + id + '/1'), classTarget: '0', pwu: 1, isBaseClass: false, minTraceRetentionDaa: 6000, epochIndex: Math.floor(world.daa / 1000), epochBudgetBlocks: 100, epochProducedBlocks: 12, bondKnown: false, fpCertified: !!c.certifiedFp, fpQuantaPerCanonicalJob: 8, fpMaxQuantaPerReceipt: 64 }; }
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
    if (sel === S('certified')) { const c = world.classes.get(id2(0)); const lane = Number(ABI.u(a[2])); return out(W(c ? (lane === 0 ? c.certifiedAttempt : lane === 1 ? c.certifiedFp : false) ? 1 : 0 : 0)); }
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
    // ADR-0090: the third word of constants() is the least seed (it carried the virtual reserve before)
    if (sel === S('constants')) return out(W(C.supplyUnits), W(C.unitsPerPosition), W(C.seedMinSompi), W(C.burnPermille), W(C.legPermille));
    const m = world.markets.get(id2(0));
    if (sel === S('market')) return m ? out(W(m.openedDaa), W(m.row.mskReserve), W(m.row.positionUnits), W(m.row.soldUnits), W(m.row.burnedSompi), W(m.row.ownerPaid || 0n), W(m.row.contributorPaid || 0n), W(m.row.closedToBuys ? 1 : 0), W(m.seeded ? 1 : 0), W(m.row.buybackSompi || 0n), W(m.row.retiredUnits || 0n)) : out(...Array(11).fill(W(0)));
    if (sel === S('price')) { const p = m ? curve.price(m.row) : null; return out(W(p == null ? 0 : p)); }
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
    if (sel === ABI.selector('decimals()')) return out(W(0));            // ADR-0090: a position is whole
    if (sel === ABI.selector('totalSupply()')) return out(W(C.supplyUnits));
    if (sel === S('balanceOf')) return out(W(posGet(line, holderOf(ABI.addr(a[0])))));
    if (sel === ABI.selector('lineId()')) return out(line.slice(0, 64), line.slice(64));
    if (sel === ABI.selector('circulating()')) return out(W(m.row.soldUnits));
    if (sel === ABI.selector('price()')) { const p = curve.price(m.row); return out(W(p == null ? 0 : p)); }
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
    case 'eth_getTransactionCount': { const a = strip0x(params[0]); return hex(params[1] === 'pending' ? pendingNonce(a) : (world.nonces.get(a) || 0)); }
    case 'eth_getTransactionByHash': { const hs = String(params[0]).toLowerCase(); const t = world.pool.get(hs) || world.mined.get(hs); return t ? rpcTxOf(t) : null; }
    case 'misaka_getEvmTxStatus': {
      const hs = String(params[0]).toLowerCase(), m = world.mined.get(hs);
      if (world.pool.has(hs)) return { transactionHash: hs, state: 'pending', inMempool: true, includedIn: [], acceptedIn: null, orphanedAcceptances: [], lastSkipClass: null };
      if (m) return { transactionHash: hs, state: 'accepted', inMempool: false, includedIn: ['0x' + h64('block/' + m.blockNumber)], acceptedIn: { block: '0x' + h64('block/' + m.blockNumber), receiptIndex: 0 }, orphanedAcceptances: [], lastSkipClass: null };
      return { transactionHash: hs, state: 'unknown', inMempool: false, includedIn: [], acceptedIn: null, orphanedAcceptances: [], lastSkipClass: null };
    }
    case 'eth_sendRawTransaction': {
      const bytes = MO.hexToBytes(params[0]); const hs = '0x' + MO.bytesToHex(MO.keccak256(bytes));
      if (world.pool.has(hs)) throw Object.assign(new Error('already known'), { code: -32000 });
      if (world.mined.has(hs)) throw Object.assign(new Error('nonce too low'), { code: -32000 });
      const t = world.dropped.get(hs);
      if (!t) throw Object.assign(new Error('mock EVM: only a transaction this mock signed can be re-broadcast'), { code: -32000 });
      world.dropped.delete(hs); t.addedBlock = world.block; world.pool.set(hs, t);
      return hs;
    }
    case 'eth_call': return ethCall(params[0].to, params[0].data);
    case 'eth_getBlockByNumber': { const n = params[0] === 'latest' ? world.block : Number(BigInt(params[0])); if (n > world.block || n < 0) return null; return { number: hex(n), hash: '0x' + h64('block/' + n), parentHash: '0x' + h64('block/' + (n - 1)), timestamp: hex(Math.floor(tsOf(n) / 1000)), transactions: [], gasUsed: '0x0', gasLimit: '0x1c9c380', baseFeePerGas: hex(MOCK_BASE_FEE), miner: '0x' + '0'.repeat(40) }; }
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
function pendingNonce(from) {
  let n = world.nonces.get(from) || 0;
  while (Array.from(world.pool.values()).some((x) => x.from === from && x.nonce === n)) n++;
  return n;
}
// signed-looking fields, hashed the way a node hashes a signed transaction (keccak of the EIP-2718 bytes)
function rpcTxOf(t) {
  return { type: '0x2', chainId: CHAIN_ID, nonce: hex(t.nonce), maxPriorityFeePerGas: hex(t.maxPriorityFeePerGas), maxFeePerGas: hex(t.maxFeePerGas), gas: hex(t.gas), to: t.to, value: hex(t.value), input: t.input, accessList: [], v: '0x0', yParity: '0x0', r: t.r, s: t.s, from: t.from, hash: t.hash, blockNumber: t.blockNumber != null ? hex(t.blockNumber) : null, blockHash: t.blockNumber != null ? '0x' + h64('block/' + t.blockNumber) : null, transactionIndex: t.blockNumber != null ? '0x0' : null, gasPrice: hex(t.maxFeePerGas) };
}
function signMock(t) {
  t.r = '0x' + h64('r/' + Math.random()); t.s = '0x' + h64('s/' + Math.random()).replace(/^[89a-f]/, '1');
  const bytes = MO.encodeRawTx(rpcTxOf(Object.assign({}, t, { hash: '0x' })));
  t.raw = '0x' + MO.bytesToHex(bytes);
  t.hash = '0x' + MO.bytesToHex(MO.keccak256(bytes));
}

// ---- the wallets (EIP-1193, announced with EIP-6963) ----------------------------------------------
// one request handler for every mock wallet; `w` is the wallet asked (only MISAKA Wallet tops up)
async function walletRequest(w, method, params) {
  switch (method) {
    case 'eth_requestAccounts': case 'eth_accounts': return [ACCOUNT];
    case 'eth_chainId': return CHAIN_ID;
    case 'wallet_switchEthereumChain': case 'wallet_addEthereumChain': return null;
    case 'eth_sendTransaction': {
      const tx = params[0];
      const to = strip0x(tx.to);
      const from = strip0x(tx.from);
      const wei = BigInt(tx.value || '0x0');
      let line = null; for (const [l, f] of world.facades) if (strip0x(f) === to) line = l;
      const isCancel = !line && to === from && wei === 0n;
      if (!line && !isCancel) throw Object.assign(new Error('mock wallet: the recipient is not a line facade'), { code: -32000 });
      const sel = strip(tx.data).slice(0, 8); const a = argWords(tx.data || '0x');
      const rec = { from, to, line, sentAt: now() };
      if (isCancel) rec.kind = 'cancel';
      else if (sel === ABI.selector(SIG.buy)) { if (wei === 0n || wei % MO.NATIVE_SCALE_WEI !== 0n) throw Object.assign(new Error('execution reverted: BadValue()'), { code: -32000 }); Object.assign(rec, { kind: 'buy', sompi: wei / MO.NATIVE_SCALE_WEI, minUnits: ABI.u(a[0]) }); }
      else if (sel === ABI.selector(SIG.sell)) Object.assign(rec, { kind: 'sell', units: ABI.u(a[0]), minMsk: ABI.u(a[1]) });
      else if (sel === ABI.selector(SIG.seed)) {
        // ADR-0090: the writer reverts at the call for a bad value or a seed under the floor
        if (wei === 0n || wei % MO.NATIVE_SCALE_WEI !== 0n) throw Object.assign(new Error('execution reverted: BadValue()'), { code: -32000 });
        if (wei / MO.NATIVE_SCALE_WEI < C.seedMinSompi) throw Object.assign(new Error('execution reverted: SeedTooSmall()'), { code: -32000 });
        Object.assign(rec, { kind: 'seed', sompi: wei / MO.NATIVE_SCALE_WEI });
      }
      else throw Object.assign(new Error('execution reverted: NonTransferable()'), { code: -32000 });
      await new Promise((r) => setTimeout(r, 600));    // "confirm in wallet"
      // MISAKA Wallet's top-up (autobridge.planTopUp): the shortfall plus a 10,000-sompi gas reserve lands on
      // the EVM account before the transaction is sent. The real one waits for the claim (~two blocks).
      if (w.isMisakaWallet && (rec.kind === 'buy' || rec.kind === 'seed')) {
        const bal = world.balances.get(rec.from) || 0n;
        if (wei > bal) world.balances.set(rec.from, wei + 10000n * MO.NATIVE_SCALE_WEI);
      }
      // MISAKA Wallet signs with the nonce the site names; the other mock wallets pick their own
      rec.nonce = w.isMisakaWallet && tx.nonce != null ? Number(BigInt(tx.nonce)) : pendingNonce(from);
      rec.maxFeePerGas = tx.maxFeePerGas != null ? BigInt(tx.maxFeePerGas) : MOCK_BASE_FEE * 2n;
      rec.maxPriorityFeePerGas = tx.maxPriorityFeePerGas != null ? BigInt(tx.maxPriorityFeePerGas) : 0n;
      rec.gas = tx.gas != null ? BigInt(tx.gas) : isCancel ? 21000n : 70000n;
      rec.value = wei; rec.input = tx.data || '0x';
      if (rec.nonce < (world.nonces.get(from) || 0)) throw Object.assign(new Error('nonce too low'), { code: -32000 });
      const prior = Array.from(world.pool.values()).find((x) => x.from === from && x.nonce === rec.nonce);
      if (prior && (rec.maxFeePerGas < prior.maxFeePerGas + prior.maxFeePerGas * 10n / 100n || rec.maxPriorityFeePerGas < prior.maxPriorityFeePerGas + prior.maxPriorityFeePerGas * 10n / 100n)) {
        throw Object.assign(new Error('replacement transaction underpriced'), { code: -32000 });
      }
      signMock(rec);
      if (prior) world.pool.delete(prior.hash);
      rec.addedBlock = world.block;
      world.pool.set(rec.hash, rec);
      return rec.hash;
    }
    default: return M.evm(method, params);
  }
}
// Stand-ins, not the real wallets: the names and rdns those announce, and plain lettered icons.
const letterIcon = (fill, letter) => 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="7" fill="' + fill + '"/><text x="16" y="21.5" font-family="Arial,Helvetica,sans-serif" font-size="15" font-weight="700" text-anchor="middle" fill="#ffffff">' + letter + '</text></svg>');
const WALLET_SPECS = {
  misaka: { name: 'MISAKA Wallet', rdns: 'com.misakachain.wallet', misaka: true, icon: letterIcon('#0f6e63', 'M') },
  'misaka-legacy': { misaka: true },
  phantom: { name: 'Phantom', rdns: 'app.phantom', icon: letterIcon('#5b4bb7', 'P') },
  metamask: { name: 'MetaMask', rdns: 'io.metamask', icon: letterIcon('#b8641a', 'MM') },
  injected: {},
};
function makeWallet(kind, spec) {
  const handlers = {};
  const w = {
    isMisakaMock: true,
    isMisakaWallet: spec.misaka || undefined,
    mockKind: kind,
    calls: [],                                  // the methods this wallet was asked for, in order (tests read it)
    on(ev, fn) { (handlers[ev] = handlers[ev] || []).push(fn); return w; },
    removeListener(ev, fn) { handlers[ev] = (handlers[ev] || []).filter((f) => f !== fn); return w; },
    listenerCount(ev) { return (handlers[ev] || []).length; },
    emit(ev, data) { for (const fn of (handlers[ev] || []).slice()) fn(data); },   // a test plays the wallet's events
    async request(args) { const { method, params } = args || {}; if (w.calls.length < 500) w.calls.push(method); return walletRequest(w, method, params); },
  };
  return w;
}
const uuid4 = () => (window.crypto && crypto.randomUUID ? crypto.randomUUID() : 'xxxxxxxx-xxxx-4xxx-8xxx-xxxxxxxxxxxx'.replace(/x/g, () => Math.floor(Math.random() * 16).toString(16)));
M.wallets = [];
for (const kind of WALLET_KINDS) {
  const spec = WALLET_SPECS[kind];
  if (!spec) { console.warn('[mock] ?mockwallet: no wallet "' + kind + '" (misaka, misaka-legacy, phantom, metamask, injected, none)'); continue; }
  if (M.wallets.some((x) => x.kind === kind)) continue;
  M.wallets.push({ kind, provider: makeWallet(kind, spec), info: spec.rdns ? Object.freeze({ uuid: uuid4(), name: spec.name, icon: spec.icon, rdns: spec.rdns }) : null });
}
// The legacy globals, as the extensions set them: MISAKA Wallet always sets window.misaka (and takes
// window.ethereum on MISAKA's own sites); a wallet that only injects has window.ethereum; else the first does.
const kindOf = (k) => (M.wallets.find((x) => x.kind === k) || {}).provider;
M.misaka = kindOf('misaka') || kindOf('misaka-legacy');
M.ethereum = kindOf('injected') || M.misaka || (M.wallets[0] && M.wallets[0].provider);
function announceWallets() {
  for (const x of M.wallets) if (x.info) window.dispatchEvent(new CustomEvent('eip6963:announceProvider', { detail: Object.freeze({ info: x.info, provider: x.provider }) }));
}
window.addEventListener('eip6963:requestProvider', announceWallets);
announceWallets();

const WORLD_TAG = 'adr0090-seeded-v1';
M.init = (mo) => {
  MO = mo; C = MO.CURVE_DEFAULTS; ABI = MO.ABI; SIG = MO.SIG; curve = MO.curve;
  // a new mock world starts from its own tape: drop the price samples and the sent transactions an
  // older mock world left under the mo-mock: prefix (they would show up as history of these lines)
  if (MO.store.get('mock:world') !== WORLD_TAG) {
    for (const k of Object.keys(localStorage)) if (k.startsWith('mo-mock:hist:') || k === 'mo-mock:txs' || k === 'mo-mock:txs:claimed' || k === 'mo-mock:lines' || k === 'mo-mock:classes' || k === 'mo-mock:lastLine') { try { localStorage.removeItem(k); } catch (e) { /* ignore */ } }
    MO.store.set('mock:world', WORLD_TAG);
    MO.history.cache.clear();
    if (MO.txlog) { MO.txlog.list.splice(0); MO.txlog.claimed.clear(); MO.txlog.save(); }   // the app read the old list before this ran
  }
  seedWorld();
  setInterval(tickBlock, TICK_MS);
  M.world = world; M.CLASS_R = CLASS_R;
  console.info('[mock] MISAKA Options mock world ready: ' + world.lines.size + ' lines (' + [...world.markets.values()].filter((m) => m.seeded).length + ' seeded), ' + world.logs.length + ' settlement logs, account ' + ACCOUNT + ', unseeded class ' + CLASS_R.slice(0, 8));
};
})();
