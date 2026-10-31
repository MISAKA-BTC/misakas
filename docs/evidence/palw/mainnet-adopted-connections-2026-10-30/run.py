import pathlib,re,subprocess,json,time,sys,datetime,os,hashlib
from source_manifest import manifest
os.environ.setdefault("RUST_MIN_STACK", "67108864")
root=pathlib.Path('/Users/wata/Downloads/MISAKA-testnet/.claude/worktrees/agent-aa4dfb62c6c3c943d')
out=pathlib.Path(__file__).parent
before=manifest()
expected=json.loads((out/'source-before-build.json').read_text())
if before != expected: raise SystemExit('source differs from build candidate; rebuild required')
(out/'source-before-tests.json').write_text(json.dumps(before,indent=2)+'\n')
log=(out/'unified-build.log').read_text()
binaries={}
for path in re.findall(r'Executable unittests .*? \(([^)]+)\)',log):
 p=root/path
 for name in ['kaspa_consensus_core','kaspa_consensus','kaspad_lib','kaspa_p2p_flows','kaspa_p2p_lib','misaka_palw_sdk','kaspa_mining']:
  if p.name.startswith(name+'-'): binaries[name]=str(p)
plan={'kaspa_consensus_core': ['finite_provider_fee', 'ordinary_verifier_fee', 'native_common_capital_', 'ordinary_fee', 'verifier_draw', 'material_intake', 'a_positive_material_storage_fee', 'family_supplier', 'g14_native_funded_candidate_fixture_generator_v1', 'funded_escrow', 'original_aggregate', 'r5_required_amount', 'rfc40', 'rfc23', 'rfc0038', 'palw_fp_answer_accounting', 'palw_row_stream_v1', 'direct_row_export', 'rfc37', 'the_state_root_preimage_is_exactly_the_adr_0043_list', 'the_version_22_state_root_golden_vectors', 'rfc0041', 'rfc0012_native39_receipt_quantum_is_work', 'family_material_', 'tokenizer_authenticated_table_geometry_', 'native39_', 'closed_fact_', 'family_source_and_prefix_consumer_', 'accounting_binding_codec_bounds_and_wrapper_order', 'existing_inner_schema_ceilings_', 'rooted_table_', 'shared_constraint_material_builder_reconstructs_real_native_original_and_rechecks', 'shared_tokenizer_builder_uses_native_original_eof_and_rechecks_authenticated_material'], 'kaspa_consensus': ['rfc0041_production_recovery', 'funded_original', 'family_native39_', 'rfc37_', 'rfc02_native39_node_e2e_register_fund_final_gate_c_pnr9_coinbase_spend_reorg_restart', 'rfc23s2_no_seam_', 'native_bond_exit_checked_readers_', 'panel_binding_collateral_faults_', 'rfc23s2_node_checkpoint', 'rfc23s2_native39_', 'rfc0012_', 'rfc12_c1_'], 'kaspad_lib': ['accounting_seat', 'recovery_draws', 'recovery_adopts', 'rfc37_row_stream_cli', 'tokenizer_scheduler'], 'kaspa_p2p_flows': ['row_snapshot', 'row_stream_protocol', 'palw_snapshot_chunks', 'rfc37_', 'compression_'], 'kaspa_p2p_lib': ['row_stream_generation'], 'misaka_palw_sdk': ['sdk_family_'], 'kaspa_mining': ['native_bond_exit_local_retry_']}
def sha256(path):
 h=hashlib.sha256()
 with open(path,'rb') as stream:
  for chunk in iter(lambda:stream.read(1024*1024),b''):h.update(chunk)
 return h.hexdigest()
binary_hashes={name:sha256(path) for name,path in binaries.items()}
(out/'binary-identities.json').write_text(json.dumps({name:{'path':binaries[name],'sha256':digest} for name,digest in binary_hashes.items()},indent=2)+'\n')
results=[]
for package,filters in plan.items():
 if package not in binaries: raise SystemExit('missing binary '+package)
 for match in filters:
  name=package+'--'+match
  listing=subprocess.check_output([binaries[package],match,'--list'],cwd=root,text=True)
  declared=[line.removesuffix(': test') for line in listing.splitlines() if line.endswith(': test')]
  start=time.time()
  with (out/(name+'.log')).open('w') as f:
   proc=subprocess.run([binaries[package],match,'--nocapture','--test-threads=2'],cwd=root,stdout=f,stderr=subprocess.STDOUT)
  data=(out/(name+'.log')).read_text()
  counts=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored',data)
  counts=counts[-1:]
  passed=sum(int(c[1]) for c in counts)
  record={'package':package,'filter':match,'exit_code':proc.returncode,'passed':passed,'failed':sum(int(c[2]) for c in counts),'ignored':sum(int(c[3]) for c in counts),'elapsed_seconds':round(time.time()-start,2),'declared_tests':declared}
  results.append(record)
  (out/'results.json').write_text(json.dumps(results,indent=2)+'\n')
  print(json.dumps(record),flush=True)
  if proc.returncode or record['passed']==0: print(data[-7000:],flush=True)
after=manifest()
(out/'source-after-tests.json').write_text(json.dumps(after,indent=2)+'\n')
if before != after: raise SystemExit('source changed during tests: candidate invalid, rebuild and rerun required')
if binary_hashes != {name:sha256(path) for name,path in binaries.items()}: raise SystemExit('test binaries changed during tests: rerun required')
print('DONE',sum(x['passed'] for x in results),'reported passes; failures',sum(x['exit_code']!=0 for x in results),flush=True)
sys.exit(any(x['exit_code']!=0 or x['passed']==0 for x in results))
