import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';
const source = readFileSync(process.env.MISAKA_SCAN_SOURCE || new URL('../app.js', import.meta.url), 'utf8');
const policyHelpers = source.slice(source.indexOf('function llmModelOneArmed('), source.indexOf('async function refreshLlm(){'));
const registryHelpers = source.slice(source.indexOf('function registryAdmitsClaims('), source.indexOf('const LLM_CLASSES ='));
const render = source.slice(source.indexOf('async function refreshLlmInner(){'), source.indexOf('// **Who made this block'));
assert(policyHelpers.startsWith('function llmModelOneArmed('));
const qwen = { id:'qwen-class', name:'Qwen test fixture', model:'Qwen identity (genesis row; historical readiness)', tag:'llm' };
const armedRow = { family:'g14ModelPanelOne', armed:true, allModels:true, seatCount:1, scope:'whole-claim' };
const arming = row => ({ available:true, found:true, viewJson:JSON.stringify({ routeFamilies:[row] }) });
function context(){
  return vm.createContext({ Array, JSON });
}
test('only the explicit whole-claim all-model single-seat arming record enables the new view', () => {
  const ctx=context();vm.runInContext(policyHelpers,ctx);
  assert.equal(ctx.llmModelOneArmed(arming(armedRow)),true);
  for(const read of [null,{}, {__unsupported:true}, {available:false,found:true}, {available:true,found:false}, {available:true,found:true,viewJson:'{'}, arming({...armedRow,armed:false}), arming({...armedRow,allModels:false}), arming({...armedRow,seatCount:5}), arming({...armedRow,scope:'sampled'}), arming({...armedRow,family:'kernelRoutePanelOne'})]) assert.equal(ctx.llmModelOneArmed(read),false);
});
async function classCards(read, available=true){
  const elements={llmClasses:{innerHTML:''},llmSeatMatrix:{innerHTML:''}};
  const facts={available,pwu:10,epochBudgetBlocks:1000,epochProducedBlocks:119,bondKnown:false,notReadyReason:'no bond was named in this request — readiness is a property of a bond'};
  const registry={active:false,classes:[{classId:qwen.id,state:'Held',reason:'historical readiness shortfall',readySeatsNow:0,requiredReadySeats:7}],readiness:[]};
  const ctx=vm.createContext({
    document:{getElementById:id=>elements[id]||null},
    LLM_CLASSES:[qwen],LLM_CLASS_BY_ID:{[qwen.id]:qwen},llmFactCache:new Map(),llmModelNameCache:new Map(),PANEL_SEATS:[],PANEL_BOND_TX:'fixture-bond',PANEL_DRAW:5,PANEL_QUORUM:3,
    verifierCount:()=>0,num:v=>String(v),esc:v=>String(v),short:v=>String(v).slice(0,8),
    refreshLlmJobs:()=>{},hydrateBlockBodies:async()=>[],llmIsFloor:()=>false,
    rpc:async method=>method==='getPalwProducerFacts'?facts:method==='getBlocks'?{blocks:[]}:{sink:'fixture-sink'},
    palwRead:async method=>method==='getPalwModelRegistry'?registry:read,
    llmFpJobs:new Map(),llmJobs:new Map(),llmSeenClaims:new Set(),llmSeenBlocks:new Set(),llmSeenTx:new Set(),
    llmCursor:'fixture-cursor',llmSubs:[],llmEvents:[],llmTotalSubs:0,paintLlm:()=>{},
  });
  vm.runInContext(registryHelpers+'\n'+policyHelpers+'\n'+render,ctx);
  await ctx.refreshLlmInner();
  return elements;
}
test('active G14 renders class facts with ready=0 and withBond=false without asserting mining or certification', async () => {
  const el=await classCards(arming(armedRow));const html=el.llmClasses.innerHTML;
  assert.match(html,/G14 · 1 seat · whole claim/);assert.match(html,/class available/);assert.match(html,/119 \/ 1000/);
  assert.doesNotMatch(html,/OFF|⛏|minable|Final|SecurityCertified|Approved|historical readiness\)/);
  assert.match(el.llmSeatMatrix.innerHTML,/historical readiness · not an admission gate/);
  assert.match(el.llmSeatMatrix.innerHTML,/Readiness records.*do not gate new claims/);
});
test('inactive or unknown arming preserves the legacy readiness hold renderer', async () => {
  for(const read of [arming({...armedRow,armed:false}),{__unsupported:true},{}]){
    const el=await classCards(read);assert.match(el.llmClasses.innerHTML,/OFF — Held · ready 0 \/ 7/);assert.doesNotMatch(el.llmClasses.innerHTML,/G14 · 1 seat/);assert.match(el.llmSeatMatrix.innerHTML,/chain ready \(need requiredReadySeats\)/);
  }
});
test('active policy does not fabricate class availability from an unavailable producer-facts read', async () => {
  const el=await classCards(arming(armedRow),false);assert.match(el.llmClasses.innerHTML,/class unavailable on this node/);assert.doesNotMatch(el.llmClasses.innerHTML,/>class available</);assert.match(el.llmClasses.innerHTML,/card off/);
});
