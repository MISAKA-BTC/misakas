const assert = require('node:assert/strict');
const {classify,seatCount,panelCell,refresh} = require('./model-readiness.js');
const model = {available:true,found:true,readySeats:8,requiredReadySeats:7,registryState:'Active'};
const permit = {available:true,bondKnown:true,notReadyReason:'',daaScore:5800};
assert.equal(classify(model,[permit],{claimsFinal:0},true).label,'Ready');
assert.equal(classify(model,[permit],{claimsFinal:0},true).history,'Final 0');
assert.equal(classify({...model,readySeats:0},[{...permit,notReadyReason:'class not admitting'}],{claimsFinal:0},true).label,'Not ready');
assert.equal(classify({...model,readySeats:0},[permit],{claimsFinal:0},true).label,'Ready'); // Floor: no Panel quorum gate.
assert.equal(classify(model,[{...permit,bondKnown:false}],null,true).label,'Registered only');
assert.equal(classify(null,[],null,false).label,'Unknown');
assert.equal(classify({available:true,found:false},[permit],null,true).label,'Unknown');
const held = classify(model,[{...permit,notReadyReason:'epoch budget exhausted'}],{ledger:{available:true,finals:12,producerNamedSompi:'5'}},true);
assert.equal(held.label,'Not ready');
assert.equal(held.history,'Final 12 · reward recorded');
assert.equal(classify(model,[],null,false).label,'Unknown');
assert.equal(classify(model,[],{ledger:{available:false,finals:99},claimsFinal:2},true).history,'Final 2');
assert.equal(seatCount(model),8);
assert.equal(seatCount({...model,readySeats:0}),0);
assert.equal(seatCount({...model,readySeats:null}),null);
assert.equal(seatCount({...model,available:false}),null);
assert.equal(seatCount({...model,readySeats:'8'}),null);
assert.equal(seatCount({...model,readySeats:-1}),null);
(async () => {
  const id = 'a'.repeat(128), rec = {classId:id};
  await refresh([rec], async method => {
    if (method === 'getPalwModel') return {...model,readySeats:0};
    if (method === 'getPalwPanelSeats') return {available:true,seats:[]};
    return {available:true,classes:[]};
  });
  assert.match(panelCell(rec),/>0<\/span>/);
  assert.match(panelCell(rec),/required: 7/);
  const missing = {classId:'b'.repeat(128)};
  await refresh([missing], async () => { throw new Error('RPC down'); });
  assert.match(panelCell(missing),/>—<\/span>/);
  const floor = {classId:'c'.repeat(128)};
  let asked = false;
  await refresh([floor], async method => {
    if (method === 'getPalwModel') return {...model,readySeats:0};
    if (method === 'getPalwPanelSeats') return {available:true,seats:[{bondOutpoint:'d'.repeat(128)+':0'}]};
    if (method === 'getPalwProducerFacts') { asked=true; return permit; }
    return {available:true,classes:[]};
  });
  assert.equal(asked,true);
  assert.match(require('./model-readiness.js').cell(floor),/>Ready<\/span>/);
  console.log('model readiness: eligibility, Final history and Panel count tests passed');
})().catch(error => { console.error(error); process.exitCode = 1; });
