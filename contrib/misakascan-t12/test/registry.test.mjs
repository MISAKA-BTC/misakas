import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const helpers = source.slice(source.indexOf('function registryProtocolView('), source.indexOf('async function renderRegistry()'));
const ctx = vm.createContext({num: v => v == null ? '—' : String(v), esc: v => String(v).replaceAll('<', '&lt;'), linkBlock: v => `<a>${v}</a>`});
vm.runInContext(helpers, ctx);
test('crossing a height does not establish G14 activation on the old live ruleset', () => {
  const html = ctx.registryProtocolView({consensusParamsId:'ef379378f09bf33f29e97854bbee157a8f86fa1dd8248e1a847f08c5c6e7f15e', fenceSchedule:[15500]}, 18028);
  assert.match(html, /does not advertise/);
  assert.doesNotMatch(html, /node advertises the G14/);
  assert.match(ctx.registryProtocolView({}, 19000), /not established/);
});
test('unavailable settlement never renders default values as an affirmative result', () => {
  const html = ctx.registrySettlementView({available:false, settled:true, depth:0});
  assert.match(html, /Settlement unavailable/);
  assert.doesNotMatch(html, /requested DAA|>yes</);
});
test('Final and certification counts remain separate and missing premise is not inferred', () => {
  const html = ctx.registrySettlementView({available:true, settled:false, depth:0, uncertifiedFinalAnchors:12});
  assert.match(html, /Final without certification/);
  assert.match(html, />12</);
  assert.match(html, /certification is not inferred/);
  assert.match(html, /only SecurityCertified/);
});
test('native snapshot absence does not fabricate safe/finalized heads', () => {
  const html = ctx.registrySettlementView({available:true, dnsRetiredAt:17400, nativeSettlement:null});
  assert.match(html, /Native settlement snapshot unavailable/);
  assert.doesNotMatch(html, /<a>/);
});
test('honest-stake premise is shown and escaped when the node supplies it', () => {
  assert.match(ctx.registrySettlementView({available:true, securityCertificationPremise:'honest stake < threshold'}), /honest stake &lt; threshold/);
});
