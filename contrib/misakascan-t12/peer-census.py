#!/usr/bin/env python3
"""Observed handshake events from the verified running T12 unit, separate from connections."""
import datetime as dt
import ipaddress
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import urlparse

GENESIS = 'a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23'
DEFAULT_LOG = '/root/.t12r-b6/misaka-testnet-12/logs/rusty-kaspa.log'
TS = re.compile(r'^(\d{4}-\d\d-\d\d[ T]\d\d:\d\d:\d\d)(?:\.(\d+))?(Z|[+-]\d\d:\d\d)')
PEER = r'\[?([0-9a-fA-F:.]+?)\]?:(\d+)'
LOCAL = re.compile(r'handshake failed for (inbound|outbound) peer '+PEER+r': (.*)$')
RECEIVED = re.compile(r'got reject message: (.*) from peer: '+PEER+r'\s*$')
FORK = re.compile(r'Fork-id mismatch on network (\S+) at DAA (\d+) - this node has crossed fence (\d+); (.*)')
NEXT = re.compile(r'expects fence (\d+) next, not (\d+)')
MISMATCH = re.compile(r'(Genesis|Consensus params) mismatch on network (\S+) - local: ([0-9a-f]+), remote: ([0-9a-f]+)')

def timestamp(line):
    m = TS.match(line)
    if not m:
        return None
    try:
        return dt.datetime.fromisoformat(m[1].replace(' ', 'T')+m[3].replace('Z', '+00:00')).astimezone(dt.timezone.utc)
    except ValueError:
        return None

def parse_events(lines, now, window=60):
    rows = {}; warnings = 0; stamps = []
    for line in lines:
        stamp = timestamp(line)
        if stamp:
            stamps.append(stamp)
        if not any(s in line for s in ['Fork-id mismatch', 'Genesis mismatch', 'Consensus params mismatch', 'CurrentNextFenceMismatch', 'UnknownFiredSet']):
            continue
        if not stamp:
            warnings += 1; continue
        if stamp < now-dt.timedelta(minutes=window) or stamp > now+dt.timedelta(seconds=60):
            continue
        local, received = LOCAL.search(line), RECEIVED.search(line)
        if local:
            ip, port, why = local[2], int(local[3]), local[4]
            direction = 'local_rejected_peer'
        elif received:
            why, ip, port = received[1], received[2], int(received[3])
            direction = 'peer_rejected_local'
        else:
            warnings += 1; continue
        try:
            ip = str(ipaddress.ip_address(ip))
        except ValueError:
            warnings += 1; continue
        fields = {'reason':'fork_id_mismatch', 'mismatch':'unclassified'}
        fork, mismatch = FORK.search(why), MISMATCH.search(why)
        if fork:
            fields.update(network=fork[1], rejecting_node_daa=int(fork[2]), crossed_fence=int(fork[3]))
        nxt = NEXT.search(why)
        if nxt:
            # "peer" is relative to the node that authored the error, not necessarily this observer.
            fields.update(mismatch='next_fence_mismatch', rejected_peer_next_fence=str(nxt[1]), rejecting_node_expected_next=str(nxt[2]))
        elif 'CurrentNextFenceMismatch' in why:
            fields['mismatch'] = 'next_fence_mismatch'
        elif 'UnknownFiredSet' in why or 'fired-fence set is not' in why:
            fields['mismatch'] = 'unknown_fired_set'
        elif 'sent no fork id' in why:
            fields['mismatch'] = 'missing_fork_id'
        elif 'does not prove it carries' in why:
            fields['mismatch'] = 'rejudge_required'
        elif mismatch:
            fields.update(reason='different_genesis' if mismatch[1]=='Genesis' else 'different_ruleset', network=mismatch[2], local_identity=mismatch[3], remote_identity=mismatch[4])
        key = (ip, port, direction, fields['reason'], fields['mismatch'], fields.get('crossed_fence'), fields.get('rejected_peer_next_fence'), fields.get('rejecting_node_expected_next'), fields.get('local_identity'), fields.get('remote_identity'))
        row = rows.setdefault(key, {'ip':ip,'port':port,'direction':direction,**fields,'attempts':0,'first_seen':stamp.isoformat(),'last_seen':stamp.isoformat(),'detail':why[:700]})
        row['attempts'] += 1
        row['last_seen'] = max(row['last_seen'],stamp.isoformat())
    return sorted(rows.values(),key=lambda r:r['last_seen'],reverse=True), warnings, stamps

def valid_runtime(runtime):
    if not isinstance(runtime,dict):
        return False
    identities=all(isinstance(runtime.get(k),str) and re.fullmatch('[0-9a-f]{64}',runtime[k]) for k in ['params','schedule'])
    fences=runtime.get('fences')
    return (runtime.get('network')=='testnet-12' and runtime.get('genesisHash')==GENESIS
            and isinstance(runtime.get('p2pId'),str) and bool(runtime['p2pId']) and identities
            and isinstance(fences,list) and all(type(x) is int and 0<=x<2**64 for x in fences))

def document(lines, runtime, bound, now, window=60):
    rows, warnings, stamps = parse_events(lines,now,window)
    reason = None
    if not bound:
        reason = 'log path does not match the running T12 unit'
    elif not valid_runtime(runtime):
        reason = 'T12 runtime RPC identity unavailable or mismatched'
    elif not stamps or max(stamps) < now-dt.timedelta(minutes=5) or max(stamps)>now+dt.timedelta(minutes=1):
        reason = 'source log timestamp unavailable or stale'
    elif any(r.get('network') not in (None,'testnet-12','misaka-testnet-12') for r in rows):
        reason = 'log rejection events report another network'
    elif warnings:
        reason = 'unparsed rejection records; count is incomplete'
    runtime=runtime if isinstance(runtime,dict) else {}
    return {'generated_at':now.isoformat(),'window_minutes':window,'available':reason is None,'availability_reason':reason,
            'network':runtime.get('network') if runtime else None,'this_node_daa':runtime.get('daa') if runtime else None,
            'vantage':'T12 unit misaka-t12-node on 169.58.232.113 (log observer, separate from primary RPC)',
            'source':runtime or {},'observed_from':min(stamps).isoformat() if stamps else None,'observed_to':max(stamps).isoformat() if stamps else None,
            'complete_window':bool(stamps and min(stamps)<=now-dt.timedelta(minutes=window)),
            'unparsed_rejections':warnings,'refused':rows if reason is None else [],
            'note':'Events are sessions, not a list of rejected node identities. One IP/NAT may have successful and rejected sessions. No fork gate is relaxed.'}

def unit_binds_log(log, unit, rpc_url):
    try:
        pid = int(subprocess.check_output(['systemctl','show',unit,'-p','MainPID','--value'],text=True,timeout=3).strip())
        if pid<=0:
            return False
        args = Path(f'/proc/{pid}/cmdline').read_bytes().decode().split('\0')
        appdir = next((v.split('=',1)[1] for v in args if v.startswith('--appdir=')),None)
        if '--appdir' in args:
            appdir = args[args.index('--appdir')+1]
        if not appdir or Path(log).resolve()!=(Path(appdir)/'misaka-testnet-12/logs/rusty-kaspa.log').resolve():
            return False
        url=urlparse(rpc_url)
        if url.hostname not in ('127.0.0.1','localhost','::1') or not url.port:
            return False
        sockets=set()
        for fd in Path(f'/proc/{pid}/fd').iterdir():
            try:
                sockets.add(fd.readlink().name)
            except OSError:
                pass
        for table in ['/proc/net/tcp','/proc/net/tcp6']:
            for row in Path(table).read_text().splitlines()[1:]:
                cols=row.split()
                if cols[3]=='0A' and int(cols[1].split(':')[1],16)==url.port and f'socket:[{cols[9]}]' in sockets:
                    return True
        return False
    except (OSError,ValueError,IndexError,subprocess.SubprocessError):
        return False

def main():
    log=Path(os.environ.get('CENSUS_LOG',DEFAULT_LOG));out=Path(os.environ.get('CENSUS_OUT','/var/www/misaka-explorer/peer-census.json'))
    unit=os.environ.get('CENSUS_UNIT','misaka-t12-node');runtime=None;lines=[]
    rpc_url=os.environ.get('CENSUS_RPC','ws://127.0.0.1:26314')
    helper=os.environ.get('CENSUS_RPC_HELPER',str(Path(__file__).with_name('peer-census-rpc.mjs')))
    try:
        runtime=json.loads(subprocess.check_output(['/usr/bin/node','--experimental-websocket',helper,rpc_url],text=True,timeout=12,stderr=subprocess.DEVNULL))
    except (OSError,ValueError,subprocess.SubprocessError):
        pass
    try:
        with log.open('rb') as f:
            f.seek(0,2);size=f.tell();f.seek(max(0,size-(32<<20)));lines=f.read().decode('utf8','replace').splitlines()
            if size>(32<<20):lines=lines[1:]
    except OSError:
        pass
    result=document(lines,runtime,unit_binds_log(log,unit,rpc_url),dt.datetime.now(dt.timezone.utc))
    fd,tmp=tempfile.mkstemp(prefix='.peer-census-',dir=out.parent)
    try:
        with os.fdopen(fd,'w') as f:json.dump(result,f)
        os.chmod(tmp,0o644);os.replace(tmp,out)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)
    print(json.dumps({'available':result['available'],'event_groups':len(result['refused']),'reason':result['availability_reason']}))
if __name__=='__main__':main()
