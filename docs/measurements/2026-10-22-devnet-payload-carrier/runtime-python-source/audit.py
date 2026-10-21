#!/usr/bin/env python3
"""Offline sender trace audit; Borsh DTO bytes are NOT P2P/block-wire bytes."""
import argparse, json, math, statistics, zlib
from pathlib import Path
from trace_io import iter_rows
ENCODING='borsh_transaction_v1'
def rows(path): return list(iter_rows(path))
def full_id(v): return isinstance(v,str) and len(v)==128 and all(c in '0123456789abcdefABCDEF' for c in v)
def audit(sender,observer):
    errors=[]; starts=[r for r in sender if r.get('ev')=='measurement_started'];ends=[r for r in sender if r.get('ev')=='measurement_ended']
    if len(starts)!=1 or len(ends)!=1: errors.append('exact measurement start/end missing')
    attempts={};results={};known={};all_ids=set();unknown_ids=set();rejected_ids=set();success_attempts=duplicates=0
    for r in sender:
        if r.get('ev') not in ('submit_attempt','submit'): continue
        if r.get('phase') not in ('split','load'): errors.append('unknown submit phase');continue
        if r['phase']!='load':continue
        seq=r.get('sequence');txid=r.get('txid')
        if not isinstance(seq,int) or isinstance(seq,bool) or not full_id(txid): errors.append('malformed attempt identity');continue
        txid=txid.lower();all_ids.add(txid)
        dest=attempts if r['ev']=='submit_attempt' else results
        if seq in dest: errors.append('duplicate attempt/result sequence')
        dest[seq]=r
        if r['ev']=='submit_attempt':continue
        if seq not in attempts:errors.append('RPC result precedes durable attempt')
        fields=['t_unix_ms','payload_bytes','serialized_bytes','mass','compute_mass','storage_mass','transient_mass']
        if not all(isinstance(r.get(k),int) and not isinstance(r[k],bool) and r[k]>=0 for k in fields):
            errors.append('malformed numeric submit facts');unknown_ids.add(txid);continue
        if r.get('serialization')!=ENCODING: errors.append('unknown serialized byte encoding');unknown_ids.add(txid);continue
        if r['mass']!=max(r['compute_mass'],r['storage_mass'],r['transient_mass']): errors.append('mass scalar differs from native dimensions')
        outcome=r.get('rpc_outcome')
        if outcome=='accepted':
            if r.get('success') is not True or r.get('rpc_txid')!=r['txid']:
                errors.append('RPC acceptance lacks matching returned txid');unknown_ids.add(txid);continue
            success_attempts+=1
            if txid in known:
                duplicates+=1
                if any(known[txid][k]!=r[k] for k in ['payload_bytes','serialized_bytes','mass','compute_mass','storage_mass','transient_mass']): errors.append('same txid conflicting byte/mass facts')
            else:known[txid]=r
        elif outcome=='rejected':
            # Only an explicit typed outcome may say rejected. Current sender never generates this.
            if r.get('success') is not False:errors.append('inconsistent rejection outcome')
            rejected_ids.add(txid)
        elif outcome=='unknown':
            unknown_ids.add(txid)
            if r.get('success') is not False:errors.append('inconsistent unknown outcome')
        else:errors.append('RPC outcome missing/unknown schema');unknown_ids.add(txid)
    for seq,r in attempts.items():
        if seq not in results: unknown_ids.add(r['txid'].lower());errors.append('durable attempt has no RPC outcome')
        elif results[seq]['txid']!=r['txid']:errors.append('attempt/result txid mismatch')
    if set(results)-set(attempts):errors.append('RPC result has no prior durable attempt')
    ordered=sorted(attempts)
    if any(b-a!=1 for a,b in zip(ordered,ordered[1:])):errors.append('load sequence gap')
    accepted={};mempool={};daa_ticks=[];removed_count=0;carrying={}
    obs_starts=[];obs_ends=[];observed_methods=set();observer_rpc_error=False
    for r in observer:
        if r.get('ev')=='start':obs_starts.append(r)
        if r.get('ev')=='end':obs_ends.append(r)
        if r.get('ev')=='rpc_error':observer_rpc_error=True
        if r.get('ev')=='rpc':observed_methods.add(r.get('method'))
        if r.get('ev')=='rpc' and r.get('method')=='getVirtualChainFromBlock':
            removed=set(r['response'].get('removedChainBlockHashes',[]));removed_count+=len(removed)
            accepted={k:v for k,v in accepted.items() if v['accepting_block'] not in removed}
        if r.get('ev')=='selected_chain_acceptance' and r.get('txid','').lower() in all_ids:
            txid=r['txid'].lower()
            if not full_id(r.get('accepting_block')) or not isinstance(r.get('accepting_daa'),int):errors.append('malformed accepting evidence')
            else:accepted[txid]=r
        if r.get('ev')=='first_mempool_observation' and r.get('txid','').lower() in all_ids:mempool.setdefault(r['txid'].lower(),r)
        if r.get('ev')=='rpc' and r.get('method')=='getBlockDagInfo':daa_ticks.append((r['utc'],r['response']['virtualDaaScore']))
        if r.get('ev')=='rpc' and r.get('method')=='getBlocks':
            for b in r['response'].get('blocks',[]):
                for tx in b.get('transactions',[]):
                    tid=(tx.get('verboseData') or {}).get('transactionId','').lower()
                    if tid in all_ids:carrying.setdefault(tid,[]).append(dict(block=(b.get('verboseData') or {}).get('hash'),carrying_header_daa=b['header']['daaScore']))
    elapsed=target=intended=configured_duration=None
    if len(starts)==len(ends)==1:
        try:
            elapsed=(ends[0]['elapsed_ms']-starts[0]['elapsed_ms'])/1000;target=starts[0]['target_tps'];intended=starts[0]['target_count'];configured_duration=starts[0]['duration_secs']
            if not all(math.isfinite(v) for v in [elapsed,target,intended,configured_duration]) or elapsed<=0 or target<=0 or intended<0 or configured_duration<=0:raise ValueError()
            if starts[0]['confirmed_utxos']<intended:errors.append('load started without declared independent pool')
        except (KeyError,TypeError,ValueError):errors.append('invalid measurement target/duration/pool');elapsed=None
    run_completed=len(ends)==1 and ends[0].get('success') is True
    pairs=[]
    for tid,r in accepted.items():
        if tid in mempool:
            m=mempool[tid];pairs.append(dict(txid=tid,first_mempool_observed_daa=m['observed_daa'],accepting_daa=r['accepting_daa'],
                accepting_block=r['accepting_block'],observed_seconds=r['utc']-m['utc'],carrying_observations=carrying.get(tid,[])))
    complete=not errors and not unknown_ids
    observation_errors=[]
    if len(obs_starts)!=1 or len(obs_ends)!=1: observation_errors.append('observer start/end missing or duplicated')
    elif any(r.get('observer_schema')!=1 for r in obs_starts+obs_ends): observation_errors.append('observer schema missing or unsupported')
    if observer_rpc_error:observation_errors.append('observer RPC error/gap recorded')
    if len(daa_ticks)<2 or max((d for _,d in daa_ticks),default=0)<=min((d for _,d in daa_ticks),default=0):
        observation_errors.append('DAA progress coverage absent')
    if elapsed is not None and len(starts)==len(ends)==1 and daa_ticks:
        window_start=starts[0].get('t_unix_ms',float('inf'))/1000
        window_end=ends[0].get('t_unix_ms',float('-inf'))/1000
        if abs((window_end-window_start)-elapsed)>1:
            observation_errors.append('wall clock shift makes sender/observer interval ambiguous')
        if min(t for t,_ in daa_ticks)>window_start or max(t for t,_ in daa_ticks)<window_end:
            observation_errors.append('DAA observations do not bracket the whole load interval')
    else:observation_errors.append('load interval cannot be covered by DAA observations')
    if not {'getBlockDagInfo','getVirtualChainFromBlock','getMempoolEntries','getBlocks'}.issubset(observed_methods):
        observation_errors.append('required observation API coverage absent')
    observer_complete=not observation_errors
    return dict(kind='offline trace audit, not new measurement or service guarantee',prerequisites_pass=complete and run_completed and observer_complete,
        sender_trace_complete=complete,observer_complete=observer_complete,observation_errors=observation_errors,
        trace_well_formed=not errors,run_completed=run_completed,errors=sorted(set(errors)),initial_pool_target_count=intended,configured_load_duration_seconds=configured_duration,nominal_load_attempts_at_target_rate=target*configured_duration if target is not None and configured_duration is not None else None,target_tps=target,measurement_seconds=elapsed,duration_clock='sender elapsed_ms monotonic',
        recorded_load_attempts=len(attempts),rpc_success_attempts=success_attempts,duplicate_success_attempts=duplicates,
        known_matching_rpc_successes=len(known),known_rpc_rejected_ids=sorted(rejected_ids),unknown_rpc_outcome_txids=sorted(unknown_ids),
        actual_rpc_successes=len(known) if complete else None,achieved_unique_rpc_success_tps=len(known)/elapsed if complete and elapsed else None,
        serialization=ENCODING,bytes_scope='signed Transaction DTO Borsh encoding; excludes P2P framing; not comparable to old block byte metric without encoder identity',
        known_unique_payload_bytes=sum(r['payload_bytes'] for r in known.values()),known_unique_borsh_dto_bytes=sum(r['serialized_bytes'] for r in known.values()),
        known_selected_chain_accepted_attempt_ids=len(accepted),selected_chain_accepted_attempt_ids=len(accepted) if observer_complete else None,rpc_unknown_but_chain_observed_ids=sorted(set(accepted)&unknown_ids),reorg_detached_blocks_observed=removed_count,
        unobserved_attempt_ids=sorted(all_ids-set(accepted)) if observer_complete else None,measurement_daa_observations=daa_ticks,paired_observed_latencies=pairs,
        p99=None,saturation='UNDETERMINED; achieved offered rate/backlog and observation scope require separate analysis',
        limits=['RPC timeout/error/mismatched response can follow admission; never inferred to be rejected.',
                'Separate DAG/UTXO calls are observations, not an atomic state snapshot.',
                'Carrying header DAA, accepting block DAA and mempool observation DAA are distinct.',
                'Last observed selected-chain acceptance is not irreversible finality; no door/cost/capacity guarantee.'])
def audit_paths(sender_path,observer_path):
    observer=iter_rows(observer_path)
    try:return audit(rows(sender_path),observer)
    except (OSError,EOFError,ValueError,TypeError,KeyError,zlib.error) as error:
        return dict(kind='offline trace audit failed',prerequisites_pass=False,audit_status='incomplete_or_invalid_evidence',
                    error=f'{type(error).__name__}: {error}',actual_rpc_successes=None,selected_chain_accepted_attempt_ids=None)
    finally:observer.close()

def main():
    p=argparse.ArgumentParser();p.add_argument('--sender',type=Path,required=True);p.add_argument('--observer',type=Path,required=True)
    a=p.parse_args();result=audit_paths(a.sender,a.observer);print(json.dumps(result,indent=2))
    if result.get('audit_status')=='incomplete_or_invalid_evidence':raise SystemExit(2)
if __name__=='__main__':main()
