from pathlib import Path
import json,runpy,subprocess,hashlib,os
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet');out=root/'handoff/positive-court/legacy-cert-aux-20261102';ns=runpy.run_path(str(out/'operator-scripts/fresh-functional-controller-20261102.py'));old=json.loads((root/'handoff/positive-court/three-roles-20261102/runtime-r3.json').read_text())['command'];command=[];i=0
while i<len(old):
 if old[i]=='--config' and 'native-links-r2-reference' in old[i+1]:i+=2;continue
 value=old[i]
 if value.startswith(('profile.dev.package.kaspa-consensus-core.opt-level=','profile.test.package.kaspa-consensus-core.opt-level=','profile.dev.package.misaka-palw-tir.opt-level=','profile.test.package.misaka-palw-tir.opt-level=','profile.dev.package.misaka-palw-tir-exec.opt-level=','profile.test.package.misaka-palw-tir-exec.opt-level=')):value=value.rsplit('=',1)[0]+'=0'
 command.append(value);i+=1
sdk=subprocess.check_output(['xcrun','--show-sdk-path'],text=True).strip();assert Path(sdk).is_dir();extra={'CARGO_TARGET_DIR':'/Users/wata/Downloads/MISAKA-testnet/target/positive-legacy-native-fresh-20261102','CARGO_BUILD_JOBS':'2','CC':'/usr/bin/clang','CXX':'/usr/bin/clang++','ROCKSDB_COMPILE':'1','SNAPPY_COMPILE':'1','SDKROOT':sdk,'BINDGEN_EXTRA_CLANG_ARGS':'-isysroot '+sdk}
registry=Path('/Users/wata/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f');pins={}
for name in ['librocksdb-sys-0.17.3+10.4.2','rocksdb-0.23.0']:
 base=registry/name
 if not base.exists():continue
 for file in ['Cargo.toml','build.rs','.cargo-checksum.json']:
  p=base/file
  if p.is_file():pins[str(p)]=hashlib.sha256(p.read_bytes()).hexdigest()
(out/'fresh-native-inputs.json').write_text(json.dumps({'reason':'shared debug artifacts and separately reused native archives now absent','fresh_compile_not_absent_cache_restoration':True,'functional_only_not_performance_or_activation':True,'environment_overrides':extra,'native_input_sha256':pins,'command':command,'debug_assertions_and_overflow_checks':True},indent=2)+'\n')
r=ns['run']('exec-fresh-functional-first',command,3600,2*1024**3,16*1024**3,extra);assert r['exit_code']==0 and r['stop_reason'] is None
