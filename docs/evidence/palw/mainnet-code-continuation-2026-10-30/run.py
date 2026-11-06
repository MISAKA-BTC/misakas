import pathlib,re,subprocess,json,time,sys,datetime,os
os.environ.setdefault("RUST_MIN_STACK", "67108864")
root=pathlib.Path('/Users/wata/Downloads/MISAKA-testnet/.claude/worktrees/agent-aa4dfb62c6c3c943d')
out=pathlib.Path(__file__).parent
log=pathlib.Path('/tmp/misaka-continuation-build-verified-20261030.log').read_text()
binaries={}
for path in re.findall(r'Executable unittests .*? \(([^)]+)\)',log):
 p=root/path
 for name in ['kaspa_consensus_core','kaspa_consensus','kaspad_lib','kaspa_p2p_flows','kaspa_p2p_lib']:
  if p.name.startswith(name+'-'): binaries[name]=str(p)
plan={
'kaspa_consensus_core': ['finite_provider_fee','ordinary_verifier_fee','ordinary_fee','verifier_draw','material_intake','a_positive_material_storage_fee','family_supplier','g14_native_funded_candidate_fixture_generator_v1','funded_escrow','original_aggregate','r5_required_amount','rfc40','rfc23','rfc0038','palw_fp_answer_accounting','palw_row_stream_v1','direct_row_export','rfc37','the_state_root_preimage_is_exactly_the_adr_0043_list','the_version_22_state_root_golden_vectors','rfc0041','rfc0012_native39_receipt_quantum_is_work'],
'kaspa_consensus': ['rfc0041_production_recovery','funded_original'],
'kaspad_lib': ['accounting_seat','recovery_draws','recovery_adopts'],
'kaspa_p2p_flows': ['row_snapshot','row_stream_protocol','palw_snapshot_chunks'],
'kaspa_p2p_lib': ['row_stream_generation']
}
results=[]
for package,filters in plan.items():
 if package not in binaries: raise SystemExit('missing binary '+package)
 for match in filters:
  name=package+'--'+match
  start=time.time()
  with (out/(name+'.log')).open('w') as f:
   proc=subprocess.run([binaries[package],match,'--nocapture','--test-threads=2'],cwd=root,stdout=f,stderr=subprocess.STDOUT)
  data=(out/(name+'.log')).read_text()
  counts=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored',data)
  counts=counts[-1:]
  passed=sum(int(c[1]) for c in counts)
  record={'package':package,'filter':match,'exit_code':proc.returncode,'passed':passed,'failed':sum(int(c[2]) for c in counts),'ignored':sum(int(c[3]) for c in counts),'elapsed_seconds':round(time.time()-start,2)}
  results.append(record)
  (out/'results.json').write_text(json.dumps(results,indent=2)+'\n')
  print(json.dumps(record),flush=True)
  if proc.returncode: print(data[-7000:],flush=True)
print('DONE',sum(x['passed'] for x in results),'reported passes; failures',sum(x['exit_code']!=0 for x in results),flush=True)
sys.exit(any(x['exit_code']!=0 for x in results))
