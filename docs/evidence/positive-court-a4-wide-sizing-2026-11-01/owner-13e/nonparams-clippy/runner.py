from pathlib import Path
import subprocess,os,json,hashlib,time,sys
BASE=Path(__file__).resolve().parents[2]
HEAD=subprocess.check_output(['git','rev-parse','HEAD'],cwd=BASE,text=True).strip()
PREFIX=sys.argv[1]
OUT=BASE/'target/a4-wide-sizing-validation'/PREFIX
OUT.mkdir(exist_ok=False)
NATIVE=Path('/Users/wata/.codex/worktrees/rfc14-g14-completion/MISAKA-testnet/target/release/build/librocksdb-sys-865d89ee7dbf2b2a/out/librocksdb.a')
EXPECTED='6eae541967b2cb5345ace55eadb34b2c490e91acfd8a4e2da5822a60f91af3a2'
def sha(p):
    h=hashlib.sha256()
    with Path(p).open('rb') as f:
        while b:=f.read(1<<20): h.update(b)
    return h.hexdigest()
assert sha(NATIVE)==EXPECTED
ENV=os.environ.copy()
ENV.update(CARGO_TARGET_DIR=str(BASE/'target/a4-fresh'),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_INCREMENTAL='false',CARGO_PROFILE_TEST_INCREMENTAL='false',ROCKSDB_STATIC='1',ROCKSDB_LIB_DIR=str(NATIVE.parent))
ENV.pop('RUST_MIN_STACK',None)
CONFIG=['--config','profile.dev.package.kaspa-consensus-core.opt-level=0','--config','profile.test.package.kaspa-consensus-core.opt-level=0']
CARGO=['cargo',*CONFIG]
TEST=CARGO+['test','--locked','-p','kaspa-consensus-core']
TASKS=[
('01-gen-price-default-nonparams',TEST+['--test','palw_gen_close_price','--','--test-threads=2','--skip','a_decode_close_past_what_can_be_carried_is_refused_by_name_and_the_catalog_records_the_one_it_admits','--skip','the_registrable_real_vocabulary_classes_are_where_the_regime_carries_their_decode_close']),
('02-strict-core-new-target-clippy',CARGO+['clippy','--locked','-p','kaspa-consensus-core','--lib','--test','palw_tir_close_size_wide_v2','--no-deps','--','-D','warnings']),
('03-docs',['python3','docs/maintenance/check_docs.py'])]

source_files=['consensus/core/src/lib.rs','consensus/core/src/palw_tir_close_size_v1.rs','consensus/core/src/palw_tir_close_range_v1.rs','consensus/core/src/palw_tir_close_size_v2.rs','consensus/core/tests/palw_tir_close_size_wide_v2.rs']
for p in source_files: assert (BASE/p).read_bytes()==subprocess.check_output(['git','show',HEAD+':'+p],cwd=BASE)
META={'source_commit':HEAD,'source_tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=BASE,text=True).strip(),'source_sha256':{p:sha(BASE/p) for p in source_files},'runner_sha256':sha(__file__),'workspace':str(BASE),'target_dir':ENV['CARGO_TARGET_DIR'],'record_directory':str(OUT),'runner_pid':os.getpid(),'started_unix':time.time(),'env':{k:ENV.get(k) for k in ['CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG','CARGO_PROFILE_DEV_INCREMENTAL','CARGO_PROFILE_TEST_INCREMENTAL','ROCKSDB_STATIC','ROCKSDB_LIB_DIR','RUST_MIN_STACK','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS']},'cargo_config':CONFIG,'native_archive':str(NATIVE),'native_archive_sha256_before':EXPECTED,'tasks':[],'status':'running','historical_binary_preservation_manifest':str(BASE/'target/a4-final-r5-preserved-20261101-c1292bc08/preparation.json'),'historical_preservation_manifest_sha256':'45676677b0780e7a63a1298e5c2b5fd698fc16ab72ee29349850deb604ed270e'}
def save():
    temp=OUT/'validation.json.tmp'; temp.write_text(json.dumps(META,ensure_ascii=False,indent=2)+'\n'); temp.replace(OUT/'validation.json')
save(); print('RUNNER_PID',os.getpid(),'SOURCE',HEAD,'TARGET',ENV['CARGO_TARGET_DIR'],flush=True)
for name,args in TASKS:
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=BASE,text=True).strip()==HEAD
    log=OUT/(name+'.log')
    item={'name':name,'argv':args,'log':str(log),'started_unix':time.time()}; META['tasks'].append(item)
    with log.open('xb') as f:
        child=subprocess.Popen(args,cwd=BASE,env=ENV,stdout=f,stderr=subprocess.STDOUT)
        item['pid']=child.pid; save(); print('START',name,'PID',child.pid,flush=True)
        code=child.wait()
    item.update(exit_code=code,finished_unix=time.time(),raw_log_sha256=sha(log))
    META['native_archive_sha256_after_last_task']=sha(NATIVE)
    assert META['native_archive_sha256_after_last_task']==EXPECTED
    save(); print('END',name,'EXIT',code,flush=True)
    if code:
        META['status']='stopped on first failure; preserve exact source/log before diagnosis'; break
else: META['status']='commands complete; outcome counts/binary hashes require independent review'
META['finished_unix']=time.time(); META['native_archive_sha256_after']=sha(NATIVE)
META['source_unchanged_after']=all(sha(BASE/p)==v for p,v in META['source_sha256'].items())
META['free_bytes_after']=os.statvfs(BASE).f_bavail*os.statvfs(BASE).f_frsize
save(); print('RELEASE',os.getpid(),'STATUS',META['status'],'RECORD',OUT/'validation.json',flush=True)
