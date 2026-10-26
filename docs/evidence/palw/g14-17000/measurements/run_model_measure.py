#!/usr/bin/env python3
"""Pinned real Dolphin A16 measurement, with bounded runtime/RSS and partial logs."""
import argparse, hashlib, json, os, re, signal, subprocess, time
from pathlib import Path

def sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for data in iter(lambda: f.read(1 << 20), b''): h.update(data)
    return h.hexdigest()

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--build-record', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--positions', type=int, required=True)
    p.add_argument('--timeout-seconds', type=int, default=43200)
    p.add_argument('--rss-cap-gib', type=float, default=8)
    a = p.parse_args()
    assert 0 <= a.positions <= 32768
    build = json.loads(a.build_record.read_text())
    assert build['stage'] == 'BUILT' and build['source_unchanged'] is True
    binary = build['binary']['path']
    assert sha(binary) == build['binary']['sha256']
    root = Path('/Users/wata/Downloads/genesis-models/work/01-dolphin3-qwen25-1p5b')
    env = dict(os.environ, TMPDIR='/private/tmp', RAYON_NUM_THREADS='3',
               G14_A16_ARTIFACT=str(root/'legacy/dolphin3.0-qwen25-1.5b-a16-32k-precise.palwart'),
               G14_A16_TOKENS=str(root/'calib/eval-a16-long-tokens.json'),
               G14_A16_POSITIONS=str(a.positions), G14_A16_CHUNK='16')
    cmd = ['/usr/bin/time','-l',binary,'g14_dolphin_long_context_cost','--exact','--ignored','--nocapture','--test-threads=1']
    a.out.mkdir(parents=True, exist_ok=False)
    r = {'build':build, 'command':cmd, 'env':{k:v for k,v in env.items() if k.startswith('G14_') or k in ['TMPDIR','RAYON_NUM_THREADS']},
         'status':'RUNNING', 'minimum_hardware':False, 'arming_evidence':False, 'load_before':os.getloadavg(),
         'timeout_seconds':a.timeout_seconds, 'rss_cap_gib':a.rss_cap_gib, 'telemetry':[]}
    def save(): (a.out/'raw.json').write_text(json.dumps(r,indent=2)+'\n')
    save(); start=time.monotonic(); stop=None
    with (a.out/'stdout.log').open('w') as stdout, (a.out/'stderr.log').open('w') as stderr:
        child=subprocess.Popen(cmd,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
        while child.poll() is None:
            # Sum ONLY processes in our newly created group, never other lanes.
            ps=subprocess.check_output(['ps','-axo','pgid=,rss='],text=True)
            rss=sum(int(parts[1])*1024 for line in ps.splitlines() if len(parts:=line.split())==2 and parts[0]==str(child.pid))
            row={'seconds':time.monotonic()-start,'process_group_rss_bytes':rss,'load':os.getloadavg()}
            r['telemetry'].append(row); save()
            if rss>a.rss_cap_gib*(1<<30): stop='RSS_LIMIT'
            elif row['seconds']>a.timeout_seconds: stop='TIME_LIMIT'
            if stop:
                os.killpg(child.pid,signal.SIGTERM)
                try: child.wait(timeout=10)
                except subprocess.TimeoutExpired: os.killpg(child.pid,signal.SIGKILL);child.wait()
                break
            time.sleep(5)
        rc=child.wait()
    r.update(exit_code=rc, wall_seconds=time.monotonic()-start, load_after=os.getloadavg(),
             stdout_sha256=sha(a.out/'stdout.log'), stderr_sha256=sha(a.out/'stderr.log'))
    r['under_load']=any(x['load'][0]>5 for x in r['telemetry']) or max(r['load_before'][0],r['load_after'][0])>5
    lines=(a.out/'stdout.log').read_text().splitlines()
    for key, prefix in [('setup','G14_MODEL_SETUP '),('result','G14_MODEL_RESULT ')]:
        hits=[json.loads(raw) for raw in re.findall(r'^(?:test \S+ \.\.\. )?'+re.escape(prefix)+r'(\{.*\})$', '\n'.join(lines), re.MULTILINE)]
        r[key]=hits[0] if len(hits)==1 else None
    chunks=[json.loads(line[len('G14_MODEL_CHUNK '):]) for line in lines if line.startswith('G14_MODEL_CHUNK ')]
    r['completed_positions']=chunks[-1]['end'] if chunks else 0
    err=(a.out/'stderr.log').read_text()
    for key,pat in [('rss_bytes',r'(\d+)\s+maximum resident set size'),('user_seconds',r'([\d.]+)\s+user'),('system_seconds',r'([\d.]+)\s+sys')]:
        m=re.search(pat,err);r[key]=float(m[1]) if m else None
    okay=rc==0 and r['result'] is not None and r['result']['positions_executed']==a.positions
    r['status']=stop or ('UNDER_LOAD_RUNTIME_ONLY' if okay and r['under_load'] else 'RUNTIME_ONLY' if okay else 'FAILED')
    save();print(json.dumps({'status':r['status'],'positions':r['completed_positions'],'output':str(a.out)}))
    return 0 if okay else 1

if __name__=='__main__':raise SystemExit(main())
