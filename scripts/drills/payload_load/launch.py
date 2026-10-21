#!/usr/bin/env python3
"""Managed public-seed local devnet launcher. Default prints plan, starts nothing.
Four floor-only nodes and lane 0,2,10 follow the existing round-lane drill.
"""
import argparse, hashlib, importlib.util, ipaddress, json, os, signal, socket, subprocess, time
from pathlib import Path
from owned_process import cleanup_owned

def checked_binary(path,expected):
    path=path.resolve(strict=True)
    if hashlib.sha256(path.read_bytes()).hexdigest()!=expected:raise ValueError('unpinned binary: '+str(path))
    return path

def matching_funded_outputs(coins,funding_id,payer_spk,amount,observed_daa):
    """Observed actual entries only; never RPC submit acceptance or an atomic snapshot."""
    matched=[]
    for item in coins.get('entries',[]):
        outpoint=item.get('outpoint',{});entry=item.get('utxoEntry',{})
        script=entry.get('scriptPublicKey')
        if (outpoint.get('transactionId')==funding_id and entry.get('isCoinbase') is False
            and entry.get('amount')==amount and isinstance(script,str) and script.lower()==payer_spk
            and isinstance(outpoint.get('index'),int) and not isinstance(outpoint['index'],bool)
            and outpoint['index']>=0 and isinstance(entry.get('blockDaaScore'),int)
            and entry['blockDaaScore']<=observed_daa):matched.append(item)
    return matched

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for n in ['repo','work','manifest','kaspad','cli','sender']:p.add_argument('--'+n,type=Path,required=True)
    p.add_argument('--p2p-base',type=int,default=33100);p.add_argument('--borsh-base',type=int,default=33200)
    p.add_argument('--grpc-port',type=int,default=33300);p.add_argument('--json-port',type=int,default=33400)
    p.add_argument('--tps',type=int,default=1);p.add_argument('--seconds',type=int,default=600)
    p.add_argument('--fund-msk',type=int,default=1000);p.add_argument('--startup-timeout',type=int,default=600)
    p.add_argument('--funding-timeout',type=int,default=3600);p.add_argument('--run',action='store_true')
    a=p.parse_args();manifest=json.loads(a.manifest.read_text())
    signal.signal(signal.SIGTERM,lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    if manifest.get('network')!='devnet' or manifest.get('key_origin')!='isolated-devnet-measurement':raise ValueError('wrong manifest scope')
    if not (1<=a.tps<=100 and 1<=a.seconds<=3600 and 1<=a.funding_timeout<=3600 and 1<=a.tps*a.seconds<=10000):raise ValueError('sender bounds')
    if a.fund_msk<=0 or a.startup_timeout<=0:raise ValueError('positive funding and startup deadline required')
    bins={n:checked_binary(getattr(a,n),manifest.get(n+'_sha256')) for n in ['kaspad','cli','sender']}
    caps=json.loads(subprocess.run([str(bins['sender']),'--measurement-capabilities'],capture_output=True,text=True,check=True,timeout=10).stdout)
    if caps.get('key_scheme')!='MLDSA87' or caps.get('trace_version')!=1 or not caps.get('devnet_fanout'):raise ValueError('sender prerequisites absent')
    ports=[a.p2p_base+i for i in range(4)]+[a.borsh_base+i for i in range(4)]+[a.grpc_port,a.json_port]
    if len(set(ports))!=len(ports) or any(not 1024<=v<=65535 for v in ports):raise ValueError('unique nonprivileged ports required')
    sockets=[]
    try:
        for port in ports:
            sock=socket.socket();sock.bind(('127.0.0.1',port));sockets.append(sock)
    finally:
        for sock in sockets:sock.close()
    if a.work.exists():raise ValueError('fresh work directory must not already exist')
    plan=dict(kind='prepared isolated public-devnet run',ports=ports,binaries={n:str(b) for n,b in bins.items()},
              lane='0,2,10',nodes=4,funding_msk=a.fund_msk,measurement_count=a.tps*a.seconds,will_start=a.run)
    if not a.run:print(json.dumps(plan,indent=2));return
    a.work.mkdir(mode=0o700,parents=False);work=a.work.resolve();keys=work/'keys';keys.mkdir(mode=0o700)
    def savekey(name,data):
        path=keys/name
        fd=os.open(path,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
        with os.fdopen(fd,'w') as f:f.write(data.hex())
        return path
    bondkeys=[savekey(f'bond-{i}.seed',hashlib.blake2b(b'misaka-devnet-genesis-bond-v1/'+str(i).encode(),digest_size=32).digest()) for i in range(4)]
    mainkey=savekey('public-main.seed',hashlib.blake2b(b'misaka-testnet-premine-9b-claude-managed',digest_size=32).digest())
    payer=savekey('measurement.seed',os.urandom(32))
    def address(key):
        return subprocess.run([str(bins['cli']),'--network','devnet','key','address','--key-file',str(key)],
                              capture_output=True,text=True,check=True,timeout=30).stdout.strip().split()[-1]
    addrs=[address(k) for k in bondkeys];payaddress=address(payer)
    pub=json.loads(subprocess.run([str(bins['cli']),'--network','devnet','--output','json','key','pubkey',
        '--key-file',str(payer)],capture_output=True,text=True,check=True,timeout=30).stdout)
    if pub.get('address')!=payaddress or len(pub.get('address_payload',''))!=128:raise ValueError('payer public identity mismatch')
    payer_spk='000076c440'+pub['address_payload'].lower()+'88a6'
    (work/'launch-plan.json').write_text(json.dumps(plan,indent=2))
    premine=b'misaka-premine'.hex().ljust(128,'0')
    children=[];logs=[]
    try:
        for i in range(4):
            command=[str(bins['kaspad']),'--devnet','--appdir='+str(work/f'node-{i}'),
                f'--listen=127.0.0.1:{a.p2p_base+i}',f'--rpclisten-borsh=127.0.0.1:{a.borsh_base+i}',
                '--utxoindex','--nodnsseed','--disable-upnp','--enable-unsynced-mining',
                '--palw-devnet-floor-only','--palw-execution-lane-devnet=0,2,10','--palw-produce','--palw-panel','--palw-round-lane',
                '--palw-producer-key='+str(bondkeys[i]),f'--palw-producer-bond={premine}:{i}',
                '--palw-producer-pay-address='+addrs[i],f'--palw-fee-outpoint={premine}:{41+i}']
            if i==0:command += [f'--rpclisten=127.0.0.1:{a.grpc_port}',f'--rpclisten-json=127.0.0.1:{a.json_port}']
            else:command += ['--nogrpc',f'--connect=127.0.0.1:{a.p2p_base}']
            logfile=(work/f'node-{i}.log').open('xb');logs.append(logfile)
            children.append(subprocess.Popen(command,stdout=logfile,stderr=subprocess.STDOUT,start_new_session=True))
            (work/f'node-{i}-launch.json').write_text(json.dumps(dict(argv=command,pid=children[-1].pid)))
        spec=importlib.util.spec_from_file_location('wrpc',a.repo/'scripts/drills/misaka_wrpc_json.py')
        mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
        deadline=time.monotonic()+a.startup_timeout;initial=None
        while True:
            if any(c.poll() is not None for c in children):raise RuntimeError('owned node exited during startup')
            if time.monotonic()>deadline:raise TimeoutError('node DAA readiness timeout')
            rpc=None
            try:
                rpc=mod.WsRpc(host='127.0.0.1',port=a.json_port,timeout=5)
                dag=rpc.call('getBlockDagInfo')
                if dag.get('network')!='devnet':raise ValueError('nondevnet RPC')
                if initial is None:initial=dag['virtualDaaScore']
                if dag['virtualDaaScore']>initial:break
            except (OSError,RuntimeError):pass
            finally:
                if rpc:rpc.close()
            time.sleep(1)
        # Recheck isolation before funding, not merely at driver startup.
        isolation=mod.WsRpc(host='127.0.0.1',port=a.json_port,timeout=10)
        peers=isolation.call('getConnectedPeerInfo');isolation.close()
        (work/'funding-peer-check.json').write_text(json.dumps(peers))
        if not isinstance(peers.get('infos'),list):raise ValueError('peer inventory schema missing')
        for peer in peers['infos']:
            host=peer['address'].rsplit(':',1)[0].strip('[]')
            if not ipaddress.ip_address(host).is_loopback:raise ValueError('external peer before funding')
        # Only public devnet premine funds; actual CLI response retained without secret material.
        funding=[str(bins['cli']),'--network','devnet','--rpc',f'127.0.0.1:{a.borsh_base}',
                 'wallet','send','--key-file',str(mainkey),'--to',payaddress,'--amount',str(a.fund_msk),'--yes','--output','json']
        with (work/'funding.stdout').open('xb') as out,(work/'funding.stderr').open('xb') as err:
            result=subprocess.run(funding,stdout=out,stderr=err,timeout=120)
        if result.returncode:raise RuntimeError('public-premine funding failed; see funding logs')
        response=json.loads((work/'funding.stdout').read_text())
        funding_id=response.get('txid')
        if response.get('ok') is not True or response.get('dryRun') is not False or not isinstance(funding_id,str) or len(funding_id)!=128:
            raise ValueError('funding CLI did not return an actual submitted transaction ID')
        # RPC submit success is not a current spendable UTXO. Preserve each actual observation.
        ready_deadline=time.monotonic()+a.funding_timeout
        with (work/'funding-observations.jsonl').open('x') as evidence:
            while True:
                if any(c.poll() is not None for c in children):raise RuntimeError('owned node died while awaiting funding')
                if time.monotonic()>=ready_deadline:raise TimeoutError('funding output did not become observable before deadline')
                rpc=mod.WsRpc(host='127.0.0.1',port=a.json_port,timeout=10)
                try:
                    dag=rpc.call('getBlockDagInfo')
                    coins=rpc.call('getUtxosByAddresses',{'addresses':[payaddress]})
                finally:rpc.close()
                evidence.write(json.dumps(dict(utc=time.time(),dag=dag,utxos=coins))+'\n');evidence.flush()
                if dag.get('network')!='devnet':raise ValueError('network changed while observing funding')
                matched=matching_funded_outputs(coins,funding_id,payer_spk,a.fund_msk*100_000_000,dag['virtualDaaScore'])
                if matched:
                    (work/'observed-funded-root.json').write_text(json.dumps(dict(kind='separate DAG/UTXO observations, not atomic proof',outputs=matched)))
                    break
                time.sleep(1)
        command=['python3',str(Path(__file__).with_name('driver.py')),'--repo',str(a.repo),'--work',str(work),
            '--manifest',str(a.manifest.resolve()),'--sender',str(bins['sender']),'--key',str(payer),
            '--grpc-port',str(a.grpc_port),'--json-port',str(a.json_port),'--tps',str(a.tps),'--seconds',str(a.seconds),
            '--fanout-timeout',str(a.funding_timeout),'--run']
        driver_out=(work/'driver.stdout').open('xb');driver_err=(work/'driver.stderr').open('xb');logs.extend([driver_out,driver_err])
        driver=subprocess.Popen(command,stdout=driver_out,stderr=driver_err,start_new_session=True);children.append(driver)
        while driver.poll() is None:
            if any(child.poll() is not None for child in children[:-1]):
                raise RuntimeError('owned node exited during measurement; stopping the experiment')
            time.sleep(.5)
        (work/'driver-exit.json').write_text(json.dumps(dict(exit_code=driver.returncode,utc=time.time())))
        if driver.returncode!=0:raise RuntimeError('measurement driver failed')
    finally:
        # Driver is last, and receives time to clean up its own sender and compressed observer.
        cleanup_owned([(child,'driver' if index==4 else f'node-{index}',True,(60,60,10))
                       for index,child in reversed(list(enumerate(children)))],work/'launcher-cleanup.json')
        for logfile in logs:logfile.close()
        # Keep appdirs, public keys, measurement seed and every raw log for review; do not delete evidence.
if __name__=='__main__':main()
