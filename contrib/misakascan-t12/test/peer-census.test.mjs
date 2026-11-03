import {test} from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';
const source=fs.readFileSync(process.env.MISAKA_SCAN_SOURCE||new URL('../app.js',import.meta.url),'utf8');
const part=source.slice(source.indexOf('let peerCensus ='),source.indexOf('// claude-bridge-fresh-v2:',source.indexOf('let peerCensus =')));
function context(read){const c=vm.createContext({num:String,esc:v=>String(v).replaceAll('<','&lt;').replaceAll('"','&quot;'),fixture:read});vm.runInContext(part+'\npeerCensus=fixture;',c);return c}
const genesis='a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23';
const valid=()=>({available:true,network:'testnet-12',generated_at:new Date().toISOString(),source:{genesisHash:genesis},refused:[],complete_window:false,observed_to:'now'});
test('legacy null or stale census is unavailable, not zero refusals',()=>{
 for(const read of [{network:null,refused:[],generated_at:new Date().toISOString()}, {...valid(),generated_at:'2020-01-01T00:00:00Z'}, {...valid(),source:{genesisHash:'wrong'}}, {...valid(),available:false,availability_reason:'wrong log'}]){
  const c=context(read);assert.equal(c.censusUsable(),false);const h=c.censusPanelHtml(new Set());assert.match(h,/unavailable/);assert.doesNotMatch(h,/No rejection events|Nobody was refused/);
 }
});
test('fresh verified log may report no observed events with partial-window context',()=>{
 const c=context(valid());assert.equal(c.censusUsable(),true);const h=c.censusPanelHtml(new Set());assert.match(h,/No rejection events observed/);assert.match(h,/does not cover the complete/);
});
test('connected and rejected sessions behind one NAT/IP remain independently visible',()=>{
 const ip='198.51.100.27',r={ip,port:1234,reason:'fork_id_mismatch',mismatch:'next_fence_mismatch',direction:'local_rejected_peer',rejecting_node_expected_next:'18640',rejected_peer_next_fence:'18446744073709551615',attempts:1};
 const c=context({...valid(),refused:[r,{...r,port:26311,direction:'peer_rejected_local',mismatch:'unknown_fired_set'}]});
 assert.equal(c.censusRefused().length,2);const h=c.censusPanelHtml(new Set([ip]));assert.match(h,/also connected/);assert.match(h,/observer rejected a peer session/);assert.match(h,/peer rejected this observer/);assert.match(h,/none \(u64::MAX\)/);assert.doesNotMatch(h,/outdated build/);
});
test('actual RPC values show18640 on the primary rather than inferring revision from a label',()=>{
 const c=context(valid());const h=c.peerRuntimeView([{names:['primary','seed'],p2pId:'52196ead-085a-408b-a1c4-5cda62c769fd',synced:false,dag:{network:'testnet-12',virtualDaaScore:18930},node:{consensusParamsId:'edf472',consensusScheduleId:'4024',fenceSchedule:[18400,18640]}}]);
 assert.match(h,/18400, 18640/);assert.match(h,/edf472/);assert.match(h,/52196ead/);assert.match(h,/not a git-revision attestation/);
});
