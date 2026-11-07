// RFC-0015 §11.3 display rules (test source; run with `node claim-assurance.test.cjs`).
const assert = require('node:assert/strict');
const {describe, render, NOT_PROOF} = require('./claim-assurance.js');
const base = {available:true, found:true, tipDaa:40, state:'Challengeable { since_daa: 3, window_end_daa: 53 }', demands:[]};
const opv = {...base, mode:'OptimisticPublicVerification', opv:true, opvAdmittedDaa:3, opvVerifierStartCutoffDaa:30,
  opvFinalFloorDaa:53, opvHardDeadlineDaa:83, settlement:'pending', settledFromDaa:0,
  finalStatement:'Final under optimistic public verification: ... This is not a proof that the computation is correct'};

const pending = describe(opv);
assert.equal(pending.opv, true);
assert.equal(pending.label, 'Pending');
assert.equal(pending.window.open, true);
assert.equal(pending.notProof, NOT_PROOF);
assert.equal(pending.safe, false);
assert.match(render(opv), /No challenge ≠ proof of correctness/);

const revocable = describe({...opv, tipDaa:60, settlement:'final-revocable', settledFromDaa:331});
assert.equal(revocable.label, 'Final · still convictable');
assert.equal(revocable.safe, false, 'a Final within the liability horizon is never shown as safe');
assert.equal(revocable.settledFrom, 331);
assert.equal(revocable.window.open, false);

assert.equal(describe({...opv, settlement:'settled'}).safe, true);
assert.equal(describe({...opv, settlement:'convicted'}).label, 'Convicted');
assert.equal(describe({...opv, settlement:'defaulted'}).label, 'Defaulted');
assert.equal(describe({...opv, settlement:undefined}).label, 'Unknown', 'an older node never reads as settled');

const panel = describe({...base, mode:'PanelLicensed', opv:false, settlement:'final-revocable'});
assert.equal(panel.opv, false);
assert.equal(panel.window, null);
assert.equal(panel.notProof, '');

assert.equal(describe({available:false}).found, false);
assert.equal(describe({available:true, found:false}).label, 'Not found');
assert.doesNotMatch(render({...opv, finalStatement:'<script>'}), /<script>/, 'escaped');
console.log('claim-assurance: ok');
