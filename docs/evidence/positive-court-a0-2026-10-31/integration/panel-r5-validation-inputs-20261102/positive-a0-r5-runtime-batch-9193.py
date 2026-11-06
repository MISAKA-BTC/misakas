#!/usr/bin/env python3
"""Prepared only. --run is mandatory; no Cargo/build is performed by default.
Examples: --run --skip-leaf --only exec; --run --skip-leaf --only core artifact.
No automatic retry or linker/config workaround. Each raw result is exclusive-created.
"""
from pathlib import Path
import argparse,hashlib,json,os,re,subprocess,sys,time
ROOT=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
PLAN=Path('/tmp/positive-a0-r5-runtime-batch-9193-plan.json')
PLAN_SHA='2fce859fe391338a269c0492eac18ccef30138ad0e170d5ddd2be291b1fbe545'
TARGET=Path('/Users/wata/.codex/worktrees/positive-court-a0/target')
ARCHIVE='/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out'
def h(p):
 d=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1048576):d.update(b)
 return d.hexdigest()
def snap(plan):
 paths={p for p in [ROOT/'Cargo.toml',ROOT/'Cargo.lock'] if p.exists()}
 for base in plan['source_roots']:
  b=ROOT/base
  if not b.exists():continue
  for p in b.rglob('*'):
   if p.is_file() and p.suffix in {'.rs','.toml','.lock','.json','.py','.sh'} and not any(x in p.relative_to(b).parts for x in ['target','validation','evidence','__pycache__']):paths.add(p)
 return {str(p.relative_to(ROOT)):h(p) for p in sorted(paths)}
def load_plan():
 if h(PLAN)!=PLAN_SHA:raise RuntimeError('Frozen plan hash changed')
 return json.loads(PLAN.read_text())
def same_inputs(plan):
 current=snap(plan)
 return current==plan['source_before'] and all(Path(p).stat().st_size==a['bytes'] and h(p)==a['sha256'] for p,a in plan['archives'].items())
def raw_exclusive(path,cmd,env):
 started=time.monotonic()
 with path.open('x') as f:
  result=subprocess.run(cmd,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT)
 return {'command':cmd,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started,'log':str(path),'sha256':h(path)}
def main():
 ap=argparse.ArgumentParser(description=__doc__)
 ap.add_argument('--run',action='store_true');ap.add_argument('--skip-leaf',action='store_true')
 ap.add_argument('--only',nargs='+',choices=['leaf','exec','core','artifact'])
 ap.add_argument('--with-capture-codec',action='store_true')
 ap.add_argument('--stem',default='panel-r5-runtime-9193-20261102')
 args=ap.parse_args()
 if not re.fullmatch(r'[a-zA-Z0-9_-]+',args.stem):ap.error('stem must be a simple evidence filename stem')
 plan=load_plan();groups=args.only or ['leaf','exec','core','artifact']
 if args.skip_leaf:groups=[g for g in groups if g!='leaf']
 specs=[s for s in plan['specs'] if s['group'] in groups and (not s['optional'] or args.with_capture_codec)]
 if not specs:ap.error('No scopes selected')
 configs=['--config','profile.dev.package.kaspa-consensus-core.opt-level=0','--config','profile.test.package.kaspa-consensus-core.opt-level=0']
 commands={}
 for g in groups:
  selected=[s for s in specs if s['group']==g]
  if not selected:continue
  pkg={'leaf':'misaka-palw-tir','exec':'misaka-palw-tir-exec','core':'kaspa-consensus-core','artifact':'misaka-palw-tir-artifact'}[g]
  cmd=['cargo','test','-p',pkg,'--offline','--locked','--no-run','--message-format=json-render-diagnostics']+configs
  if g=='exec':
   cmd+=['--features','positive-panel','--lib']
   for s in selected:
    if s['key']!='misaka_palw_tir_exec':cmd+=['--test',s['key']]
  elif g=='leaf':cmd+=['--test','node_cost_noalloc']
  else:cmd+=['--lib']
  commands[g]=cmd
 if not args.run:
  print(json.dumps({'status':'PLAN_ONLY_NOT_EXECUTED','head':plan['head'],'groups':groups,'commands':commands,'expected_pass':sum(len(s['names']) for s in specs),'expected_ignored':sum(len(s['ignored']) for s in specs),'scopes':specs},ensure_ascii=False,indent=2));return 0
 if sys.platform!='darwin':raise RuntimeError('Pinned Darwin source/process/native archive candidate requires macOS')
 if not TARGET.is_dir():raise RuntimeError('Existing target absent; new target forbidden')
 head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 if head!=plan['head']:raise RuntimeError('HEAD differs from frozen 9193 plan; refresh plan explicitly, never silently re-pin')
 if not same_inputs(plan):raise RuntimeError('Source/native archive drift from frozen plan')
 out=ROOT/'docs/evidence/positive-court-a0-2026-10-31/integration';manifest=out/(args.stem+'.json')
 if manifest.exists():raise RuntimeError('Refusing to overwrite prior manifest/evidence')
 env=os.environ.copy();removed={}
 for k in list(env):
  if k in ['RUST_MIN_STACK','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_TARGET'] or k.endswith('_RUSTFLAGS'):
   removed[k]=env.pop(k)
 overrides={'CARGO_TARGET_DIR':str(TARGET),'CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0','CARGO_PROFILE_DEV_DEBUG':'0','ROCKSDB_LIB_DIR':ARCHIVE,'ROCKSDB_STATIC':'1','SNAPPY_LIB_DIR':ARCHIVE,'SNAPPY_STATIC':'1','RAYON_NUM_THREADS':'2'}
 env.update(overrides)
 record={'status':'RUNNING','head':head,'runner':{'path':str(Path(__file__).resolve()),'sha256':h(__file__)},'plan':{'path':str(PLAN),'sha256':PLAN_SHA},'environment_overrides':overrides,'removed_inherited_environment':removed,'RUST_MIN_STACK_effective':None,'RUSTFLAGS_effective':None,'source_before':plan['source_before'],'archives':plan['archives'],'scopes':specs,'builds':[],'runs':[],'scope':'Fixed source local runtime regression only; not native registration/role/funding/P/credit, real HF or RSS qualification. Prior leaf evidence is separate, never counted again when skipped.','skipped_leaf_prior_record':plan['leaf_prior_evidence'] if args.skip_leaf else None}
 def save():
  # Manifest is new for this run; prior raw output is never rewritten.
  manifest.write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n')
 with manifest.open('x') as f:f.write(json.dumps(record,ensure_ascii=False,indent=2)+'\n')
 try:
  for g,cmd in commands.items():
   if not same_inputs(plan):raise RuntimeError('Inputs changed before build')
   b=raw_exclusive(out/(args.stem+'-'+g+'-build.jsonl'),cmd,env)
   artifacts={};finished=False
   for line in Path(b['log']).read_text(errors='replace').splitlines():
    try:x=json.loads(line)
    except ValueError:continue
    if x.get('reason')=='build-finished':finished=x.get('success') is True
    if x.get('reason')=='compiler-artifact' and x.get('profile',{}).get('test') and x.get('executable'):
     e=Path(x['executable']).resolve()
     if not e.is_relative_to(TARGET.resolve()):raise RuntimeError('Test executable escaped existing target')
     artifacts[x['target']['name']]={'path':str(e),'bytes':e.stat().st_size,'sha256':h(e),'fresh':x.get('fresh'),'cargo_profile':x['profile']}
   needed={s['key'] for s in specs if s['group']==g};b['artifacts']={k:v for k,v in artifacts.items() if k in needed}
   b['source_and_archives_stable']=same_inputs(plan)
   b['success']=b['exit_code']==0 and finished and needed<=artifacts.keys() and b['source_and_archives_stable']
   record['builds'].append(b);save();print(json.dumps({'group':g,'build':b['success'],'exit':b['exit_code']}),flush=True)
   if not b['success']:raise RuntimeError('Build/artifact/source failure; raw retained; no retries/config changes')
   for idx,s in enumerate(x for x in specs if x['group']==g):
    art=artifacts[s['key']]
    if not same_inputs(plan) or h(art['path'])!=art['sha256']:raise RuntimeError('Source/archive/binary changed before runtime')
    run=[art['path']]
    if s['filter']:run.append(s['filter'])
    if s['exact']:run.append('--exact')
    run+=['--test-threads=1']
    t=raw_exclusive(out/(args.stem+'-'+g+'-'+str(idx)+'-'+s['key']+'.log'),run,env)
    content=Path(t['log']).read_text(errors='replace')
    found=re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',content,re.M)
    got=[{'status':v[0],'passed':int(v[1]),'failed':int(v[2]),'ignored':int(v[3])} for v in found]
    expected=len(s['names']);ignored=len(s['ignored'])
    exact_summary=len(got)==1 and got[0]=={'status':'ok','passed':expected,'failed':0,'ignored':ignored}
    named_ok=all(re.search(r'^test '+re.escape(n)+r' \.\.\. ok$',content,re.M) for n in s['names'])
    t.update({'scope':s,'artifact':art,'test_summaries':got,'nonzero':expected>0 and bool(got) and got[0]['passed']>0,'exact_counts_and_names':bool(exact_summary and named_ok),'inputs_stable':same_inputs(plan),'binary_stable':h(art['path'])==art['sha256']})
    t['success']=t['exit_code']==0 and t['nonzero'] and t['exact_counts_and_names'] and t['inputs_stable'] and t['binary_stable']
    record['runs'].append(t);save();print(json.dumps({'scope':s['key'],'filter':s['filter'],'passed':got,'success':t['success']}),flush=True)
    if not t['success']:raise RuntimeError('Runtime failure/zero/count drift; raw retained; no retry')
  record['status']='PASS';record['passed']=sum(len(s['names']) for s in specs);record['ignored']=sum(len(s['ignored']) for s in specs);save();return 0
 except BaseException as e:
  record['status']='INCOMPLETE';record['stopped_reason']=repr(e);record['source_stable_at_stop']=same_inputs(plan);save();print(repr(e),file=sys.stderr);return 1
if __name__=='__main__':sys.exit(main())
