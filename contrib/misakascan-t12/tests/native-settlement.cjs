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
vm.runInContext(['readNativeSettlement', 'nativeSettlementView', 'renderOverlay', 'renderFinality',
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
  console.log('native settlement explorer tests passed');
})().catch(error => { console.error(error); process.exitCode = 1; });
