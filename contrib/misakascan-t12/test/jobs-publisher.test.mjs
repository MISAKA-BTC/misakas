import {test} from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs/promises';import os from 'node:os';import path from 'node:path';
import {runOnce,GENESIS,losslessJson,canonicalBond,rpcFailure} from '../jobs-publisher.mjs';
const fixture=JSON.parse(await fs.readFile(new URL('../../../docs/evidence/palw/model-panel-one-2026-10-31/jobs-export-481352-input.json',import.meta.url),'utf8')).blocks[0];
const expected=JSON.parse(await fs.readFile(new URL('../../../docs/evidence/palw/model-panel-one-2026-10-31/jobs-export-481352-expected.json',import.meta.url),'utf8'));
const hash=n=>n.toString(16).padStart(128,'0'),SINK=hash(900),MID=hash(899);
const heartbeat=(h,parent,blue=80000)=>({header:{hash:h,daaScore:19000,blueScore:blue,timestamp:1793470000000,powAlgoId:8},verboseData:{selectedParentHash:parent,isChainBlock:true}});
const oldRow={block:hash(700),claim:hash(701),class_id:expected.class_id,executor_bond:hash(702)+':0',execution_root:hash(703),ts:1,daa:1};
async function setup(t){
 const dir=await fs.mkdtemp(path.join(os.tmpdir(),'misaka-jobs-'));t.after(()=>fs.rm(dir,{recursive:true,force:true}));
 const o={workdir:dir,output:path.join(dir,'llm-jobs.json'),exporter:'/immutable/jobs-export',lookback:1,overlap:0,maxPages:2,maxRpc:16};
 await fs.writeFile(o.output,JSON.stringify({network:'testnet-12',genesis:GENESIS,rows:[oldRow],fp_rows:[]}));
 return {o,dir};
}
function source({pages,model=fixture,badCursor,fail}={}){let count=0;const calls=[];return {calls,async call(method,p){calls.push([method,p]);if(fail===method)throw Error('source RPC error');
 if(method==='getInfo')return {isSynced:true};if(method==='getBlockDagInfo')return {network:'testnet-12',sink:SINK,virtualDaaScore:19000};if(method==='getPalwNodeStatus')return {genesisHash:GENESIS};
 if(method==='getBlock'){if(p.hash===badCursor)throw rpcFailure('getBlock','cannot find header '+p.hash);if(p.hash===GENESIS)return heartbeat(GENESIS,GENESIS,0);if(p.hash===MID)return heartbeat(MID,GENESIS);return heartbeat(SINK,GENESIS)}
 if(method==='getBlocks')return {blocks:pages?pages[count++]:[model,heartbeat(SINK,GENESIS)]};throw Error('unexpected RPC');},close(){}}}
function exporter({bad,observe}={}){return async(executable,args)=>{
 assert.equal(executable,'/immutable/jobs-export');assert.deepEqual(args.slice(2,6),['--retention','/dev/null','--network','testnet-12']);
 const input=losslessJson(await fs.readFile(args[1],'utf8'));assert(input.blocks.every(b=>Object.keys(b).sort().join(',')==='hash,header'));observe?.(input);
 if(bad==='throw')throw Error('exporter failed');if(bad==='json'){await fs.writeFile(args[7],'{');return}
 const rows=input.blocks.map(b=>({...oldRow,block:b.hash,claim:expected.claim,class_id:expected.class_id,executor_bond:'5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669:0',daa:b.header.daaScore,ts:b.header.timestamp,prompt_text:'never publish this synthetic content',accepted:true,phase:'final'}));
 if(bad==='pwu')rows[0].pwu={private:'not metadata'};if(bad==='anchor')rows[0].anchor={private:'not metadata'};if(bad==='bond')rows[0].executor_bond='('+hash(1)+', 4294967296)';
 await fs.writeFile(args[7],JSON.stringify({network:'testnet-12',hash_mismatches:0,rows:bad==='missing'?[]:rows,fp_rows:[]}));
}}
test('actual public model header reaches metadata exporter, history merges and no admission/text is inferred',async t=>{
 const {o,dir}=await setup(t),rpc=source();const r=await runOnce(o,{rpc,exporter:exporter()});
 const feed=JSON.parse(await fs.readFile(o.output,'utf8'));assert.equal(feed.rows.length,2);assert.equal(feed.rows[0].claim,expected.claim);assert.equal(feed.rows[0].prompt_ids,null);assert.equal(feed.rows[0].accepted,undefined);assert.equal(feed.rows[0].phase,undefined);assert.equal(feed.rows[0].prompt_text,undefined);assert.equal(r.complete,true);assert(r.rpcCalls<=o.maxRpc);assert.equal(JSON.parse(await fs.readFile(path.join(dir,'cursor.json'),'utf8')).hash,SINK);
});
test('raw JSON u64 precision survives into the exact header dump, without touching quoted text',async t=>{
 const parsed=losslessJson('{"nonce":9007199254740993,"note":"\\\"nonce\\\":9007199254740993"}');assert.equal(parsed.nonce,'9007199254740993');assert.equal(parsed.note,'"nonce":9007199254740993');
 const clocks=losslessJson('{"daaScore":18446744073709551615,"blueScore":9007199254740993,"timestamp":9007199254740995}');assert.equal(clocks.daaScore,'18446744073709551615');assert.equal(clocks.blueScore,'9007199254740993');assert.equal(clocks.timestamp,'9007199254740995');
 const {o}=await setup(t),model=structuredClone(fixture);model.header.nonce=parsed.nonce;let seen;
 await runOnce(o,{rpc:source({model}),exporter:exporter({observe:d=>{seen=d.blocks[0].header.nonce}})});assert.equal(seen,'9007199254740993');
 // Transport fixture only: changing nonce invalidates the old block hash; real Rust validation is tested separately.
});
test('source errors, exporter failures, malformed JSON and missing rows preserve feed and cursor bytes',async t=>{
 for(const failure of ['rpc','throw','json','missing','header','pwu','anchor','bond']){
  const {o,dir}=await setup(t),cursor=path.join(dir,'cursor.json');await fs.writeFile(cursor,JSON.stringify({genesis:GENESIS,hash:GENESIS}));const before=await fs.readFile(o.output,'utf8'),beforeCursor=await fs.readFile(cursor,'utf8');
  const model=structuredClone(fixture);if(failure==='header')delete model.header.evmPayloadHash;
  await assert.rejects(runOnce(o,{rpc:source({model,fail:failure==='rpc'?'getBlocks':null}),exporter:exporter({bad:failure})}));
  assert.equal(await fs.readFile(o.output,'utf8'),before);assert.equal(await fs.readFile(cursor,'utf8'),beforeCursor);
 }
});
test('page limit advances only through an exported page, next run catches up without erasing history',async t=>{
 const {o,dir}=await setup(t);o.maxPages=1;let r=await runOnce(o,{rpc:source({pages:[[fixture,heartbeat(MID,GENESIS)]]}),exporter:exporter()});assert.equal(r.complete,false);assert.equal(r.pages,1);assert.equal(r.processedHash,MID);assert.equal(JSON.parse(await fs.readFile(path.join(dir,'cursor.json'),'utf8')).hash,MID);
 r=await runOnce(o,{rpc:source({pages:[[heartbeat(SINK,MID)]]}),exporter:exporter()});assert.equal(r.complete,true);assert.equal(r.processedHash,SINK);assert.equal(JSON.parse(await fs.readFile(o.output,'utf8')).rows.length,2);
});
test('retired/reorg cursor is reacquired from a bounded recent window',async t=>{
 const {o,dir}=await setup(t),retired=hash(123);await fs.writeFile(path.join(dir,'cursor.json'),JSON.stringify({genesis:GENESIS,hash:retired}));
 const rpc=source({badCursor:retired});const r=await runOnce(o,{rpc,exporter:exporter()});assert.equal(r.complete,true);assert(rpc.calls.some(([m,p])=>m==='getBlock'&&p.hash===SINK));assert(rpc.calls.length<=o.maxRpc);
});
test('RPC budget and run deadline preserve the previous feed',async t=>{
 for(const mode of ['budget','deadline']){
  const {o}=await setup(t),before=await fs.readFile(o.output,'utf8'),rpc=source();
  if(mode==='budget'){o.maxRpc=8;o.lookback=6;const call=rpc.call;rpc.call=async(m,p)=>m==='getBlock'?heartbeat(p.hash,hash(Number(BigInt('0x'+p.hash))-1)):call(m,p)}
  else{o.timeoutMs=100;const call=rpc.call;rpc.call=async(...a)=>{await new Promise(r=>setTimeout(r,40));return call(...a)}}
  await assert.rejects(runOnce(o,{rpc,exporter:exporter()}));assert.equal(await fs.readFile(o.output,'utf8'),before);
 }
});

test('actual immutable exporter CLI output is normalized without inventing admission or decoding text',async t=>{
 const smoke=JSON.parse(await fs.readFile(new URL('../../../docs/evidence/palw/model-panel-one-2026-10-31/jobs-publisher-exporter-481352-cli-smoke.json',import.meta.url),'utf8'));
 const {o}=await setup(t);await runOnce(o,{rpc:source(),exporter:async(_exe,args)=>{await fs.writeFile(args[7],JSON.stringify(smoke.output))}});
 const row=JSON.parse(await fs.readFile(o.output,'utf8')).rows.find(r=>r.block===fixture.header.hash);
 assert.equal(row.claim,expected.claim);assert.equal(row.class_id,expected.class_id);assert.equal(row.executor_bond,canonicalBond(smoke.output.rows[0].executor_bond));assert.equal(row.executor_bond,smoke.output.rows[0].executor_bond.slice(1,-1).replace(', ',':'));
 assert.equal(row.execution_root,smoke.output.rows[0].execution_root);assert.equal(row.prompt_ids,null);assert.equal(row.acceptance,undefined);assert.equal(row.phase,undefined);
});
test('outpoint tuple and canonical forms are strict hex/u32 identities',()=>{
 const tx=hash(1);assert.equal(canonicalBond('('+tx+', 4294967295)'),tx+':4294967295');assert.equal(canonicalBond(tx+':0'),tx+':0');
 for(const s of [tx+':4294967296','('+tx+', 4294967296)',tx+':00','('+tx+', -1)','('+tx+', 1.0)','('+tx+', 0, 1)',tx.slice(1)+':0',tx+':1 trailing'])assert.throws(()=>canonicalBond(s));
});
test('initial and mid-run unsynced source preserve exact feed/cursor, never infer acceptance',async t=>{
 for(const when of ['initial','publication']){
  const {o,dir}=await setup(t);const cursor=path.join(dir,'cursor.json');await fs.writeFile(cursor,JSON.stringify({genesis:GENESIS,hash:GENESIS}));const before=await fs.readFile(o.output,'utf8'),beforeCursor=await fs.readFile(cursor,'utf8');
  const rpc=source(),call=rpc.call;let reads=0,exports=0;rpc.call=async(m,p)=>m==='getInfo'?{isSynced:when==='publication'&&++reads===1}:call(m,p);
  await assert.rejects(runOnce(o,{rpc,exporter:async(...args)=>{exports++;return exporter()(...args)}}),/source/);assert.equal(exports,when==='initial'?0:1);assert.equal(await fs.readFile(o.output,'utf8'),before);assert.equal(await fs.readFile(cursor,'utf8'),beforeCursor);
 }
});
test('cursor RPC timeout or unknown errors do not masquerade as a retired header',async t=>{
 const {o,dir}=await setup(t),cursor=path.join(dir,'cursor.json'),saved=hash(123);await fs.writeFile(cursor,JSON.stringify({genesis:GENESIS,hash:saved}));const before=await fs.readFile(o.output,'utf8'),beforeCursor=await fs.readFile(cursor,'utf8');
 const rpc=source(),call=rpc.call;rpc.call=async(m,p)=>{if(m==='getBlock'&&p.hash===saved)throw rpcFailure(m,'temporary storage unavailable');return call(m,p)};
 await assert.rejects(runOnce(o,{rpc,exporter:exporter()}));assert.equal(await fs.readFile(o.output,'utf8'),before);assert.equal(await fs.readFile(cursor,'utf8'),beforeCursor);
 assert.equal(rpcFailure('getBlock','cannot find header '+saved).retiredHeader,true);assert.equal(rpcFailure('getInfo','cannot find header '+saved).retiredHeader,false);
});

test('overlapping publishers preserve feed and stale-lock recovery cannot steal a new owner',async t=>{
 const {o,dir}=await setup(t),lock=path.join(dir,'publisher.lock'),before=await fs.readFile(o.output,'utf8');
 await fs.writeFile(lock,String(process.pid));await assert.rejects(runOnce(o,{rpc:source(),exporter:exporter()}),/already running/);assert.equal(await fs.readFile(o.output,'utf8'),before);assert.equal(await fs.readFile(lock,'utf8'),String(process.pid));
 await fs.unlink(lock);let release,started;const gate=new Promise(r=>release=r),begin=new Promise(r=>started=r);const actual=exporter();
 // A vanished owner is represented by a high PID whose absence is verified, not assumed.
 let dead=2147483647;assert.throws(()=>process.kill(dead,0),e=>e.code==='ESRCH');await fs.writeFile(lock,String(dead));
 const first=runOnce(o,{rpc:source(),exporter:async(...a)=>{started();await gate;return actual(...a)}});await begin;
 try{await assert.rejects(runOnce(o,{rpc:source(),exporter:exporter()}),/already running/);assert.equal(await fs.readFile(lock,'utf8'),String(process.pid));assert.equal(await fs.readFile(o.output,'utf8'),before)}finally{release();await first}
 assert.equal(JSON.parse(await fs.readFile(o.output,'utf8')).rows.length,2);
});
