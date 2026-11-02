#!/usr/bin/env node
// Public header metadata only. No material, answer, model, outbox or key paths are inputs.
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
const exec = promisify(execFile);
export const GENESIS='a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23';
const FLOOR='f1c5635c6e47e96e7af864789c94523335dc56584af297cb8cc19021c228b897bee1a50145597e45f8ca2727349bf4aa352a98cc05274b7f059a176642f623c8';
const hex=v=>typeof v==='string'&&/^[0-9a-f]{128}$/.test(v);
const defaults={rpc:'ws://127.0.0.1:38314',lookback:24,overlap:12,maxPages:6,maxRpc:64,timeoutMs:60000,maxRows:512};
export function options(input){
 const o={...defaults,...input};
 for(const [key,min,max] of [['lookback',1,64],['overlap',0,24],['maxPages',1,12],['maxRpc',8,128],['timeoutMs',100,90000],['maxRows',1,512]]){
  if(!Number.isInteger(o[key])||o[key]<min||o[key]>max)throw Error('invalid limit '+key);
 }
 if(o.overlap>o.lookback)throw Error('overlap exceeds lookback');
 for(const k of ['workdir','output','exporter'])if(!path.isAbsolute(o[k]||''))throw Error(k+' must be an absolute path');
 return o;
}
// Scan JSON tokens before parsing: JSON.parse revivers see already-rounded numbers.
export function losslessJson(raw){
 if(typeof raw!=="string"||Buffer.byteLength(raw)>16*1024*1024)throw Error('JSON byte limit');
 const number=/-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y,chunks=[];let i=0,start=0;
 while(i<raw.length){
  if(raw[i]==='"'){i++;while(i<raw.length){if(raw[i]==='\\'){i+=2;continue}if(raw[i++]==='"')break}continue}
  if(raw[i]==='-'||/[0-9]/.test(raw[i])){
   number.lastIndex=i;const m=number.exec(raw);if(m){const token=m[0];if(token.length>128)throw Error('JSON number length limit');
    if(!/[.eE]/.test(token)&&(BigInt(token)>BigInt(Number.MAX_SAFE_INTEGER)||BigInt(token)<BigInt(Number.MIN_SAFE_INTEGER))){chunks.push(raw.slice(start,i),JSON.stringify(token));start=number.lastIndex}
    i=number.lastIndex;continue;
   }
  }
  i++;
 }
 chunks.push(raw.slice(start));return JSON.parse(chunks.join(''));
}
const uint=v=>(typeof v==='number'&&Number.isSafeInteger(v)&&v>=0)||(typeof v==='string'&&/^(?:0|[1-9]\d{0,19})$/.test(v)&&BigInt(v)<=18446744073709551615n);
async function readJson(file,missing){try{return losslessJson(await fs.readFile(file,'utf8'))}catch(e){if(e.code==='ENOENT')return missing;throw e}}
async function atomic(file,value){
 const temporary=file+'.next-'+process.pid;
 try{const h=await fs.open(temporary,'wx',0o644);try{await h.writeFile(JSON.stringify(value)+'\n');await h.sync()}finally{await h.close()}await fs.rename(temporary,file)}finally{await fs.unlink(temporary).catch(()=>{})}
}
export function rpcFailure(method,wire){
 const message=typeof wire==='string'?wire:typeof wire?.message==='string'?wire.message:'';
 const error=Error('RPC '+method+' failed');
 error.retiredHeader=method==='getBlock'&&/cannot find (?:header|full block) [0-9a-f]{128}(?![0-9a-f])/i.test(message);
 return error;
}
export function canonicalBond(value){
 const match=typeof value==='string'&&(value.match(/^([0-9a-f]{128}):((?:0|[1-9]\d{0,9}))$/)||value.match(/^\(([0-9a-f]{128}), ?((?:0|[1-9]\d{0,9}))\)$/));
 if(!match||BigInt(match[2])>4294967295n)throw Error('invalid executor outpoint');
 return match[1]+':'+match[2];
}
export async function connect(url,remaining){
 const connectMs=Math.min(5000,remaining()),socket=new WebSocket(url),pending=new Map();let seq=0;
 await new Promise((resolve,reject)=>{const t=setTimeout(()=>{socket.close();reject(Error('RPC connect timeout'))},connectMs);socket.onopen=()=>{clearTimeout(t);resolve()};socket.onerror=()=>{clearTimeout(t);reject(Error('RPC connect failed'))}});
 socket.onmessage=e=>{
  try{
   if(typeof e.data!=='string'||Buffer.byteLength(e.data)>16*1024*1024)throw Error('RPC message limit');
   const r=losslessJson(e.data),p=pending.get(r.id);if(!p)return;pending.delete(r.id);clearTimeout(p.t);
   r.error||r.params?.error?p.reject(rpcFailure(p.method,r.error||r.params.error)):p.resolve(r.params);
  }catch(error){for(const p of pending.values()){clearTimeout(p.t);p.reject(error)}pending.clear();socket.close()}
 };
 socket.onclose=()=>{for(const p of pending.values()){clearTimeout(p.t);p.reject(Error('RPC closed'))}pending.clear()};
 return {call(method,params){return new Promise((resolve,reject)=>{const id=++seq,t=setTimeout(()=>{pending.delete(id);reject(Error('RPC '+method+' timeout'))},Math.min(5000,remaining()));pending.set(id,{resolve,reject,t,method});socket.send(JSON.stringify({id,method,params}))})},close(){socket.close()}};
}
function blockOf(r){
 const b=r?.block||r;
 if(!b?.header||!hex(b.header.hash))throw Error('incomplete RPC block header');
 for(const key of ['daaScore','blueScore','timestamp'])if(!uint(b.header[key]))throw Error('invalid header '+key);
 return b;
}
function modelOf(header){
 if(Number(header.powAlgoId)!==6)return null;
 const raw=header.palwCommitment;
 if(Array.isArray(raw)&&raw.some(v=>!Number.isInteger(v)||v<0||v>255))throw Error('invalid commitment bytes');
 const bytes=Array.isArray(raw)?Buffer.from(raw):typeof raw==='string'&&/^(?:[0-9a-f]{2})+$/i.test(raw)?Buffer.from(raw,'hex'):null;
 if(!bytes||!['PAV2','PAV3'].includes(bytes.subarray(0,4).toString()))throw Error('unsupported native attempt header');
 const base=bytes[3]===51?6:4;
 if(base===6&&bytes.readUInt16LE(4)!==3)throw Error('unsupported PAV3 version');
 if(bytes.length<base+262)throw Error('short native attempt');
 const classId=bytes.subarray(base+130,base+194).toString('hex');
 if(classId===FLOOR)return null;
 const bond=bytes.subarray(base+194,base+258).toString('hex')+':'+bytes.readUInt32LE(base+258);
 return {classId,bond};
}
function validateModelHeader(h){
 for(const key of ['hashMerkleRoot','acceptedIdMerkleRoot','utxoCommitment','pruningPoint','evmPayloadHash','evmCommitmentRoot','overlayCommitmentRoot','palwStateRoot'])if(!hex(h[key]))throw Error('incomplete model header '+key);
 if(!Array.isArray(h.parentsByLevel)||h.parentsByLevel.some(level=>!Array.isArray(level)||level.some(v=>!hex(v))))throw Error('invalid model parents');
 for(const key of ['version','bits'])if(!Number.isSafeInteger(h[key])||h[key]<0)throw Error('unsafe model '+key);
 if(!uint(h.nonce))throw Error('unsafe model nonce');
 if(typeof h.blueWork!=='string'||! /^[0-9a-fA-F]+$/.test(h.blueWork))throw Error('invalid model blueWork');
}
function metadata(row){
 if(!row||!hex(row.block)||!hex(row.claim)||!hex(row.class_id)||!hex(row.execution_root))throw Error('incomplete exported identity');
 if(!uint(row.ts)||!uint(row.daa))throw Error('invalid exported clock');
 const bond=canonicalBond(row.executor_bond);
 if(row.pwu!=null&&!uint(row.pwu))throw Error('invalid exported work metadata');
 if(row.anchor!=null&&!hex(row.anchor))throw Error('invalid exported anchor');
 const out={};
 for(const k of ['block','claim','class_id','executor_bond','execution_root','daa','ts','pwu','anchor'])if(row[k]!=null)out[k]=row[k];
 out.class=typeof row.class==='string'?row.class.slice(0,256):row.class_id;
 // Metadata-only publication, even if a historical exporter knows a prompt codec.
 return {...out,executor_bond:bond,prompt_ids:null,prompt_derived:false,prefill:null,decode:null,disclosed:[]};
}
function merge(existing,incoming,maxRows){
 const rows=new Map();
 for(const row of [...existing,...incoming]){const r=metadata(row);rows.set(r.block,r)}
 return [...rows.values()].sort((a,b)=>BigInt(a.ts)===BigInt(b.ts)?a.block.localeCompare(b.block):BigInt(a.ts)>BigInt(b.ts)?-1:1).slice(0,maxRows);
}
export async function runOnce(input,deps={}){
 const o=options(input),deadline=Date.now()+o.timeoutMs;
 const remaining=()=>{const n=deadline-Date.now();if(n<=0)throw Error('publisher deadline exceeded');return n};
 await fs.mkdir(o.workdir,{recursive:true});
 const lock=path.join(o.workdir,'publisher.lock');let owned=false,rpc;
 try{
  try{const h=await fs.open(lock,'wx');owned=true;await h.writeFile(String(process.pid));await h.close()}
  catch(e){
   if(e.code!=='EEXIST')throw e;
   // Serialize stale-lock recovery, then reread: a second recovering process must not unlink a new owner's lock.
   const recovery=lock+'.recovery',guard=await fs.open(recovery,'wx');
   try{
    try{const pid=Number(await fs.readFile(lock,'utf8'));if(!Number.isInteger(pid)||pid<=0)throw Error('invalid publisher lock');try{process.kill(pid,0);throw Error('publisher already running')}catch(error){if(error.code!=='ESRCH')throw error}await fs.unlink(lock)}catch(error){if(error.code!=='ENOENT')throw error}
    const h=await fs.open(lock,'wx');owned=true;await h.writeFile(String(process.pid));await h.close();
   }finally{await guard.close();await fs.unlink(recovery)}
  }
  const old=await readJson(o.output,{network:'testnet-12',rows:[],fp_rows:[]});
  if(old.network!=='testnet-12'||!Array.isArray(old.rows)||!Array.isArray(old.fp_rows))throw Error('invalid existing feed');
  if(old.genesis&&old.genesis!==GENESIS)throw Error('existing feed genesis mismatch');
  if((old.rows.length||old.fp_rows.length)&&!old.genesis)throw Error('unversioned nonempty feed requires operator migration');
  rpc=deps.rpc||await connect(o.rpc,remaining);let calls=0;
  const call=async(method,params={})=>{remaining();if(++calls>o.maxRpc)throw Error('RPC call budget exceeded');return rpc.call(method,params)};
  const info=await call('getInfo'),dag=await call('getBlockDagInfo'),status=await call('getPalwNodeStatus');
  if(info?.isSynced!==true||dag?.network!=='testnet-12'||status?.genesisHash!==GENESIS||!hex(dag.sink))throw Error('source network/genesis/sync check failed');
  const cache=new Map();
  async function get(hash){if(cache.has(hash))return cache.get(hash);const b=blockOf(await call('getBlock',{hash,includeTransactions:false}));if(b.header.hash!==hash)throw Error('RPC returned another block');cache.set(hash,b);return b}
  async function rewind(hash,n){let h=hash;for(let i=0;i<n&&h!==GENESIS;i++){const b=await get(h),parent=b.verboseData?.selectedParentHash;if(!hex(parent))throw Error('selected parent unavailable');h=parent}return h}
  const saved=await readJson(path.join(o.workdir,'cursor.json'),null);let anchor=null;
  if(saved?.genesis===GENESIS&&hex(saved.hash)){
   try{const b=await get(saved.hash);if(b.verboseData?.isChainBlock===true)anchor=await rewind(saved.hash,o.overlap)}catch(error){if(!error.retiredHeader)throw error; /* explicitly retired header: bounded fresh acquisition */ }
  }
  if(!anchor)anchor=await rewind(dag.sink,o.lookback);
  let cursor=anchor,processed=anchor,complete=cursor===dag.sink,pages=0;const headers=new Map();
  for(;!complete&&pages<o.maxPages;){
   pages++;
   const r=await call('getBlocks',{lowHash:cursor,includeBlocks:true,includeTransactions:false});
   if(!Array.isArray(r?.blocks)||!r.blocks.length)throw Error('incomplete or stalled RPC page');
   if(r.blocks.length>256||headers.size+r.blocks.length>2048)throw Error('header page budget exceeded');
   let next=null;
   for(const raw of r.blocks){const b=blockOf(raw),h=b.header;cache.set(h.hash,b);headers.set(h.hash,h);if(b.verboseData?.isChainBlock===true&&(!next||BigInt(h.blueScore)>BigInt(next.blueScore)))next=h;if(h.hash===dag.sink)complete=true}
   if(complete){processed=dag.sink;break}
   if(!next||next.hash===cursor)throw Error('no selected-chain page progress');
   processed=cursor=next.hash;
  }
  const models=new Map();
  for(const h of headers.values()){const model=modelOf(h);if(model){validateModelHeader(h);models.set(h.hash,{header:h,model})}}
  const headersFile=path.join(o.workdir,'headers.next.json'),exportFile=path.join(o.workdir,'export.next.json');
  await atomic(headersFile,{blocks:[...models.values()].map(b=>({hash:b.header.hash,header:b.header}))});
  await fs.unlink(exportFile).catch(e=>{if(e.code!=='ENOENT')throw e});
  const args=['--blocks',headersFile,'--retention','/dev/null','--network','testnet-12','--out',exportFile];
  if(deps.exporter)await deps.exporter(o.exporter,args,remaining());else await exec(o.exporter,args,{timeout:remaining(),maxBuffer:1024*1024});
  remaining();const output=await readJson(exportFile,null);
  if(!output||output.network!=='testnet-12'||output.hash_mismatches!==0||!Array.isArray(output.rows)||!Array.isArray(output.fp_rows)||output.fp_rows.length||output.rows.length!==models.size)throw Error('incomplete exporter output');
  const seen=new Set();
  for(const row of output.rows){const r=metadata(row),m=models.get(r.block);if(!m||seen.has(r.block)||r.class_id!==m.model.classId||r.executor_bond!==m.model.bond||BigInt(r.daa)!==BigInt(m.header.daaScore)||BigInt(r.ts)!==BigInt(m.header.timestamp))throw Error('exported header identity mismatch');seen.add(r.block)}
  const feed={network:'testnet-12',genesis:GENESIS,generatedUtc:new Date().toISOString(),source:{rpc:o.rpc,daa:dag.virtualDaaScore,sink:dag.sink,params:status.consensusParamsId,schedule:status.consensusScheduleId,complete,processedHash:processed},rows:merge(old.rows,output.rows,o.maxRows),fp_rows:old.fp_rows.map(metadata).slice(0,o.maxRows)};
  if((await call('getInfo'))?.isSynced!==true)throw Error('source lost sync before publication');
  remaining();await atomic(o.output,feed);
  // Feed first: a crash cannot advance the cursor past metadata actually published.
  let cursorSaved=true;try{await atomic(path.join(o.workdir,'cursor.json'),{genesis:GENESIS,hash:processed,updatedUtc:feed.generatedUtc})}catch{cursorSaved=false}
  return {rows:feed.rows.length,newRows:output.rows.length,rpcCalls:calls,pages,complete,processedHash:processed,cursorSaved};
 }finally{rpc?.close?.();if(owned)await fs.unlink(lock).catch(()=>{})}
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const names={'--rpc':'rpc','--exporter':'exporter','--workdir':'workdir','--output':'output','--lookback':'lookback','--overlap':'overlap','--max-pages':'maxPages','--max-rpc':'maxRpc','--timeout-ms':'timeoutMs','--max-rows':'maxRows'},o={};
 try{for(let i=2;i<process.argv.length;i+=2){const name=names[process.argv[i]],v=process.argv[i+1];if(!name||v==null)throw Error('invalid argument');o[name]=['rpc','exporter','workdir','output'].includes(name)?v:Number(v)}console.log(JSON.stringify(await runOnce(o)))}catch(error){console.error('jobs publisher: '+error.message);process.exitCode=1}
}
