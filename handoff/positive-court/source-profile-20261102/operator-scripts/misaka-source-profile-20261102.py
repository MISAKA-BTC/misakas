from pathlib import Path
import hashlib,json,runpy,subprocess
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/source-profile-20261102';out.mkdir(exist_ok=False)
s=(root/'handoff/positive-court/optimized-runtime-20261102/operator-scripts/misaka-optimized-controller-20261102.py').read_text().replace("out=root/'handoff/positive-court/optimized-runtime-20261102'","out=root/'handoff/positive-court/source-profile-20261102'")
s=s.replace("start=time.monotonic();stop=None;peak=0","start=time.monotonic();stop=None;peak=0;profiler=None;profile_start=None;profile_console=None;profile_stop=None")
s=s.replace("minimum=min(minimum,shutil.disk_usage(root).free)","""minimum=min(minimum,shutil.disk_usage(root).free)
    if profiler is None and '"checked_relations":1024' in log.read_text():
     profile_console=(out/'sample-console.log').open('xb')
     profiler=subprocess.Popen(['/usr/bin/sample',str(child.pid),'10','1','-mayDie','-file',str(out/'cpu-sample.txt')],stdout=profile_console,stderr=subprocess.STDOUT,start_new_session=True)
     profile_start=time.monotonic()
     print('SAMPLER_LIVE',profiler.pid,'TARGET',child.pid,flush=True)
    if profiler and profiler.poll() is None and time.monotonic()-profile_start>30:
     profile_stop='sampler wall limit'
     os.killpg(profiler.pid,signal.SIGKILL);profiler.wait()
""")
s=s.replace("text=log.read_text();counts=", """if profiler and profiler.poll() is None:
  profile_stop='target terminal before sampler'
  os.killpg(profiler.pid,signal.SIGKILL);profiler.wait()
 if profile_console:profile_console.close()
 profile_record={'sampler_pid':profiler.pid if profiler else None,'exit_code':profiler.returncode if profiler else None,'stop_reason':profile_stop,'target_pid':child.pid,'duration_seconds':10,'interval_milliseconds':1,'max_sampler_wall_seconds':30,'separate_profiler_process':True,'scope':'diagnostic sampling perturbs observed timing; this is not production/cold-role cost qualification'}
 (out/'sampler.json').write_text(json.dumps(profile_record,indent=2)+'\\n')
 text=log.read_text();counts=""")
p=Path('/tmp/misaka-source-profile-controller-20261102.py');p.write_text(s);ns=runpy.run_path(str(p));sha=ns['sha']
previous=root/'handoff/positive-court/optimized-runtime-20261102';r=json.loads((previous/'hashed-walk-runtime-first.json').read_text());binary=r['binary'];assert sha(Path(binary['path']))==binary['sha256'] and ns['snapshot']()==r['source_after']
inputs=json.loads((previous/'input-sha-hashed-walk.json').read_text())['after']
for f,h in inputs.items():assert sha(Path(f))==h
case=ns['run']('pythia-profile-first',[binary['path'],'--exact','trained_roles::pythia70m_original_short_job_ordinary_panel_and_complete_node','--ignored','--nocapture','--test-threads=1'],360,512*1024**2+305797608+16*1024**2,4*1024**3,{'MISAKA_RUN_TRAINED_ROLES':'pythia70m-original-16-1'})
after={f:sha(Path(f)) for f in inputs};assert after==inputs and sha(Path(binary['path']))==binary['sha256']
(out/'input-sha.json').write_text(json.dumps({'before':inputs,'after':after,'binary':binary,'match':True,'sha_warms_os_cache':True},indent=2)+'\n')
print('PROFILE RESULT',json.loads((out/'sampler.json').read_text()),flush=True)
assert case['exit_code']==-24 and case['stop_reason'] is None
