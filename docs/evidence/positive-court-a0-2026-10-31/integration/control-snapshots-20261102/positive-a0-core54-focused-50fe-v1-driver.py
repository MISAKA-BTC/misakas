#!/usr/bin/env python3
"""OWNER-SELECTED A6 MIXED CORE54-FOCUSED 50FE-V1; pure-Python freeze authorized, Cargo/runtime HOLD pending Root GO. No Cargo without --run. Draft cannot execute.
1: --freeze-candidate FULL_SHA --write-plan /tmp/NEW.json (source reads only)
2: --run --plan /tmp/NEW.json --scope small --stem UNIQUE
HF: --run --plan /tmp/NEW.json --scope hf --hf-timeout-seconds 1800 --hf-max-rss-bytes 8589934592 --stem UNIQUE
"""
from pathlib import Path
import argparse,ast,copy,hashlib,json,os,re,shutil,signal,subprocess,sys,time,datetime,threading
from contextlib import contextmanager
ROOT=None # bound only by explicit existing --workspace
DRAFT=Path('/tmp/positive-a0-core54-focused-50fe-v1-draft.json')
DRAFT_SHA='ec281a2ccecf9b257f1b705c1e1d56ce8064764be9e020a2e15ce69d8ed9e1f1'
TARGET=None # bound only by explicit existing --target-dir
def h(p):
 d=hashlib.sha256()
 with Path(p).open('rb') as f:
  while b:=f.read(1048576):d.update(b)
 return d.hexdigest()
def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def head():return subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
def write_new(p,v):
 with Path(p).open('x') as f:f.write(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def snap(m):
 paths={p for p in [ROOT/'Cargo.toml',ROOT/'Cargo.lock',ROOT/'consensus/Cargo.toml',ROOT/'rust-toolchain.toml',ROOT/'rust-toolchain'] if p.is_file()}
 for base in m['source_roots']:
  b=ROOT/base
  if not b.exists():continue
  for p in b.rglob('*'):
   if p.is_file() and p.suffix in {'.rs','.toml','.lock','.json','.py','.sh'} and not any(v in p.relative_to(b).parts for v in ['target','validation','evidence','__pycache__']):paths.add(p)
 return {str(p.relative_to(ROOT)):h(p) for p in sorted(paths)}
def static_inputs(m):
 c=m['native_config'];return h(c['path'])==c['sha256'] and Path(c['path']).stat().st_size==c['bytes'] and all(Path(p).stat().st_size==a['bytes'] and h(p)==a['sha256'] for p,a in m['native_inputs'].items())
def stable(m):return head()==m['execution_source'] and snap(m)==m['source_before'] and static_inputs(m)
def group_commands(m,specs):
 cfg=['--config',m['native_config']['path'] or '<EXPLICIT_PINNED_NATIVE_CONFIG>']
 for profile in ['dev','test']:
  for pkg in ['kaspa-consensus-core','kaspa-consensus']:
   cfg+=['--config',f'profile.{profile}.package.{pkg}.opt-level=0','--config',f'profile.{profile}.package.{pkg}.debug=0']
 commands={}
 for g in ['core','exec','sdk','node']:
  selected=[s for s in specs if s['group']==g]
  if not selected:continue
  pkg={'core':'kaspa-consensus-core','exec':'misaka-palw-tir-exec','sdk':'misaka-palw-sdk','node':'kaspa-consensus'}[g]
  cmd=['cargo','test','-p',pkg,'--offline','--locked','--no-run','--message-format=json-render-diagnostics']+cfg
  if g in ['core','node']:cmd+=['--lib']
  else:
   if g=='exec':cmd+=['--features','positive-panel']
   for k in dict.fromkeys(s['key'] for s in selected):
    cmd+=['--lib'] if g=='exec' and k=='misaka_palw_tir_exec' else ['--test',k]
  commands[g]=cmd
 return commands
class ProcessInterrupted(KeyboardInterrupt):
 def __init__(self, signum, result):
  self.command_result=result
  super().__init__(f'signal {signum}; owned group stopped before raw hash')
class CommandFailure(RuntimeError):
 def __init__(self, reason, result):
  self.command_result=result
  super().__init__(reason)
@contextmanager
def cancellation_signals():
 state={'signal':None};previous={}
 def record(signum,_frame):
  if state['signal'] is None:state['signal']=signum
 if threading.current_thread() is threading.main_thread():
  for name in ['SIGINT','SIGTERM','SIGHUP']:
   if hasattr(signal,name):
    sig=getattr(signal,name);previous[sig]=signal.signal(sig,record)
 try:yield state
 finally:
  for sig,handler in previous.items():signal.signal(sig,handler)
def group_members(group):
 rows=subprocess.check_output(['ps','-A','-o','pid=,pgid=,stat='],text=True)
 result=[]
 for row in rows.splitlines():
  pid,pgid,state=row.split()
  if int(pgid)==group:result.append({'pid':int(pid),'state':state})
 return result
def active_group(group):return [v['pid'] for v in group_members(group) if not v['state'].startswith('Z')]
def terminate(process):
 # Leader exit does not end ownership: descendants may still hold the log FD.
 group=process.pid;sent=[]
 for sig,grace in [(signal.SIGTERM,1.0),(signal.SIGKILL,5.0)]:
  try:os.killpg(group,sig);sent.append(sig.name)
  except ProcessLookupError:pass
  until=time.monotonic()+grace
  while time.monotonic()<until:
   process.poll()
   if not active_group(group):
    process.wait(timeout=1)
    members=group_members(group)
    if any(not v['state'].startswith('Z') for v in members):continue
    return {'group':group,'signals_sent':sent,'direct_child_reaped':process.returncode is not None,'live_members':[],'zombies':[v for v in members if v['state'].startswith('Z')]}
   time.sleep(0.05)
 raise RuntimeError(f'owned group {group} still active; raw hash MUST NOT be finalized')
def pg_rss(pgid):
 # Native ps reports KiB; sampled process-group sum, not a certified high-water RSS.
 r=subprocess.run(['ps','-axo','pgid=,rss='],capture_output=True,text=True,check=False)
 if r.returncode:raise RuntimeError('Cannot observe own process-group RSS')
 total=0
 for line in r.stdout.splitlines():
  parts=line.split()
  if len(parts)==2 and parts[0].isdigit() and int(parts[0])==pgid:total+=int(parts[1])*1024
 return total
def command(path,argv,env,timeout=None,rss=None):
 started=time.monotonic();why=None;peak=0;cleanup=None;failure=None;proc=None
 if shutil.disk_usage(TARGET).free<512*(1<<20):
  with path.open('x') as f:f.write('NOT STARTED: free space below512MiB threshold\n')
  path.chmod(0o444)
  return {'command':argv,'exit_code':None,'executed':False,'stop_reason':'disk_before_start','log':str(path),'sha256':h(path),'elapsed_seconds':0,'sampled_own_pg_rss_max':0,'cleanup':None}
 with cancellation_signals() as cancelled:
  with path.open('x') as f:
   if cancelled['signal'] is not None:
    raise CommandFailure('Cancellation before spawn',{'command':argv,'executed':False,'log':str(path),'sha256':None})
   proc=subprocess.Popen(argv,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
   try:
    while proc.poll() is None:
     if cancelled['signal'] is not None:why='cancelled'
     elif shutil.disk_usage(TARGET).free<512*(1<<20):why='disk_below_512MiB'
     elif timeout and time.monotonic()-started>timeout:why='timeout'
     if rss and not why:
      used=pg_rss(proc.pid);peak=max(peak,used)
      if used>rss:why='sampled_own_pg_rss_limit'
     if why:cleanup=terminate(proc);break
     time.sleep(0.05)
    if cleanup is None:
     if active_group(proc.pid):
      why='descendant_leak';cleanup=terminate(proc)
     else:
      proc.wait(timeout=1);members=group_members(proc.pid)
      cleanup={'group':proc.pid,'signals_sent':[],'direct_child_reaped':True,'live_members':[],'zombies':[v for v in members if v['state'].startswith('Z')]}
    if cancelled['signal'] is not None:why='cancelled'
   except BaseException as e:
    failure=e
    try:cleanup=terminate(proc)
    except BaseException as stop_error:
     raise CommandFailure('Group cleanup unconfirmed; raw hash prohibited',{'command':argv,'executed':True,'log':str(path),'sha256':None,'cleanup_error':repr(stop_error),'initial_error':repr(e)}) from stop_error
   if why:f.write('\nOWNER STOP '+why+': owned group has no live members before raw finalization\n')
  # No command metadata/hash is final while a descendant can still write.
  members=group_members(proc.pid)
  if any(not v['state'].startswith('Z') for v in members):
   raise CommandFailure('Live group after cleanup; raw hash prohibited',{'command':argv,'executed':True,'log':str(path),'sha256':None,'members':members})
  path.chmod(0o444)
  result={'command':argv,'exit_code':proc.returncode,'executed':True,'stop_reason':why,'log':str(path),'sha256':h(path),'elapsed_seconds':time.monotonic()-started,'sampled_own_pg_rss_max':peak,'cleanup':cleanup,'no_live_before_raw_hash':True,'cancelled_signal':cancelled['signal']}
  if cancelled['signal'] is not None:raise ProcessInterrupted(cancelled['signal'],result)
  if failure is not None:raise CommandFailure(repr(failure),result) from failure
  return result
def listed(raw):return re.findall(r'^(\S+): test$',raw,re.M)
def summaries(raw):return [{'status':x[0],'passed':int(x[1]),'failed':int(x[2]),'ignored':int(x[3])} for x in re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',raw,re.M)]
def hf_json(raw,slug):
 stages=[];results=[]
 for line in raw.splitlines():
  for tag,out in [('A1_HF3_STAGE ',stages),('A1_HF3_RESULT ',results)]:
   at=line.find(tag)
   if at>=0:
    try:out.append(json.loads(line[at+len(tag):]))
    except ValueError:pass
 return {'model':slug,'stages':stages,'results':results,'stage_record_complete':len(results)==1 and results[0].get('model')==slug and results[0].get('schema')=='misaka.a1.hf3.native-registration-result.v1'}
def validate_fixed_environment(plan,draft,target):
 expected=dict(draft['fixed_environment']);expected['CARGO_TARGET_DIR']=str(target)
 if plan.get('fixed_environment')!=expected:raise RuntimeError('Frozen environment differs from immutable draft/selected target; no flags or stack substitution allowed')
 return expected
def scrub_inherited_environment(inherited):
 env=dict(inherited);removed={}
 for k in list(env):
  if k in ['RUST_MIN_STACK','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_TARGET','CXX','HOST_CXX','TARGET_CXX'] or k.startswith('CARGO_PROFILE_') or k.endswith('_RUSTFLAGS') or k.startswith('CXX_'):
   removed[k]=env.pop(k)
 return env,removed
def main():
 global ROOT,TARGET
 ap=argparse.ArgumentParser(description=__doc__)
 ap.add_argument('--workspace',type=Path);ap.add_argument('--target-dir',type=Path);ap.add_argument('--native-config',type=Path);ap.add_argument('--profile-policy',choices=['a6-mixed'])
 ap.add_argument('--freeze-candidate');ap.add_argument('--write-plan',type=Path);ap.add_argument('--plan',type=Path)
 ap.add_argument('--root-go',action='store_true',help='Use only after explicit Root GO; source freeze alone does not authorize build/runtime');ap.add_argument('--run',action='store_true');ap.add_argument('--scope',choices=['small','os-main','hf','all'],default='small')
 ap.add_argument('--only',nargs='+',choices=['core','exec','sdk','node'])
 ap.add_argument('--stem',default='positive-next-validation')
 ap.add_argument('--min-free-start-bytes',type=int,help='Optional explicit whole-batch start threshold, e.g.4294967296; per-command512MiB own-PG stop always applies')
 ap.add_argument('--hf-timeout-seconds',type=int);ap.add_argument('--hf-max-rss-bytes',type=int)
 args=ap.parse_args()
 if h(DRAFT)!=DRAFT_SHA:raise RuntimeError('Draft plan changed')
 d=json.loads(DRAFT.read_text())
 if args.run or args.freeze_candidate:
  if not all([args.workspace,args.target_dir,args.native_config,args.profile_policy]):ap.error('Explicit --workspace EXISTING --target-dir EXISTING --native-config PINNED --profile-policy a6-mixed required')
  if not args.workspace.is_dir() or not args.target_dir.is_dir() or not args.native_config.is_file():raise RuntimeError('Existing checkout/target/config required; no new directory or target creation')
  ROOT=args.workspace.resolve();TARGET=args.target_dir.resolve()
  top=Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],cwd=ROOT,text=True).strip()).resolve()
  if top!=ROOT:raise RuntimeError('Workspace must name the exact existing git checkout')
  if h(args.native_config)!=d['native_config']['sha256']:raise RuntimeError('Native config is not the pinned A6 metadata-preserving config')
  supplied={'workspace':str(ROOT),'target_dir':str(TARGET),'native_config':str(args.native_config.resolve()),'profile_policy':args.profile_policy}
  if supplied!=d['authorized_owner_parameters']:raise RuntimeError('Explicit parameters differ from Root owner-selected existing workspace/target/config/profile')
  if head()!=d['authorized_candidate'] or subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=ROOT,text=True).strip()!=d['authorized_tree']:raise RuntimeError('Owner-authorized source/tree changed; no self-repin')
  if subprocess.check_output(['git','branch','--show-current'],cwd=ROOT,text=True).strip()!=d['authorized_branch']:raise RuntimeError('Owner-selected existing branch changed')
  if d.get('pending_scope_extensions') or d.get('owner_candidate_selection_pending'):raise RuntimeError('Preparation-only v3: clean A6 actualcandidate and final Root owner parameters pending; no execution or workspace freeze yet')
 if args.freeze_candidate:
  if args.run or not args.write_plan:ap.error('Freeze is a separate source-only step requiring --write-plan; no simultaneous --run')
  if not re.fullmatch(r'[0-9a-f]{40}',args.freeze_candidate) or args.freeze_candidate!=d['authorized_candidate'] or head()!=args.freeze_candidate:raise RuntimeError('Explicit full candidate SHA differs from actual HEAD')
  d['native_config']['path']=str(args.native_config.resolve())
  if not static_inputs(d):raise RuntimeError('Native config/archive/bindings changed')
  if any(h(ROOT/p)!=v for p,v in d['test_source_hashes'].items()):raise RuntimeError('Selected test sources changed; review/refresh selectors explicitly')
  m=copy.deepcopy(d);m['status']='EXPLICITLY_FROZEN_NOT_RUN';m['execution_source']=args.freeze_candidate;m['workspace']=str(ROOT);m['root']=str(ROOT);m['target_dir']=str(TARGET);m['runtime_parameters']={'workspace':str(ROOT),'target_dir':str(TARGET),'native_config':str(args.native_config.resolve()),'profile_policy':args.profile_policy};m['fixed_environment']['CARGO_TARGET_DIR']=str(TARGET);m['source_before']=snap(m)
  tracked=set(subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0'))
  changed=set(subprocess.check_output(['git','diff','--name-only',args.freeze_candidate,'--'],cwd=ROOT).decode().splitlines())
  if set(m['source_before'])-tracked or set(m['source_before']) & changed:raise RuntimeError('Code/fixture snapshot must exactly match tracked candidate; edited handoff docs outside pin may remain')
  m['prepared_delta_paths']=subprocess.check_output(['git','diff','--name-only',m['prepared_source'],args.freeze_candidate,'--'],cwd=ROOT,text=True).splitlines();m['runner_sha256']=h(__file__);m['draft_sha256']=DRAFT_SHA;m['frozen_at_utc']=utc();write_new(args.write_plan,m);print(json.dumps({'status':m['status'],'plan':str(args.write_plan),'sha256':h(args.write_plan),'source':m['execution_source'],'delta_paths':m['prepared_delta_paths']}));return 0
 m=json.loads(args.plan.read_text()) if args.plan else d
 specs=[s for s in m['specs'] if (args.scope=='all' or (args.scope=='hf' and s['hf']) or (args.scope=='os-main' and s.get('os_main')) or (args.scope=='small' and not s['hf'] and not s.get('os_main'))) and (not args.only or s['group'] in args.only)]
 if not specs:ap.error('No selected scopes')
 cmds=group_commands(m,specs)
 if not args.run:
  print(json.dumps({'status':'PREPARED_NOT_EXECUTED','plan_status':m['status'],'execution_source':m['execution_source'],'commands':cmds,'scopes':specs,'selected_small_source_names':sum(len(s['names']) for s in specs if not s['hf'] and not s.get('os_main')),'selected_os_main_cases':sum(bool(s.get('os_main')) for s in specs),'runtime_counts':None,'hf_cases':sum(s['hf'] for s in specs),'estimate':m['estimate']},ensure_ascii=False,indent=2));return 0
 if not args.root_go:raise RuntimeError('Root GO required before any Cargo/runtime; frozen plan authorizes source identity only')
 if not args.plan or m['status']!='EXPLICITLY_FROZEN_NOT_RUN' or m.get('runner_sha256')!=h(__file__) or m.get('draft_sha256')!=DRAFT_SHA:raise RuntimeError('Explicit refreshed frozen plan required before execution')
 if m['specs']!=d['specs'] or m['native_inputs']!=d['native_inputs'] or m['native_config']['sha256']!=d['native_config']['sha256']:raise RuntimeError('Frozen selector/native contract differs from reviewed draft')
 validate_fixed_environment(m,d,TARGET)
 if m['runtime_parameters']!={'workspace':str(ROOT),'target_dir':str(TARGET),'native_config':str(args.native_config.resolve()),'profile_policy':args.profile_policy}:raise RuntimeError('Explicit runtime parameters differ from final owner freeze')
 if sys.platform!='darwin' or not TARGET.is_dir():raise RuntimeError('Use explicitly selected existing Darwin checkout/target only')
 if not stable(m):raise RuntimeError('Source/HEAD/native input changed after explicit freeze')
 if args.min_free_start_bytes is not None and (args.min_free_start_bytes<512*(1<<20) or shutil.disk_usage(TARGET).free<args.min_free_start_bytes):raise RuntimeError('Explicit min-free-start threshold invalid or unmet; no work started')
 if any(s['hf'] for s in specs) and (not args.hf_timeout_seconds or args.hf_timeout_seconds<=0 or not args.hf_max_rss_bytes or args.hf_max_rss_bytes<=0):ap.error('HF process timeout and sampled RSS ceiling must be supplied explicitly')
 if not re.fullmatch(r'[a-zA-Z0-9_-]+',args.stem):ap.error('Simple unique evidence filename stem required')
 out=ROOT/'docs/evidence/positive-court-a0-2026-10-31/integration';manifest=out/(args.stem+'.json')
 if manifest.exists():raise RuntimeError('Evidence exists; no overwrite/retry')
 env,removed=scrub_inherited_environment(os.environ)
 env.update(m['fixed_environment']);guard=out/(args.stem+'-forbid-cxx.py')
 with guard.open('x') as f:f.write('#!/usr/bin/env python3\nimport sys\nsys.stderr.write("C++ compile/probe forbidden; use pinned native links config\\n")\nsys.exit(95)\n')
 guard.chmod(0o555);env['CXX']=str(guard)
 rec={'status':'RUNNING','started_utc':utc(),'source':m['execution_source'],'plan':{'path':str(args.plan),'sha256':h(args.plan)},'runner':{'path':str(Path(__file__).resolve()),'sha256':h(__file__)},'native_config':m['native_config'],'native_inputs':m['native_inputs'],'source_before':m['source_before'],'environment_overrides':m['fixed_environment']|{'CXX':str(guard)},'removed_inherited_environment':removed,'RUST_MIN_STACK_effective':None,'RUSTFLAGS_effective':None,'optional_explicit_min_free_start_bytes':args.min_free_start_bytes,'disk_stop_bytes':512*(1<<20),'guard_sha256':h(guard),'selected_scopes':specs,'builds':[],'runs':[],'scope':m['scope'],'hf_failure_policy':'Each HF exact ignored case collected independently even if another model refuses; no success on zero/missing summary/stageJSON/missing inputs. Fundamental source/native/resource loss is never permission to continue unsafe work.'}
 write_new(manifest,rec)
 def save():manifest.write_text(json.dumps(rec,ensure_ascii=False,indent=2)+'\n')
 allok=True
 try:
  for group,cmd in cmds.items():
   if not stable(m):raise RuntimeError('Input drift before build')
   b=command(out/(args.stem+'-'+group+'-build.jsonl'),cmd,env);arts={};finish=False;source_profiles=[]
   for line in Path(b['log']).read_text(errors='replace').splitlines():
    try:v=json.loads(line)
    except ValueError:continue
    if v.get('reason')=='build-finished':finish=v.get('success') is True
    if v.get('reason')=='build-script-executed' and 'kaspa-consensus-core' in str(v.get('package_id','')):
     source_profiles.append({'cfgs':v.get('cfgs',[]),'env':dict(v.get('env',[])),'out_dir':v.get('out_dir')})
    if v.get('reason')=='compiler-artifact' and v.get('executable') and (v.get('profile',{}).get('test') or ('test' in v.get('target',{}).get('kind',[]) and any(q.get('os_main') and q['key']==v.get('target',{}).get('name') for q in specs))):
     p=Path(v['executable']).resolve()
     if not p.is_relative_to(TARGET.resolve()):raise RuntimeError('New target/test executable path forbidden')
     arts[v['target']['name']]={'path':str(p),'bytes':p.stat().st_size,'sha256':h(p),'profile':v['profile'],'target_kind':v['target'].get('kind',[]),'fresh':v.get('fresh')}
   required_profile=any(s.get('requires_supported_source_profile') for s in specs if s['group']==group)
   profile_contract=m.get('required_supported_source_profile',{})
   profile_ok=bool(source_profiles) and all(profile_contract.get('cfg') in p['cfgs'] and p['env'].get(profile_contract.get('environment_name'))==profile_contract.get('environment_value') for p in source_profiles)
   b['supported_source_profile_required']=required_profile;b['observed_core_build_profiles']=source_profiles;b['supported_source_profile_verified']=profile_ok
   wanted={s['key'] for s in specs if s['group']==group};b['artifacts']={k:v for k,v in arts.items() if k in wanted};b['inputs_stable']=stable(m);b['mixed_artifact_profiles_ok']=all(a['profile']['debuginfo']==(0 if group in ['core','node'] else 2) for a in b['artifacts'].values());b['success']=b['exit_code']==0 and b['stop_reason'] is None and finish and wanted<=arts.keys() and b['inputs_stable'] and b['mixed_artifact_profiles_ok'] and (not required_profile or profile_ok);rec['builds'].append(b);save()
   if not b['success']:raise RuntimeError('Build failed/incomplete; no execution or automatic workaround')
   for s in (v for v in specs if v['group']==group):
    a=arts[s['key']]
    if s.get('os_main'):
     if not stable(m) or h(a['path'])!=a['sha256']:raise RuntimeError('Input drift before OS-main standalone')
     t=command(out/(args.stem+'-'+s['id']+'.log'),[a['path']],env,timeout=1800)
     raw=Path(t['log']).read_text(errors='replace')
     marker=r'^A3_LOADED_PROCESS_V2_PASS profile=audited-source-v1 actual_file=1 loaded=1 cpu_i64_endpoints=1 cycles=2 client_steps=2 server_steps=2 params_read=26 child_guard_released=1 reaped=1 stack=os-main elapsed_ms=([0-9]+)$'
     found=re.findall(marker,raw,re.M);unsupported=[v for v in m['required_supported_source_profile']['unsupported_markers'] if v in raw]
     t.update({'scope':s,'artifact':a,'custom_harness':'OS-main, no arguments, no libtest summary','elapsed_ms_marker':[int(x) for x in found],'unique_exact_full_marker':len(found)==1 and raw.count('A3_LOADED_PROCESS_V2_PASS')==1,'unsupported_markers':unsupported,'inputs_stable':stable(m),'binary_stable':h(a['path'])==a['sha256']})
     t['success']=t['exit_code']==0 and t['stop_reason'] is None and t['unique_exact_full_marker'] and not unsupported and t['inputs_stable'] and t['binary_stable']
     rec['runs'].append(t);allok &= t['success'];save()
     if not t['success']:raise RuntimeError('OS-main standalone failure/refusal/incomplete marker; no libtest success claim')
     continue
    base=[a['path']]+([s['filter']] if s['filter'] else [])+(['--exact'] if s['exact'] else [])+(['--ignored'] if s['hf'] else [])
    if not stable(m) or h(a['path'])!=a['sha256']:raise RuntimeError('Source/native/binary drift before selection')
    identity=command(out/(args.stem+'-'+s['id']+'-list.log'),base+['--list'],env,timeout=60)
    if identity.get('stop_reason'):raise RuntimeError('Selection command stopped; no missing-selector workaround')
    actual=listed(Path(identity['log']).read_text(errors='replace'));expected=s['names']+s['ignored'];identity['exact_identity']=identity['exit_code']==0 and identity['stop_reason'] is None and sorted(actual)==sorted(expected) and bool(actual);identity['listed_test_names']=actual
    if not identity['exact_identity']:
     if not s['hf']:raise RuntimeError('Missing/changed/zero test selector; no runtime')
     stagepath=out/(args.stem+'-'+s['id']+'-stages.json')
     write_new(stagepath,{'model':s['model'],'stages':[],'results':[],'stage_record_complete':False,'error':'exact ignored selector did not identify one expected test; no zero-test runtime success'})
     stagepath.chmod(0o444)
     rec['runs'].append({'scope':s,'identity_list':identity,'executed':False,'success':False,'exact_one_executed_summary':False,'stage_json':{'path':str(stagepath),'sha256':h(stagepath)},'error':'missing_changed_or_zero_selector'})
     allok=False;save();continue
    missing=[]
    if s['hf']:
     pin=next(v for v in m['hf_pins']['models'] if v['slug']==s['model'])
     missing=[pin[k]['path'] for k in ['artifact','fixture','inventory'] if not Path(pin[k]['path']).is_file()]
    run=base+(['--nocapture'] if s['hf'] or s.get('nocapture') else [])+['--test-threads=1']
    t=command(out/(args.stem+'-'+s['id']+'.log'),run,env,timeout=args.hf_timeout_seconds if s['hf'] else 1800,rss=args.hf_max_rss_bytes if s['hf'] else None)
    raw=Path(t['log']).read_text(errors='replace');summary=summaries(raw);n=len(s['names']);ig=len(s['ignored']);outer=summary[-1:] if s.get('nocapture') else summary;counts=outer==[{'status':'ok','passed':n,'failed':0,'ignored':ig}];unsupported=[marker for marker in m.get('required_supported_source_profile',{}).get('unsupported_markers',[]) if marker in raw]
    # With --nocapture the test-name line can interleave with stderr. The separate
    # exact --list proves identity; only the one-test summary determines cardinality.
    named=all(re.search(r'^test '+re.escape(v)+r' \.\.\. ok$',raw,re.M) for v in s['names']) if not s['hf'] and not s.get('nocapture') else True
    t.update({'scope':s,'identity_list':identity,'artifact':a,'summaries':summary,'outer_summaries':outer,'nested_summaries_not_counted':summary[:-1] if s.get('nocapture') else [],'unsupported_early_return_markers':unsupported,'audited_decode_success_not_implied_by_raw_test_count':s['id']=='source_facade_8','nonzero_exact_success_counts':n>0 and counts,'test_names_match':bool(named),'missing_inputs_before_case':missing,'inputs_stable':stable(m),'binary_stable':h(a['path'])==a['sha256']})
    if s['hf']:
     stages=hf_json(raw,s['model']);stagepath=out/(args.stem+'-'+s['id']+'-stages.json');write_new(stagepath,stages);stagepath.chmod(0o444);t['stage_json']={'path':str(stagepath),'sha256':h(stagepath)};t['structured_native_result_success']=stages['stage_record_complete'] and stages['results'][0].get('result')=='native_registration_checked';t['exact_one_executed_summary']=len(summary)==1 and summary[0]['passed']+summary[0]['failed']==1 and summary[0]['ignored']==0
    t['success']=t['exit_code']==0 and t['stop_reason'] is None and t['nonzero_exact_success_counts'] and t['test_names_match'] and t['inputs_stable'] and t['binary_stable'] and not missing and not unsupported and (not s['hf'] or t['structured_native_result_success'])
    rec['runs'].append(t);allok &= t['success'];save();print(json.dumps({'scope':s['id'],'summary':summary,'success':t['success']}),flush=True)
    if t.get('stop_reason') in ['disk_before_start','disk_below_512MiB','descendant_leak']:raise RuntimeError('Fatal own-group/resource stop; no further HF cases')
    if not t['inputs_stable'] or not t['binary_stable']:raise RuntimeError('Input/binary changed; stop all cases')
    if not t['success'] and not s['hf']:raise RuntimeError('Small-scope failure; stop; no automatic retry')
  rec['status']='PASS' if allok else 'COMPLETE_WITH_HF_REFUSALS';rec['finished_utc']=utc();rec['small_passed']=sum(t['outer_summaries'][0]['passed'] for t in rec['runs'] if t['success'] and not t['scope']['hf'] and not t['scope'].get('os_main'));rec['os_main_passed']=sum(bool(t['scope'].get('os_main')) and t['success'] for t in rec['runs']);rec['hf_cases_collected']=sum(t['scope']['hf'] for t in rec['runs']);rec['hf_passed']=sum(t['scope']['hf'] and t['success'] for t in rec['runs']);save();return 0 if allok else 1
 except BaseException as e:
  rec['status']='INCOMPLETE';rec['stopped_reason']=repr(e);rec['interrupted_or_failed_command']=getattr(e,'command_result',None);rec['finished_utc']=utc();save();print(repr(e),file=sys.stderr);return 1
if __name__=='__main__':sys.exit(main())
