import subprocess,os,pathlib,json,time,hashlib
binary='/tmp/misaka-public-contract-test.fd7edK/debug/deps/kaspa_consensus_core-698db0d76a225778'
filters=['tfo3_', 'tfp1_public_original_pairs_exact_native_identity_before_and_after_signing', 'palw_tir_funded_original_envelope_v1::', 'palw_tir_funded_original_envelope_v2::', 'funded_escrow_provisional_lock_names_only_a_paired_tfo1_own_output', 'g14_native_funded_batch_candidate_v1', 'family_supplier_ordinary_native_original_and_financial_transitions']
env=dict(os.environ,RUST_MIN_STACK='67108864')
results=[]
with open('/tmp/t12-tfo3-final-tests-20261111.log','w') as out:
 for f in filters:
  out.write('\nFILTER '+f+'\n');out.flush()
  start=time.monotonic()
  r=subprocess.run([binary,f,'--nocapture'],stdout=out,stderr=subprocess.STDOUT,env=env)
  results.append({'filter':f,'exit':r.returncode,'elapsed_seconds':round(time.monotonic()-start,3)})
  if r.returncode:break
pathlib.Path('/tmp/t12-tfo3-final-results-20261111.json').write_text(json.dumps({'binary':binary,'binary_sha256':hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),'results':results},indent=2)+'\n')
raise SystemExit(any(r['exit'] for r in results))
