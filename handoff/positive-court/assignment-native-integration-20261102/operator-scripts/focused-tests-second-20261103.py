from pathlib import Path
import json,runpy
root=Path('/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet')
legacy=root/'handoff/positive-court/legacy-native-history-20261102'
ns=runpy.run_path(str(legacy/'operator-scripts/native-history-controller-20261102.py'))
ns['run'].__globals__['out']=root/'handoff/positive-court/assignment-native-integration-20261102'
extra=json.loads((legacy/'native-legacy-eight-erased-futures.json').read_text())['extra_environment']
common=['--message-format=json-render-diagnostics','--offline','--locked']
def run(label,args,timeout=1800):
 r=ns['run'](label,args,timeout,4*1024**3,16*1024**3,extra)
 assert r['exit_code']==0 and r['stop_reason'] is None
 return r
run('contract-all-test-clippy-second',['cargo','clippy']+common+['-p','misaka-palw-positive-contract','--lib','--tests','--','-D','warnings'],900)
run('core-focused-test-clippy-second',['cargo','clippy']+common+['-p','kaspa-consensus-core','--test','positive_assignment_body_v2','--test','positive_assignment_fence_v2','--test','positive_assignment_funds_v2','--test','positive_assignment_gate_c_v2','--test','positive_source_v2','--','-D','warnings'],900)
run('contract-all-second',['cargo','test']+common+['-p','misaka-palw-positive-contract','--lib','--tests','--','--nocapture','--test-threads=1'],900)
run('core-assignment-source-second',['cargo','test']+common+['-p','kaspa-consensus-core','--test','positive_assignment_body_v2','--test','positive_assignment_fence_v2','--test','positive_assignment_funds_v2','--test','positive_assignment_gate_c_v2','--test','positive_source_v2','--','--nocapture','--test-threads=1'])
run('native-db-filter-legacy-second',['cargo','test']+common+['-p','kaspa-consensus','--lib','--','model::stores::palw_state_v2::assignment_journal::tests','malformed_assignment_money_projection_fails_closed_in_the_shared_filter','g14_cert_kernel_route_','g14_kernel_route_a_covered_lie_is_convicted','g14_kernel_route_a_chunked_object_is_signature_checked_at_the_completing_chunk','g14r_kernel_v2_the_pursuit_resumes_after_a_node_restart_mid_filing','g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_across_reorg_replay_and_redemption','--nocapture','--test-threads=1'],2400)
