import datetime,hashlib,json,os,pathlib,re,subprocess,sys,time
root=pathlib.Path('/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet');os.chdir(root)
source=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();out=root/('handoff/positive-court/A3-r1-evidence/hello2-prepared-cpu-'+source[:9]);out.mkdir(parents=True,exist_ok=False);print('SOURCE '+source+' DIR '+str(out),flush=True)
env=os.environ.copy();env.update(CARGO_TARGET_DIR='/Users/wata/.cache/misaka-positive-a3-target',CARGO_BUILD_JOBS='1',CARGO_PROFILE_DEV_DEBUG='0',CARGO_INCREMENTAL='0',ROCKSDB_STATIC='1',ROCKSDB_LIB_DIR='/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out')
env.pop('RUST_MIN_STACK',None)
base=['cargo','--config','profile.dev.package.kaspa-consensus-core.opt-level=0']
targets=[]
suites=[('bounded-header',base+['test','--offline','--locked','-p','misaka-palw-tir-artifact','--lib','bounded_v1::tests','--','--nocapture']),('clippy',base+['clippy','--offline','--locked','-p','misaka-palw-tir-artifact','--lib','--no-deps','--','-D','warnings']),('docs',['python3','docs/maintenance/check_docs.py'])]
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
source=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
files=subprocess.check_output(['git','diff','--name-only','9c4e21f56',source],text=True).splitlines()
archive=pathlib.Path(env['ROCKSDB_LIB_DIR'])/'librocksdb.a'
meta={'status':'RUNNING','source_commit':source,'source_tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],text=True).strip(),'base_commit':subprocess.check_output(['git','rev-parse','9c4e21f56'],text=True).strip(),'dependencies_upstream':['9c4e21f56'],'source_sequence':['e49e5628a','61d243bca','9df57dea8',source],'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'build_settings':{k:env[k] for k in ['CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_PROFILE_DEV_DEBUG','CARGO_INCREMENTAL','ROCKSDB_STATIC','ROCKSDB_LIB_DIR']},'static_archive':{'path':str(archive),'bytes':archive.stat().st_size,'sha256':sha(archive)},'source_files_sha256':{f:sha(pathlib.Path(f)) for f in files},'checks':[],'test_binaries':[],'count_policy':'Last outer result per Cargo-launched binary; nested child repeats excluded. Historical and other-role counts never added.'}
meta['effective_RUST_MIN_STACK']=env.get('RUST_MIN_STACK')
meta['preexisting_package_binaries']=[{'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)} for p in (pathlib.Path(env['CARGO_TARGET_DIR'])/'debug/deps').glob('misaka_palw_tir_artifact-*') if p.is_file() and p.suffix=='' and os.access(p,os.X_OK)]
meta['scope']='Core-free PALWTIR1 header parser/split-table data binding only; not generic artifact startup or process execution.'
meta['archive_note']='Existing supplied archive recorded for host build policy; artifact leaf crate has no Core/RocksDB dependency.'
assert pathlib.Path(env['CARGO_TARGET_DIR']).is_dir(), 'existing A3 cache required'
def save():(out/'validation.json').write_text(json.dumps(meta,indent=2)+'\n')
save()
for name,cmd in suites:
 log=out/(name+'.log');start=time.time();print('START '+name,flush=True)
 with log.open('wb') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,env=env)
 s=log.read_text(errors='replace');top=[]
 for section in re.split(r'(?m)^\s*Running [^\n]*\([^\n]*\)\n',s)[1:]:
  m=re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',section)
  if m:
   a,b,c=m[-1];top.append({'passed':int(a),'failed':int(b),'ignored':int(c)})
 row={'name':name,'command':cmd,'exit_code':r.returncode,'elapsed_seconds':round(time.time()-start,3),'log':log.name,'log_sha256':sha(log),'top_level_results':top,'passed':sum(x['passed'] for x in top),'failed':sum(x['failed'] for x in top)};meta['checks'].append(row)
 for path in re.findall(r'Running[^\n]*\((/[^\n)]+)\)',s):
  p=pathlib.Path(path)
  if p.is_file() and path not in [x['path'] for x in meta['test_binaries']]:meta['test_binaries'].append({'path':path,'sha256':sha(p),'bytes':p.stat().st_size})
 save();print('END '+name+' exit='+str(r.returncode)+' passed='+str(row['passed']),flush=True)
 if r.returncode:meta['status']='FAILED';save();sys.exit(r.returncode)
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==source
assert all(sha(pathlib.Path(f))==digest for f,digest in meta['source_files_sha256'].items())
meta['source_files_stable']=True
meta['static_archive_stable']=sha(archive)==meta['static_archive']['sha256']
assert meta['static_archive_stable']
meta.update(status='PASS_BOUNDED_PALWTIR1_HEADER_ONLY_NOT_STARTUP',passed=sum(x['passed'] for x in meta['checks']),failed=sum(x['failed'] for x in meta['checks']),completed_utc=datetime.datetime.now(datetime.timezone.utc).isoformat());assert meta['passed']==5;save();print('TOTAL '+str(meta['passed']),flush=True)
