// No browser/network dependency: exercise the retired capability route and absent-head rendering.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../app.js'), 'utf8');
function extract(name) {
  const marker = `function ${name}(`;
  let start = source.indexOf(marker);
  assert.ok(start >= 0, name);
  if (source.slice(start - 6, start) === 'async ') start -= 6;
  const end = source.indexOf('\n}', start);
  assert.ok(end > start, name);
  return source.slice(start, end + 2);
}
const surface = { innerHTML: '' };
const context = vm.createContext({
  routeGen: 1, pollTimer: null, bridgeAnchor: { hash: null, blue: null }, bridgeNativeSettlement: null,
  viewFor: () => surface, clearInterval: () => {}, armPoll: () => {}, onBlockAdded: () => {},
  esc: value => String(value ?? '').replace(/</g, '&lt;'),
  linkBlock: hash => `<a>${hash}</a>`, num: String,
  status: { dnsRetiredAt: 5, nativeSettlement: { latest: 'executed-result', safe: null, finalized: null,
    depth: 0, uniqueWork: '0', stop: 'insufficientWork' } },
  rpc: async () => context.status,
});
vm.runInContext(['readNativeSettlement', 'readinessWaitText', 'nativeReadinessView', 'nativeSettlementView', 'renderOverlay', 'renderFinality',
  'refreshBridgeAnchorBlue', 'bridgeFreshNote', 'showNativeSettlement', 'refreshNativeSettlement', 'armNativeSettlementRefresh'].map(extract).join('\n'), context);
(async () => {
  await context.renderOverlay();
  assert.match(surface.innerHTML, /PALW settlement/);
  assert.match(surface.innerHTML, /executed-result/);
  assert.equal((surface.innerHTML.match(/unavailable/g) || []).length, 2);
  assert.doesNotMatch(surface.innerHTML, /fund validators|quorum|staking &amp;/);
  await context.renderFinality();
  assert.doesNotMatch(surface.innerHTML, /DNS-final blocks|irreversible under DNS/);
  await context.refreshBridgeAnchorBlue(null);
  assert.match(context.bridgeFreshNote(null, null), /safe head unavailable/);
  context.status = { dnsRetiredAt: 5 };
  await context.renderOverlay();
  assert.match(surface.innerHTML, /snapshot unavailable/);
  assert.doesNotMatch(surface.innerHTML, /<a>executed-result/);
  // A late capability response cannot overwrite a page the user navigated to.
  let resolve;
  context.rpc = () => new Promise(r => { resolve = r; });
  surface.innerHTML = 'new route';
  const pending = context.renderOverlay();
  context.routeGen++;
  resolve(context.status);
  await pending;
  assert.equal(surface.innerHTML, 'new route');
  context.rpc = async () => { throw new Error('unavailable'); };
  assert.equal(await context.readNativeSettlement(), null);
  await context.refreshNativeSettlement(context.routeGen);
  assert.match(surface.innerHTML, /status unavailable/);
  assert.doesNotMatch(surface.innerHTML, /executed-result/);
  // A below-finalized conflict is shown as an alarm that names the resync, never as an ordinary stop reason.
  context.rpc = async () => context.status;
  context.status = { dnsRetiredAt: 5, nativeSettlement: { latest: 'executed-result', safe: null, finalized: null,
    depth: 0, uniqueWork: '0', stop: 'finalizedConflict' } };
  surface.innerHTML = '';
  await context.renderOverlay();
  assert.match(surface.innerHTML, /Safety alarm/);
  assert.match(surface.innerHTML, /resynced/);
  assert.equal((surface.innerHTML.match(/unavailable/g) || []).length, 2, 'safe and finalized stay unavailable');
  // An ordinary stop reason raises no alarm.
  context.status = { dnsRetiredAt: 5, nativeSettlement: { latest: 'executed-result', safe: null, finalized: null,
    depth: 0, uniqueWork: '0', stop: 'frontierNotCovered' } };
  surface.innerHTML = '';
  await context.renderOverlay();
  assert.doesNotMatch(surface.innerHTML, /Safety alarm/);
  assert.match(surface.innerHTML, /frontierNotCovered/);
  // RFC-0012 D1 (C8): the explanation is rendered, in the node's own numbers, and a withdrawn finalized label is an alarm.
  const readiness = (patch = {}) => ({
    version: 1, safeLagDaa: 1400, safeLagBlue: 1100, stoppedEarly: null, tip: null,
    maturity: { rule: 'v1', claimRetirementDaa: 3000, quantumMaturityDaa: 120 },
    blocking: { block: 'blocked-effect', daa: 1001, blue: 900, inSafePrefix: false, openClaimsTotal: 1, openSessionsTotal: 0,
      earliestReadyInDaa: 4099, waits: [
        { kind: 'openClaim', claim: 'c1'.repeat(32), stage: 'final', acceptedBlue: 880, retentionDaa: 6401, nextDeadlineDaa: null, waitDaa: 4000 },
        { kind: 'insufficientDepth', have: 1, need: 3 },
        { kind: 'waitingMaturity', facts: 2, work: '40', earliestMaturedDaa: 6401, waitDaa: 4000, readyDaa: 6500 } ] },
    finalized: { finalized: null, pruningPoint: 'pp', pruningBlue: 0, wait: { kind: 'pruningPointNotExecuted' }, withdrawnFrom: null },
    skipped: { voided: 0, baseClass: 0, openDa: 0, unpriced: 0, bondNotHeld: 1 },
    ...patch });
  context.status = { dnsRetiredAt: 5, nativeSettlement: { latest: 'executed-result', safe: null, finalized: null,
    depth: 0, uniqueWork: '0', stop: 'openLifecycle' }, nativeReadiness: readiness() };
  surface.innerHTML = '';
  await context.renderOverlay();
  assert.match(surface.innerHTML, /Safe cannot pass/);
  assert.match(surface.innerHTML, /its trace retention lapses in 4000 DAA/);
  assert.match(surface.innerHTML, /1 of 3 settled anchors/);
  assert.match(surface.innerHTML, /waiting on maturity: 2 fact\(s\), work 40; the first matures in 4000 DAA, enough by DAA 6500/);
  assert.match(surface.innerHTML, /no sooner than 4099 DAA from now/);
  assert.match(surface.innerHTML, /Finalized waits on the pruning point: pruningPointNotExecuted/);
  assert.match(surface.innerHTML, /1 with no bond in state/);
  assert.doesNotMatch(surface.innerHTML, /Safety alarm|Finalized label withdrawn/);
  // No promise where an event is awaited.
  context.status.nativeReadiness = readiness({ blocking: { ...readiness().blocking, earliestReadyInDaa: null } });
  surface.innerHTML = '';
  await context.renderOverlay();
  assert.doesNotMatch(surface.innerHTML, /no sooner than/);
  // Weighing that never happened says why.
  context.status.nativeReadiness = readiness({ stoppedEarly: { kind: 'missingHistory', gap: 'deltaNotRetained', block: 'gap-block' }, blocking: null });
  surface.innerHTML = '';
  await context.renderOverlay();
  assert.match(surface.innerHTML, /Safe is withheld/);
  assert.match(surface.innerHTML, /missing history \(deltaNotRetained\) at <a>gap-block/);
  // A published finalized label that was withdrawn while the head stayed canonical is an alarm, named.
  context.status.nativeReadiness = readiness({ finalized: { finalized: null, pruningPoint: 'pp', pruningBlue: 0, wait: { kind: 'noSafePrefix' }, withdrawnFrom: 'withdrawn-head' } });
  surface.innerHTML = '';
  await context.renderOverlay();
  assert.match(surface.innerHTML, /Finalized label withdrawn/);
  assert.match(surface.innerHTML, /<a>withdrawn-head/);
  // An older node (no field) renders exactly what it did.
  context.status = { dnsRetiredAt: 5, nativeSettlement: { latest: 'executed-result', safe: null, finalized: null, depth: 0, uniqueWork: '0', stop: 'insufficientWork' } };
  surface.innerHTML = '';
  await context.renderOverlay();
  assert.doesNotMatch(surface.innerHTML, /Safe cannot pass|Maturity rule/);
  console.log('native settlement explorer tests passed');
})().catch(error => { console.error(error); process.exitCode = 1; });
