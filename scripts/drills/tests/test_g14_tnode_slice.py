import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g14_tnode_slice as T


def test_missing_rows_never_pass_and_waiver_never_passes():
    led = T.ledger(None, None, 'x')
    st = {r['t']: r['status'] for r in led}
    assert 'PASS' not in st.values()
    assert st['T06'] == 'WAITING-INTEGRATION' and st['T01'] == 'NOT-IMPLEMENTED' and st['T14'] == 'BLOCKED'
    reqs = T.requirement_results(led, {'sourceCommit': 'a' * 40}, 'cmd')
    assert all(not r['passed'] and r['exitCode'] == 1 for r in reqs)
    assert any(r['waiver'] for r in reqs)


def test_all_rows_pass_only_lifts_backed_items():
    rows = [{'phase': p, 'status': 'PASS'} for p in ('F0', 'F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7')]
    led = T.ledger({'rows': rows}, {'status': 'PASS'})
    st = {r['t']: r['status'] for r in led}
    assert st['T14'] == 'PASS' and st['T15'] == 'PASS' and st['T01'] == 'NOT-IMPLEMENTED'
    reqs = {(r['fence'], r['requirementIndex']): r['passed'] for r in T.requirement_results(led, {}, 'c')}
    assert reqs[('palw_probabilistic_constraints_v1', 7)] is True
    assert reqs[('palw_probabilistic_constraints_v1', 1)] is False  # rests on T01/T03/T05 too
    assert reqs[('palw_probabilistic_constraints_v1', 4)] is False  # waiver


def test_a_fail_row_fails_the_item():
    rows = [{'phase': 'F3', 'status': 'FAIL'}]
    st = {r['t']: r['status'] for r in T.ledger({'rows': rows}, None)}
    assert st['T14'] == 'FAIL'
