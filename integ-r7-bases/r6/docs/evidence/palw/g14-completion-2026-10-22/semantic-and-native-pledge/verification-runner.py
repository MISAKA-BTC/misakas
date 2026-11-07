from pathlib import Path
import subprocess,hashlib,json,tempfile,shutil,time,os,sys
repo=Path('/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet');os.chdir(repo)
out=repo/'docs/evidence/palw/g14-completion-2026-10-22/semantic-and-native-pledge';out.mkdir()
pin=Path(tempfile.mkdtemp(prefix='g14-semantic-and-native-pledge-'));(Path('/tmp/g14-current-material-native-pin')).write_text(str(pin))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def sources():
 names=subprocess.check_output(['rg','--files','-g','!docs/**','-g','*.rs','-g','Cargo.toml','-g','Cargo.lock','-g','rust-toolchain*']).decode().splitlines()
 return {n:sha(repo/n) for n in sorted(names)}
initial=sources();base=subprocess.check_output(['git','rev-parse','HEAD']).decode().strip();patch=subprocess.check_output(['git','diff','--binary']);(out/'source.patch').write_bytes(patch)
for n in subprocess.check_output(['git','ls-files','--others','--exclude-standard']).decode().splitlines():
 p=repo/n
 if n in initial:
  target=out/'added-source'/n;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,target)
metadata=dict(base=base,pin=str(pin),sources=initial,tracked_patch_sha256=sha(out/'source.patch'),steps=[],executables={})
def save(): (out/'verification.json').write_text(json.dumps(metadata,indent=2)+'\n')
def run(cmd,name,env=None):
 assert sources()==initial,'Rust source changed while frozen'
 log=out/name;start=time.time();print('START',name,flush=True)
 with log.open('xb') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,env=env)
 metadata['steps'].append(dict(command=cmd,log=name,exit_code=r.returncode,elapsed_seconds=time.time()-start));save();print('END',name,r.returncode,flush=True)
 assert sources()==initial,'Rust source changed during native execution'
 return r.returncode
save()
cmd=['cargo','test','--release','-p','kaspa-consensus-core','--lib','--test','palw_tir_court','--test','palw_tir_parameter_cover','--test','palw_tir_original_parameter_cover','--test','palw_the_release_did_not_move','--test','palw_activation_pool_is_t12_only','--no-run','--message-format=json']
if run(cmd,'core-build.log'):sys.exit(1)
for line in (out/'core-build.log').read_text().splitlines():
 try:r=json.loads(line)
 except ValueError:continue
 if r.get('reason')=='compiler-artifact' and r.get('executable'):
  name=r['target']['name'];p=Path(r['executable']);dest=pin/name;shutil.copy2(p,dest);metadata['executables'][name]=dict(path=str(dest),sha256=sha(dest),bytes=dest.stat().st_size)
save();core=metadata['executables']['kaspa_consensus_core']['path'];failed=False
filters=['palw_tir_public_source_v1::parameter_cover_v1::native_owner_tests', 'root_input_material_store_obeys_actual_retained_flat_and_merkle_forms', 'palw_court_source_material_graph_v3::tests', 'palw_court_numerical_geometry_v3::tests', 'shared_tokenizer_builder_uses_native_original_eof_and_rechecks_authenticated_material', 'shared_constraint_material_builder_reconstructs_real_native_original_and_rechecks', 'palw_tokenizer_table_stream_v1::tests', 'palw_tir_court_v1::source_geometry::tests', 'step_material_dependency_tests', 'fixed_root_material_tests', 'root_input_tests', 'shared_history_root_binds_public_original_policy_and_literal_producer_signature', 'private_prefix_material_replay_authenticates_canonical_boundary_and_refuses_missing_other_source_or_corruption', 'g14_tir_prefix_state_full_collusion_is_collected_or_convicted_from_the_claims_own_leaves', 'semantic_material_retained_all_id_authentication_and_quota_release', 'semantic_material_rejects_missing_duplicate_tampered_and_range_invalid_eog', 'native_step_space_inventory_tests_v1', 'native_source_owner_tests_v1']
for i,f in enumerate(filters):failed=bool(run([core,f,'--nocapture'],f'core-selection-{i}.log')) or failed
for target in ['palw_tir_court','palw_tir_parameter_cover','palw_tir_original_parameter_cover','palw_the_release_did_not_move','palw_activation_pool_is_t12_only']:
 failed=bool(run([metadata['executables'][target]['path'],'--nocapture'],target+'.log')) or failed
if failed:sys.exit(1)
metadata['source_hashes_unchanged']=sources()==initial;save()

fixture=repo/'consensus/tests/fixtures/g14-funded-original-native'
metadata['native_fixture']={str(p.relative_to(repo)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in fixture.iterdir() if p.is_file()};save()
if run(['cargo','test','--release','-p','kaspa-consensus','--lib','--no-run','--message-format=json'],'native-build.log'):sys.exit(1)
arts=[]
for line in (out/'native-build.log').read_text().splitlines():
 try:r=json.loads(line)
 except ValueError:continue
 if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']=='kaspa_consensus':arts.append(r)
assert len(arts)==1
p=Path(arts[0]['executable']);dest=pin/'kaspa_consensus';shutil.copy2(p,dest);metadata['executables']['kaspa_consensus']=dict(path=str(dest),bytes=dest.stat().st_size,sha256=sha(dest));save()
if run([str(dest),'palw_funded_original_real_native_inputs_preserve_fallback_and_refuse_incomplete_admission','--nocapture'],'native-candidate-test.log'):sys.exit(1)
assert metadata['native_fixture']=={str(p.relative_to(repo)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in fixture.iterdir() if p.is_file()}
metadata['source_and_fixture_unchanged']=True;save()
