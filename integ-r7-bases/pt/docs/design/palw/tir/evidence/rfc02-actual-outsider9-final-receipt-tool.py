import argparse,datetime,hashlib,json,pathlib,shlex,subprocess,tarfile
ROOT=pathlib.Path('/Users/wata/.codex/worktrees/08bd/MISAKA-testnet').resolve()
BASE=pathlib.Path('/tmp/rfc02-actual-outsider9-final')
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
def write(p,d):p.write_text(json.dumps(d,ensure_ascii=False,indent=2)+'\n')
def now():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def files():
 out=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=ROOT)
 return sorted({x.decode() for x in out.split(b'\0') if x and (ROOT/x.decode()).is_file()})
def capture():
 paths=files(); hashes={p:sha(ROOT/p) for p in paths}
 d={'recorded_utc':now(),'workspace':str(ROOT),'git_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'whole_workspace_file_preimages':hashes,'scope':'All tracked and nonignored existing workspace files at coordinated source barrier. The postbuild compiler-artifact/.d manifest selects the compiled local source closure; unexported drafts and changing documentation are not asserted to be compiled.'}
 write(BASE.with_name(BASE.name+'-workspace-preimage.json'),d)
 select=[p for p in paths if pathlib.Path(p).suffix in ('.rs','.toml','.lock','.borsh','.tir') or p in ('.cargo/config','.cargo/config.toml','rust-toolchain','rust-toolchain.toml','docs/design/palw/tir/evidence/rfc02-actual-outsider9-protocol-v2.json')]
 with tarfile.open(BASE.with_name(BASE.name+'-workspace-source-preimages.tar.gz'),'w:gz') as tf:
  for p in select:tf.add(ROOT/p,arcname=p,recursive=False)
 write(BASE.with_name(BASE.name+'-archived-files.json'),{'paths':select,'archive_sha256':sha(BASE.with_name(BASE.name+'-workspace-source-preimages.tar.gz'))})
 print(json.dumps({'captured_files':len(paths),'archived_source_files':len(select)}))
def dep_candidates(f):
 p=pathlib.Path(f); yield p.with_suffix('.d')
 if p.name=='build-script-build':
  yield p.parent/('build_script_build-'+p.parent.name.rsplit('-',1)[-1]+'.d')
 if p.name.startswith('lib'):yield p.with_name(p.name[3:]).with_suffix('.d')
def finalize(logs):
 pre=json.loads(BASE.with_name(BASE.name+'-workspace-preimage.json').read_text())['whole_workspace_file_preimages']; old=set(pre)
 local=set(); artifacts=[]; external_generated=set(); failed=[]
 for log in logs:
  for line in pathlib.Path(log).read_text().splitlines():
   try:o=json.loads(line)
   except json.JSONDecodeError:continue
   if o.get('reason')!='compiler-artifact':continue
   src=pathlib.Path(o['target']['src_path'])
   try:src.relative_to(ROOT)
   except ValueError:continue
   candidates=set()
   for f in o['filenames']:
    candidates.update(p for p in dep_candidates(f) if p.is_file())
   if o.get('executable'):candidates.update(p for p in dep_candidates(o['executable']) if p.is_file())
   # Cargo's non-test bin artifact is copied from deps under an unhashed
   # hyphenated name. Select .d only by exact equal immutable payload,
   # never by newest glob or an unrelated cached hash.
   if not candidates and o.get('executable') and o['target']['kind']==['bin']:
    ep=pathlib.Path(o['executable']);eh=sha(ep)
    for dp in (ep.parent/'deps').glob(o['target']['name'].replace('-','_')+'-*.d'):
     other=dp.with_suffix('')
     if other.is_file() and other.stat().st_size==ep.stat().st_size and sha(other)==eh:candidates.add(dp)
   a={'package_id':o['package_id'],'target':o['target'],'features':o['features'],'profile':o['profile'],'fresh':o['fresh'],'filenames':o['filenames'],'executable':o.get('executable'),'dep_info':[]}
   if not candidates:failed.append({'package_id':o['package_id'],'target':o['target']['name'],'reason':'missing dep-info'})
   for dp in sorted(candidates):
    a['dep_info'].append({'path':str(dp),'sha256':sha(dp)})
    data=dp.read_text()
    for dl in data.splitlines():
     if dl.startswith('#') or ': ' not in dl:continue
     _,deps=dl.split(': ',1)
     for s in shlex.split(deps):
      p=pathlib.Path(s)
      if not p.is_absolute():p=ROOT/p
      if not p.is_file():continue
      try:rel=str(p.resolve().relative_to(ROOT))
      except ValueError:
       if '/build/' in str(p) and '/out/' in str(p):external_generated.add(str(p))
       continue
      if rel.startswith('target/'):
       external_generated.add(str(p));continue
      local.add(rel)
   artifacts.append(a)
   # Manifest ancestry for each local package is explicit, including workspace.
   for par in [src.parent,*src.parents]:
    if par==ROOT.parent:break
    mf=par/'Cargo.toml'
    if mf.is_file():local.add(str(mf.relative_to(ROOT)));break
 for p in ('Cargo.toml','Cargo.lock','.cargo/config','.cargo/config.toml','rust-toolchain','rust-toolchain.toml'):
  if (ROOT/p).is_file():local.add(p)
 manifest={p:{'prebuild_sha256':pre.get(p),'postbuild_sha256':sha(ROOT/p),'unchanged':pre.get(p)==sha(ROOT/p)} for p in sorted(local)}
 changed=[p for p,v in manifest.items() if not v['unchanged']]
 d={'recorded_utc':now(),'compiled_source_manifest_method':'Cargo JSON compiler-artifact events for actual selected node and SDK builds, each local artifact .d inputs, package manifests and workspace lock/config. Cargo.lock pins external registry sources; generated external build outputs listed separately. Existing unexported Rust drafts outside .d are not claimed compiled.','local_source_preimages':manifest,'compiled_artifacts':artifacts,'missing_dep_info':failed,'local_source_changes_since_prebuild':changed,'generated_external_input_hashes':{p:sha(pathlib.Path(p)) for p in sorted(external_generated)},'noncompiled_workspace_source_files':sorted(p for p in old-set(local) if pathlib.Path(p).suffix=='.rs'),'compiled_local_source_closure_unchanged':not changed and not failed}
 if not changed and not failed:
  archive=BASE.with_name(BASE.name+'-compiled-source-preimages.tar.gz')
  with tarfile.open(archive,'w:gz') as tf:
   for rel in sorted(local):tf.add(ROOT/rel,arcname=rel,recursive=False)
  d['compiled_source_archive']={'path':str(archive),'sha256':sha(archive),'bytes':archive.stat().st_size}
 write(BASE.with_name(BASE.name+'-compiled-source-manifest.json'),d)
 print(json.dumps({'local_source_files':len(local),'local_artifacts':len(artifacts),'source_changes':changed,'missing_dep_info':failed,'compiled_closure_unchanged':d['compiled_local_source_closure_unchanged']}))
p=argparse.ArgumentParser();p.add_argument('action',choices=['capture','finalize']);p.add_argument('logs',nargs='*');args=p.parse_args()
if args.action=='capture':capture()
else:finalize(args.logs)
