"""Read-only checks of the uploaded snapshot, not a Rust test runner.
Usage: python reproduce_static_checks.py /path/to/extracted/misakas-pre
"""
from __future__ import annotations
from pathlib import Path
import hashlib
import json
import math
import sys

root = Path(sys.argv[1]) if len(sys.argv)>1 else Path('/mnt/data/audit_current/misakas-pre')
if not (root/'Cargo.toml').exists():
    raise SystemExit(f'Repository not found: {root}')

def source(p: str) -> str:
    return (root/p).read_text(encoding='utf-8')

supply = source('consensus/core/src/palw_rfc0016_v1/capacity.rs')
assert 'if palw_capacity_claims_per_span_v1(&a.capability, &load) == 0' in supply
assert 'entry.0 = entry.0.max(a.capability.compute_ccu_per_span);' in supply
# Exact arithmetic of the existing helper and one-class water-filling branch.
cap = dict(compute=1000, memory=100, da=50, court=3000)
load = dict(compute=10, memory=100, da=10, court=1000)
claims = min(cap['compute']//load['compute'], cap['da']//load['da'], cap['court']//load['court'])
returned_supply = cap['compute'] if claims>0 and cap['memory']>=load['memory'] else 0
one_class_waterfill = returned_supply  # single class receives whole supply
candidate_claims = 10
candidate_demand = candidate_claims*load['compute']
case = dict(capability=cap, per_claim_load=load, true_resource_limit_claims=claims,
    returned_supply_ccu=returned_supply, candidate_claims=candidate_claims,
    candidate_demand_ccu=candidate_demand,
    current_scalar_capacity_fits=(one_class_waterfill>0 and candidate_demand<=one_class_waterfill),
    actual_joint_resources_fit=(candidate_claims<=claims),
    conservative_single_class_ccu_limit=claims*load['compute'])
assert case['current_scalar_capacity_fits'] and not case['actual_joint_resources_fit']

# Scope: files in the named production trees, including their inline tests.
rs=[]
for d in ['consensus/core/src','consensus/src','kaspad/src','misaka/palw']:
    rs.extend((root/d).rglob('*.rs'))
needles=['with_fp_class_facts_v1(', 'palw_capacity_supply_from_attestations_v1(', 'PalwCapacityAttestationV1']
refs={n:[] for n in needles}
for p in rs:
    text=p.read_text(encoding='utf-8',errors='replace')
    for n in needles:
        if n in text:
            refs[n].extend(dict(path=str(p.relative_to(root)),line=i,text=l.strip()) for i,l in enumerate(text.splitlines(),1) if n in l)

samples=[]
for d in (1,10,100,500,1000):
    n,k=1000,4
    miss = math.comb(n-d,k)/math.comb(n,k) if n-d>=k else 0.0
    samples.append(dict(intervals=n,faulty_intervals=d,sampled=k,detect_probability=1-miss))

result={
 'kind':'static source inspection and independent arithmetic; NOT Rust execution',
 'repository':str(root),
 'capacity_counterexample':case,
 'scoped_symbol_occurrences':refs,
 'new_lane_modules':{n:[str(p.relative_to(root)) for p in root.rglob(n)] for n in ['palw_court_capacity_v1.rs','g14_drill_factory.rs']},
 'sampling_assumptions':'uniform sampling without replacement; faulty set fixed before draw; every queried bad interval reveals a mismatch',
 'sampling':samples,
 'rule_e_root_wired': 'palw_fork_choice_rule_e_leaf_v3::palw_committed_state_root_v2' in source('consensus/src/pipeline/virtual_processor/processor/palw.rs'),
 'remote_rule_e_stop_present':'LeafV1Insufficient' in source('misaka/palw/remote/src/l2.rs'),
 'python_drill_tests':'129 passed, 176 subtests passed (separate saved log)',
}
print(json.dumps(result,ensure_ascii=False,indent=2))
