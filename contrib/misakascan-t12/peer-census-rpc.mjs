#!/usr/bin/env node
// Read-only runtime identity; no headers, transactions, keys or material.
const url=process.argv[2]||'ws://127.0.0.1:26314';
const ws=new WebSocket(url),pending=new Map();let id=0,timer;
const deadline=new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('RPC deadline exceeded')),10000)});
async function snapshot(){
 await new Promise((r,j)=>{ws.onopen=r;ws.onerror=()=>j(Error('RPC connection unavailable'))});
 ws.onmessage=e=>{try{const v=JSON.parse(e.data),p=pending.get(v.id);if(p){pending.delete(v.id);v.error||v.params?.error?p.j(Error('RPC method unavailable')):p.r(v.params)}}catch(error){for(const p of pending.values())p.j(error)}};
 ws.onclose=()=>{for(const p of pending.values())p.j(Error('RPC closed'))};
 const call=method=>new Promise((r,j)=>{const n=++id;pending.set(n,{r,j});ws.send(JSON.stringify({id:n,method,params:{}}))});
 const [info,dag,status]=await Promise.all(['getInfo','getBlockDagInfo','getPalwNodeStatus'].map(call));
 return {network:dag.network,daa:dag.virtualDaaScore,p2pId:info.p2pId,isSynced:info.isSynced,genesisHash:status.genesisHash,params:status.consensusParamsId,schedule:status.consensusScheduleId,fences:status.fenceSchedule};
}
try{console.log(JSON.stringify(await Promise.race([snapshot(),deadline])))}catch(error){console.error('census RPC: '+error.message);process.exitCode=1}finally{clearTimeout(timer);ws.close()}
