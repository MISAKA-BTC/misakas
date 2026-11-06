#!/usr/bin/env python3
"""Source-only equivalence proof for the frozen private dispatch extraction.
No compiler/tests/git index mutation. Optionally pass an applied objects.rs path.
"""
from pathlib import Path
import sys,json,collections
from dispatch_tools import *
base=Path(__file__).resolve().parent
m=json.loads((base/'manifest.json').read_text())
old=Path(m['initial_path']).read_text();candidate=Path(sys.argv[1] if len(sys.argv)>1 else m['candidate_path']);new=candidate.read_text()
assert sha(old)==m['initial_sha256'],'initial snapshot hash changed'
assert Path(m['initial_path']).stat().st_mode & 0o222 == 0,'initial snapshot is writable'
if len(sys.argv)==1:assert sha(new)==m['candidate_sha256'],'frozen candidate hash changed'
om=top_matches(old,'apply_object_inner');nm=top_matches(new,'apply_object_inner');assert len(om)==len(nm)==3
original,tail=parse_arms(old,om[-1]);outer,outer_tail=parse_arms(new,nm[-1]);assert len(original)==m['arm_count']==153
assert not any(not a['variants'] for a in original),'original main match catchall was not modeled'
for name in ['apply_object','apply_object_with_pair_v1']:
    a=fn_span(old,name);b=fn_span(new,name)
    assert old[a[0]:a[3]]==new[b[0]:b[3]],name+' changed'
    assert sha(new[b[0]:b[3]])==m['unchanged'][name]['sha256']
a=fn_span(old,'apply_object_inner');b=fn_span(new,'apply_object_inner')
alias_start=new.index('// One borrowed dispatch call keeps branch Result temporaries out of the common frame.')
assert old[:a[0]]==new[:alias_start],'existing wrappers/module prefix changed'
let_start=new.index('let handler: ApplyObjectHandlerV1',b[1],b[2])
assert old[a[1]+1:om[-1][0]]==new[b[1]+1:let_start],'common guards/prefix changed'
for i in range(2):
    assert old[om[i][0]:om[i][3]]==new[nm[i][0]:nm[i][3]],'receipt guard changed'
    assert sha(new[nm[i][0]:nm[i][3]])==m['unchanged'][f'receipt_guard_{i+1}']['sha256']
call='handler(builder, ctx, object)?;'
call_start=new.index(call,b[1],b[2]);assert new[b[1]:b[2]].count(call)==1,'not a single handler invocation'
assert old[om[-1][3]:a[3]]==new[call_start+len(call):b[3]],'common success tail changed'
assert tokens_sha(new[nm[-1][3]:call_start])==tokens_sha(';\n    '),'extra work between mapping and call'
variant_owners={};seen=[];records=m['arms']
for batch in m['batches']:
    name=batch['name'];span=fn_span(new,name)
    assert '#[inline(never)]\nfn '+name in new,'batch can inline'
    matches=top_matches(new,name);assert len(matches)==1
    actual,_=parse_arms(new,matches[0]);indices=batch['arm_indices'];assert len(actual)==len(indices)+1
    fallback=actual[-1]
    assert fallback['pattern'].strip()=='_'
    assert tokens_sha(fallback['body'])==tokens_sha('unreachable!("private object dispatcher selected another batch"),')
    for i,item in zip(indices,actual[:-1]):
        before=original[i];record=records[i];seen.append(i)
        for key in ['pattern','body','chunk']:
            assert before[key]==item[key],f'arm{i} {key} changed'
        assert sha(item['pattern'])==record['pattern_sha256']
        assert sha(item['body'])==record['body_sha256']
        assert sha(item['chunk'])==record['chunk_with_comments_sha256']
        assert tokens_sha(item['pattern'])==record['pattern_tokens_sha256']
        assert tokens_sha(item['body'])==record['body_tokens_sha256']
        assert [v for v,k in item['variants']]==record['variants']
        for variant,kind in item['variants']:
            assert variant_owners.setdefault(variant,name)==name,'guarded overlap split between handlers'
assert seen==list(range(153)),'arm omission/duplication/relative reordering'
counts=collections.Counter()
for item in outer:
    assert item['variants'],'outer wildcard is forbidden'
    assert not any(t[0]=='if' for t in lex(item['pattern'])),'outer re-evaluates guard'
    body=[t[0] for t in lex(item['body'])]
    assert len(body)==2 and body[1]==',' and body[0] in [v['name'] for v in m['batches']],body
    for variant,kind in item['variants']:
        counts[variant]+=1
        assert variant_owners[variant]==body[0],variant+' mapped to wrong handler'
assert all(n==1 for n in counts.values()),'duplicate outer variant membership'
enum_path=base/'initial/PalwConsensusObjectV2.enum.rs';enum=enum_path.read_text();assert sha(enum)==m['enum_snapshot_sha256']
t=lex(enum);j=next(j for j,x in enumerate(t) if x[0]=='{');k=close_index(t,j);n=j+1;variants=[]
while n<k:
    variants.append(t[n][0]);n+=1
    if t[n][0] in ('{','('):n=close_index(t,n)+1
    if t[n][0]=='=':
        while t[n][0]!=',':n+=1
    assert t[n][0]==',';n+=1
assert set(variants)==set(counts)==set(variant_owners) and len(variants)==m['variant_count']==156,'enum coverage differs'
last=fn_span(new,m['batches'][-1]['name'])
assert old[a[3]:]==new[last[3]:],'existing helper tail changed'
assert sha(new[last[3]:])==m['unchanged']['other_helpers_tail']['sha256']
print(json.dumps({'status':'PASS source-only equivalence','candidate':str(candidate),'candidate_sha256':sha(new),'original_sha256':sha(old),'original_arms':len(original),'enum_variants':len(variants),'private_batches':len(m['batches']),'guarded_overlap':m['guarded_overlap'],'all_arm_patterns_bodies_comments_exact':True,'outer_variant_membership_once':True,'outer_guards_reevaluated':False,'common_guards_tail_and_rollback_wrappers_exact':True,'build_or_tests_run':False},indent=2))
