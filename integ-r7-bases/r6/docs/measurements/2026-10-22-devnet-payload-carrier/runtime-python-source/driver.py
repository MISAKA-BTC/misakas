#!/usr/bin/env python3
"""Prepared isolated measurement launcher. Default is preflight-only, no tx/node start.
Requires the reviewed MLDSA measurement candidate capabilities and pinned binary manifest.
"""
import argparse, hashlib, json, os, signal, stat, subprocess, time
from pathlib import Path
from owned_process import cleanup_owned
from native_peers import require_loopback_peers_v1

def prepare(a):
    if not (1<=a.tps<=100 and 1<=a.seconds<=3600 and 1<=a.fanout_timeout<=3600 and 0<=a.payload_bytes<=20000):
        raise ValueError('sender native tooling bounds exceeded before binary/key/RPC use')
    work = a.work.resolve(strict=True)
    if stat.S_IMODE(work.stat().st_mode) != 0o700: raise ValueError('fresh workdir must be mode 0700')
    key = a.key.resolve(strict=True)
    if not key.is_relative_to(work) or stat.S_IMODE(key.stat().st_mode) != 0o600:
        raise ValueError('measurement key must be inside isolated workdir and mode 0600')
    manifest = json.loads(a.manifest.read_text())
    if manifest.get('network') != 'devnet' or manifest.get('key_origin') != 'isolated-devnet-measurement':
        raise ValueError('manifest must identify isolated devnet key provenance')
    sender = a.sender.resolve(strict=True)
    sha = hashlib.sha256(sender.read_bytes()).hexdigest()
    if sha != manifest.get('sender_sha256') or not manifest.get('source_commit'):
        raise ValueError('sender binary is not pinned to reviewed source/build manifest')
    # This command must be pure/local and implemented explicitly by the reviewed port.
    caps = json.loads(subprocess.run([str(sender), '--measurement-capabilities'], check=True,
                                    capture_output=True, text=True, timeout=10).stdout)
    if caps.get('key_scheme') != 'MLDSA87' or caps.get('trace_version') != 1 or not caps.get('devnet_fanout'):
        raise ValueError('sender lacks reviewed MLDSA, per-submit evidence or fanout prerequisites')
    need = int(a.tps*a.seconds)
    if not 1 <= need <= 10000: raise ValueError('sender initial pool bound is 1..10000')
    if need != a.tps*a.seconds: raise ValueError('integer target count required')
    command = [str(sender), '--network', 'devnet', '--rpcserver', f'127.0.0.1:{a.grpc_port}',
               '--private-key-file', str(key), '--payload-size', str(a.payload_bytes), '--tps', str(a.tps),
               '--initial-utxos', str(need), '--measurement-duration-secs', str(a.seconds),
               '--measurement-log', str(work/'sender-events.jsonl'),
               '--measurement-funding-timeout-secs', str(a.fanout_timeout)]
    return work, command, sha, caps

def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['repo','work','sender','key','manifest']: p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--grpc-port', type=int, required=True); p.add_argument('--json-port', type=int, required=True)
    p.add_argument('--tps', type=int, default=1); p.add_argument('--seconds', type=int, default=600)
    p.add_argument('--payload-bytes', type=int, default=17408)
    p.add_argument('--fanout-timeout', type=int, default=3600)
    p.add_argument('--drain-seconds', type=int, default=300)
    p.add_argument('--run', action='store_true')
    a=p.parse_args()
    signal.signal(signal.SIGTERM,lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    if not (1<=a.tps<=100 and 1<=a.seconds<=3600 and 1<=a.fanout_timeout<=3600 and a.drain_seconds>0):
        p.error('sender requires TPS 1..100; duration/funding timeout 1..3600; positive drain')
    work, command, sha, caps = prepare(a)
    evidence = dict(kind='prepared launch only', sender_sha256=sha, capabilities=caps, command=command)
    if not a.run:
        print(json.dumps(evidence, indent=2)); return
    # The observer enforces devnet; run a network probe BEFORE starting any sender.
    import importlib.util
    spec=importlib.util.spec_from_file_location('wrpc',a.repo/'scripts/drills/misaka_wrpc_json.py')
    wrpc=importlib.util.module_from_spec(spec);spec.loader.exec_module(wrpc)
    rpc=wrpc.WsRpc(host='127.0.0.1',port=a.json_port)
    network=rpc.call('getBlockDagInfo')
    if network.get('network')!='devnet': raise ValueError('refusing nondevnet endpoint')
    # Probe all observation APIs before any sender is allowed to create/split a transaction.
    rpc.call('getMempoolEntries',{'includeOrphanPool':True,'filterTransactionPool':False})
    rpc.call('getVirtualChainFromBlock',{'startHash':network['sink'],'includeAcceptedTransactionIds':True,'minConfirmationCount':None})
    rpc.call('getBlocks',{'lowHash':network['sink'],'includeBlocks':True,'includeTransactions':True})
    peers=rpc.call('getConnectedPeerInfo')
    require_loopback_peers_v1(peers)
    rpc.close()
    for name in ['launch.json','sender.stdout','sender.stderr','sender-events.jsonl','observer.jsonl.gz','observer-ready.json']:
        if (work/name).exists(): raise ValueError('refusing evidence overwrite: '+name)
    (work/'launch.json').write_text(json.dumps(dict(evidence,network=network,peers=peers),indent=2))
    stdout=(work/'sender.stdout').open('xb');stderr=(work/'sender.stderr').open('xb')
    observer_out=(work/'observer.stdout').open('xb');observer_err=(work/'observer.stderr').open('xb')
    limit=a.fanout_timeout+a.seconds+a.drain_seconds
    obs=subprocess.Popen(['python3',str(Path(__file__).with_name('observe.py')),'--repo',str(a.repo),
        '--port',str(a.json_port),'--duration',str(limit),'--target-tps',str(a.tps),
        '--payload-bytes',str(a.payload_bytes),'--output',str(work/'observer.jsonl.gz'),'--ready-file',str(work/'observer-ready.json')],stdout=observer_out,stderr=observer_err)
    sender=None
    try:
        # An existing funded pool can enter load immediately; establish observation first.
        ready_deadline=time.monotonic()+30
        while True:
            if obs.poll() is not None: raise RuntimeError('observer failed before sender startup')
            evidence_path=work/'observer-ready.json'
            if evidence_path.exists():
                try:ready=json.loads(evidence_path.read_text())
                except json.JSONDecodeError:ready={}
                if ready.get('observer_schema')==1 and ready.get('started') is True:break
            if time.monotonic()>=ready_deadline: raise TimeoutError('observer readiness timeout')
            time.sleep(.1)
        sender=subprocess.Popen(command,cwd=work,stdout=stdout,stderr=stderr,start_new_session=True)
        deadline=time.monotonic()+a.fanout_timeout+a.seconds+60
        while sender.poll() is None:
            if obs.poll() is not None:
                raise RuntimeError('observer exited while sender was active; stopping the experiment')
            if time.monotonic()>=deadline:
                raise TimeoutError('sender failed to finish within declared fanout/load deadlines')
            time.sleep(.5)
        code=sender.returncode
        # Keep listening after sender exit. Unknown/failed drain is retained, never an acceptance claim.
        time.sleep(a.drain_seconds)
        (work/'sender-exit.json').write_text(json.dumps(dict(exit_code=code,utc=time.time())))
    except BaseException as error:
        (work/'driver-failure.json').write_text(json.dumps(dict(error=str(error),utc=time.time())))
        raise
    finally:
        owned=[]
        if sender is not None:owned.append((sender,'sender',True,(15,15,5)))
        owned.append((obs,'observer',False,(35,10,5)))  # Prefer graceful JSON/gzip footer completion.
        cleanup_owned(owned,work/'driver-cleanup.json')
        stdout.close();stderr.close();observer_out.close();observer_err.close()
if __name__=='__main__': main()
