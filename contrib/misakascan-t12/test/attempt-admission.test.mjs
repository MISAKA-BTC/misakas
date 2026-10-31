import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';
const source=readFileSync(process.env.MISAKA_SCAN_SOURCE || new URL('../app.js',import.meta.url),'utf8');
const section=(a,b)=>source.slice(source.indexOf(a),source.indexOf(b,source.indexOf(a)));
const classId='ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01';
const bondTx='5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669';
// A controlled positive RPC fixture; this block/claim is not live evidence.
const row={block:'2'.repeat(128),classId,bondTx,bondIx:0,ts:1,pwu:42,chain:true};
const claim={claimId:'c'.repeat(128),acceptedBlock:row.block,classId,executorBond:bondTx+':0',acceptedDaa:18851,phase:'final',isFreePrompt:false,seats:['seat-one']};
function context({claims=[],detail=null,fail=false,job=null}={}){
  const elements={llmSubs:{},llmSubCount:{},llmClaimLinks:{}};
  const ctx=vm.createContext({Map,Set,Promise,Uint8Array,document:{getElementById:id=>elements[id]||null},
    num:n=>String(n),esc:s=>String(s).replaceAll('<','&lt;'),short:s=>String(s).slice(0,12),dt:()=>'',ago:()=>'',linkBlock:b=>b,
    llmIsFloor:()=>false,llmClassName:()=> 'QWEN25-A16-8K',LLM_CLASS_BY_ID:{[classId]:{name:'QWEN25-A16-8K'}},
    llmJobs:new Map(job?[[row.block,job]]:[]),llmFpJobs:new Map(),llmSubs:[row],llmEvents:[],llmFilter:'llm',llmTotalSubs:1,
    llmDisclosedCell:()=>null,llmOutCell:()=> 'output withheld',llmInCell:()=> 'input',llmPromptOnChain:()=> 'prompt committed',llmSealed:()=> 'sealed',
    scanClaimsOf:async()=>{if(fail)throw Error('RPC unavailable');return claims},palwRead:async()=>{if(fail)throw Error('RPC unavailable');return detail},
  });
  vm.runInContext(source.match(/^const scanIsRound =.*$/m)[0],ctx);
  vm.runInContext(section('const PALW_PHASE =','/// The production half:')+section('const llmAdmissionByBlock =','/* ----------------------- LATEST TRANSACTIONS (full page)'),ctx);
  return {ctx,elements};
}
test('an exact admitted one-seat Final is joined without increasing execution round counts',async()=>{
  const {ctx,elements}=context({claims:[claim]});await ctx.refreshLlmAdmissions([row]);ctx.paintLlm();
  assert.equal(ctx.llmClaimForRow(row),claim.claimId);assert.match(elements.llmSubs.innerHTML,/Accepted · Final/);
  assert.match(elements.llmSubs.innerHTML,/1 seat\(s\)/);assert.match(elements.llmClaimLinks.innerHTML,new RegExp(claim.claimId));
  assert.match(elements.llmSubCount.textContent,/1 matched/);assert.doesNotMatch(elements.llmSubCount.textContent,/on chain/);
  assert.equal(vm.runInContext('scanIsRound({algo:6})',ctx),false);assert.equal(vm.runInContext('scanIsRound({algo:10})',ctx),true);
});
test('the actually received 481352 header is never admitted from its presence or a blue flag',async()=>{
  const actual={...row,block:'48135274990f64bfdf3448339361a713331f7c4d0a3b871ff2a43a331cd0cdab4889a112dd1e84fa975cf81716da1a642dcf5406acd62cd1635e9019ece9605f'};
  const {ctx,elements}=context();ctx.llmSubs=[actual];await ctx.refreshLlmAdmissions([actual]);ctx.paintLlm();
  assert.equal(ctx.llmClaimForRow(actual),null);assert.match(elements.llmSubs.innerHTML,/acceptance unconfirmed/);
  assert.doesNotMatch(elements.llmSubs.innerHTML,/Accepted ·|Final/);assert.match(elements.llmSubCount.textContent,/0 matched/);
});
test('class, executor, block and claimed identity mismatches cannot borrow another admission',()=>{
  const {ctx}=context();
  for(const bad of [{...claim,classId:'f'.repeat(128)},{...claim,executorBond:bondTx+':1'},{...claim,acceptedBlock:'3'.repeat(128)},{...claim,isFreePrompt:true}]) assert.equal(ctx.llmAdmissionState(row,[bad],null).accepted,false);
  assert.equal(ctx.llmAdmissionState({...row,claim:'d'.repeat(128)},[claim],null).accepted,false);
});
test('found=false, truncated/retired row absence and unavailable RPC remain unknown and retryable',async()=>{
  for(const settings of [{claims:[],detail:{found:false}},{claims:[],detail:{__unsupported:true}},{claims:null,fail:true}]){
    const {ctx,elements}=context({...settings,job:{claim:claim.claimId,class_id:classId,executor_bond:bondTx+':0',prompt_derived:false}});
    await ctx.refreshLlmAdmissions([row]);ctx.paintLlm();
    assert.match(elements.llmSubs.innerHTML,/acceptance unconfirmed/);assert.doesNotMatch(elements.llmSubs.innerHTML,/Accepted ·|rejected|Approved/);
    assert.match(elements.llmSubs.innerHTML,/prompt unavailable for this profile/);
  }
});
test('a feed candidate may be confirmed only by a matching found detail response',async()=>{
  const {ctx,elements}=context({job:{claim:claim.claimId,class_id:classId,executor_bond:bondTx+':0',prompt_derived:false},detail:{...claim,found:true}});
  await ctx.refreshLlmAdmissions([row]);ctx.paintLlm();assert.match(elements.llmSubs.innerHTML,/Accepted · Final/);
  const bad=context({job:{claim:claim.claimId,class_id:classId,executor_bond:bondTx+':0',prompt_derived:false},detail:{...claim,found:true,acceptedBlock:'f'.repeat(128)}});
  await bad.ctx.refreshLlmAdmissions([row]);bad.ctx.paintLlm();assert.doesNotMatch(bad.elements.llmSubs.innerHTML,/Accepted ·/);
});
test('PAV3 metadata reads the unchanged base attempt at the correct offset',()=>{
  const ctx=vm.createContext({Uint8Array});vm.runInContext(section('function llmBytes(','function llmOutpoint('),ctx);
  const u64=n=>[n,0,0,0,0,0,0,0],hash=n=>Array(64).fill(n);
  const base=[6,0,...hash(1),...hash(2),...hash(3),...hash(4),0,0,0,0,0,0,0,0,...hash(5),...hash(6),...hash(7),...hash(8),...u64(42),...hash(9),1,0,0,0,...u64(99),...hash(10)];
  const v2=ctx.llmDecodeAttempt([80,65,86,50,...base]),v3=ctx.llmDecodeAttempt([80,65,86,51,3,0,...base]);
  assert.equal(v2.pwu,42);assert.equal(v3.classId,v2.classId);assert.equal(v3.execRoot,v2.execRoot);
  assert.equal(ctx.llmDecodeAttempt([80,65,86,51,9,0,...base]),null);
});

test('an unavailable operator RPC cannot supply stray rows as admitted claims',async()=>{
  const {ctx,elements}=context();
  ctx.palwRead=async()=>({available:false,claims:[claim]});
  vm.runInContext(section('const scanClaimCache =','function scanClaimClassName('),ctx);
  await ctx.refreshLlmAdmissions([row]);ctx.paintLlm();
  assert.match(elements.llmSubs.innerHTML,/acceptance unconfirmed/);assert.doesNotMatch(elements.llmSubs.innerHTML,/Accepted ·/);
});


test('two free-prompt claims sharing a carrier cannot share cached admission or phase',async()=>{
  const first={...row,fp:true,claim:claim.claimId};
  const second={...row,fp:true,claim:'d'.repeat(128)};
  const {ctx,elements}=context({claims:[{...claim,isFreePrompt:true}]});
  ctx.llmSubs=[first,second];await ctx.refreshLlmAdmissions([first,second]);ctx.paintLlm();
  assert.notEqual(ctx.llmAdmissionKey(first),ctx.llmAdmissionKey(second));
  assert.match(ctx.llmAdmissionCell(first),/Accepted · Final/);
  assert.match(ctx.llmAdmissionCell(second),/acceptance unconfirmed/);
  assert.doesNotMatch(ctx.llmAdmissionCell(second),/Accepted ·|Final/);
  assert.match(elements.llmSubCount.textContent,/1 matched/);
});
