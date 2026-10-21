#!/usr/bin/env python3
"""Read-only loopback devnet observer; retains raw RPC evidence, never sends a tx.
No claim of serialized byte size or exact RPC-submitted count is made.
"""
import argparse, importlib.util, json, signal, time
from pathlib import Path
from trace_io import open_trace_writer

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--repo', required=True, type=Path)
    ap.add_argument('--port', required=True, type=int)
    ap.add_argument('--duration', type=float, default=600)
    ap.add_argument('--interval', type=float, default=1)
    ap.add_argument('--target-tps', type=float, required=True)
    ap.add_argument('--payload-bytes', type=int, default=17408)
    ap.add_argument('--output', type=Path, default=Path('observer.jsonl.gz'))
    ap.add_argument('--ready-file', type=Path)
    a = ap.parse_args()
    if a.duration <= 0 or a.interval <= 0 or a.target_tps <= 0:
        ap.error('duration, interval and target must be positive')
    module = a.repo / 'scripts/drills/misaka_wrpc_json.py'
    spec = importlib.util.spec_from_file_location('misaka_local_wrpc', module)
    mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
    rpc = mod.WsRpc(host='127.0.0.1', port=a.port)
    stop = [False]
    signal.signal(signal.SIGTERM, lambda *_: stop.__setitem__(0, True))
    signal.signal(signal.SIGINT, lambda *_: stop.__setitem__(0, True))
    start = time.monotonic()
    # Exclusive creation prevents silently concatenating unrelated runs.
    with open_trace_writer(a.output) as out:
        def record(ev, **values):
            json.dump(dict(ev=ev, utc=time.time(), elapsed=time.monotonic()-start, **values),out)
            out.write('\n');out.flush()
        def call(method, params=None):
            try:
                response = rpc.call(method, params)
            except Exception as error:
                record('rpc_error', method=method, error=str(error))
                raise
            record('rpc', method=method, params=params or {}, response=response)
            return response
        info = call('getBlockDagInfo')
        if info.get('network') != 'devnet':
            raise RuntimeError('refusing observer on any network other than explicit devnet')
        record('start', observer_schema=1, target_tps=a.target_tps, intended_count=a.target_tps*a.duration,
               payload_bytes=a.payload_bytes, exact_rpc_submissions=None,
               qualification='observer only; sender success trace/fanout not authenticated')
        if a.ready_file:
            with a.ready_file.open('x') as ready:json.dump(dict(observer_schema=1,started=True),ready)
        cursor = info['sink']; low = cursor
        seen_blocks = set(); seen_mempool = set(); accepted_seen = set()
        while not stop[0] and time.monotonic()-start < a.duration:
            tick = time.monotonic()
            info = call('getBlockDagInfo')
            mem = call('getMempoolEntries', {'includeOrphanPool':True, 'filterTransactionPool':False})
            for entry in mem.get('mempoolEntries', []):
                tx = entry.get('transaction', {})
                payload = tx.get('payload', '')
                if isinstance(payload, str) and len(payload) == 2*a.payload_bytes:
                    txid = (tx.get('verboseData') or {}).get('transactionId')
                    if txid and txid not in seen_mempool:
                        seen_mempool.add(txid)
                        record('first_mempool_observation', txid=txid, observed_daa=info['virtualDaaScore'])
            chain = call('getVirtualChainFromBlock', {'startHash':cursor, 'includeAcceptedTransactionIds':True,
                         'minConfirmationCount':None})
            for group in chain.get('acceptedTransactionIds', []):
                accepting = group['acceptingBlockHash']
                block = call('getBlock', {'hash':accepting, 'includeTransactions':False})['block']
                daa = block['header']['daaScore']
                for txid in group.get('acceptedTransactionIds', []):
                    accepted_seen.add(txid)
                    record('selected_chain_acceptance', txid=txid, accepting_block=accepting, accepting_daa=daa)
            added = chain.get('addedChainBlockHashes', [])
            # Empty added list can follow a pure detach; resync cursor to actual sink, raw removal retained.
            cursor = added[-1] if added else info['sink']
            dag = call('getBlocks', {'lowHash':low, 'includeBlocks':True, 'includeTransactions':True})
            hashes = dag.get('blockHashes', [])
            for block in dag.get('blocks', []):
                blockid = (block.get('verboseData') or {}).get('hash')
                if not blockid or blockid in seen_blocks: continue
                seen_blocks.add(blockid)
                # Raw body already retained above. No JSON-byte-count proxy for serialized wire bytes.
                record('dag_block_observation', block=blockid, carrying_header_daa=block['header']['daaScore'])
            if hashes: low = hashes[-1]
            delay = a.interval-(time.monotonic()-tick)
            if delay > 0: time.sleep(delay)
        record('end', observer_schema=1, unique_payload_mempool_observed=len(seen_mempool), unique_dag_blocks=len(seen_blocks),
               all_selected_accepted_ids_observed=len(accepted_seen), exact_rpc_submissions=None,
               measured_offered_rate=None, saturation='UNDETERMINED: requires sender trace and queue/rate analysis')
    rpc.close()
if __name__ == '__main__': main()
