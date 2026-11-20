#!/usr/bin/env python3
"""Independent review checks. Does not overwrite the initial counterexamples."""
from pathlib import Path
from dataclasses import replace
import sys, copy, json, hashlib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent/'reference_impl'
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT))
import an1_reference as a

ROWS=[]
def check(name, passed, **details):
    ROWS.append({'check':name, 'passed':bool(passed), **details})
    if not passed:
        raise AssertionError(name)

def reject(name, fn):
    try:
        fn()
    except (ValueError, TypeError, KeyError) as error:
        check(name, True, error=type(error).__name__, reason=str(error))
    else:
        check(name, False)

def raw_root(kind, scope, records):
    # Deliberately bypasses the honest Tree constructor. A malicious miner can
    # hash arbitrary preimages using the public encoding, including huge leaves.
    n=len(records); L=1<<(n-1).bit_length()
    leaves=list(records)+[a.pad_record(i) for i in range(n,L)]
    current=[hashlib.sha256(a.Tree.preimage(
        'AN1-real-leaf' if i<n else 'AN1-padding-leaf',kind,scope,n,L,[i],[raw]
    )).digest() for i,raw in enumerate(leaves)]
    height=1
    while len(current)>1:
        current=[hashlib.sha256(a.Tree.preimage('AN1-internal',kind,scope,n,L,
            [height,j//2],[current[j],current[j+1]])).digest()
            for j in range(0,len(current),2)]
        height+=1
    return current[0].hex()

def register(claim):
    ledger=a.Ledger({(C['beacon_key'],C['beacon_round']):(SIGMA,10)})
    slot=ledger.register(C,JOB,0,C['owner'])
    ledger.seal(slot,claim,20,C['owner'])
    return ledger,slot

def panel_ledger(claim):
    ledger,slot=register(claim)
    panel=ledger.select_panel(slot,bytes.fromhex('44'*32),100,[f'actor-{i}' for i in range(5)])
    for actor in panel[:4]:
        ledger.approve(slot,actor)
    ledger.rows[slot].update(materials_ready=True,closed=True,delivered=True)
    return ledger,slot,panel

def main():
    global C,SIGMA,JOB,WEIGHTS
    C,SIGMA,JOB,WEIGHTS=a.fixture_context()
    claim,material=a.build_claim(C,SIGMA,JOB,WEIGHTS)
    native=a.build_native_context(C,SIGMA,JOB,claim)
    check('adversarial raw tree reproduces canonical honest root',
          raw_root('trace',claim.attempt,material.trace_records)==claim.trace_root)
    bad=copy.deepcopy(material)
    bad.trace_records[0]=a.encode({'i':0,'v':1})
    wrong=replace(claim,trace_root=raw_root('trace',claim.attempt,bad.trace_records))
    for field in ('trace_count','output_count'):
        original=getattr(wrong,field)
        for value,label in ((float(original),'integer-valued float'),(True,'bool true'),
                            (False,'bool false'),(str(original),'numeric string'),
                            (-1,'negative'),(1<<64,'unsigned64 overflow')):
            candidate=replace(wrong,**{field:value})
            reject(f'{field}: reject {label} at native seal',lambda candidate=candidate:register(candidate))
            reject(f'{field}: reject {label} at native context',
                   lambda candidate=candidate:a.build_native_context(C,SIGMA,JOB,candidate))
    ledger,slot,panel=panel_ledger(wrong)
    review=a.panel_review(ledger.native(slot),bad)
    check('wrong trace with canonical integer count produces valid Court',
          review['status']=='Refuted' and a.judge_court(ledger.native(slot).court_view(),review['court'])=='ComputationRefuted')
    outcome=ledger.apply_batch(slot,[review['court']],True,110)
    check('same-batch canonical refutation blocks four approvals and Final',
          outcome['court_results']==['ComputationRefuted'] and outcome['final'] is False and not ledger.credits,
          outcome=outcome)
    for raw,label in ((b'[]','array record'),(a.encode({'i':0,'v':0.0}),'float trace value'),
                      (a.encode({'i':0,'v':False}),'boolean trace value'),(b'','empty record')):
        sample=copy.deepcopy(material); sample.trace_records[0]=raw
        candidate=replace(claim,trace_root=raw_root('trace',claim.attempt,sample.trace_records))
        context=a.build_native_context(C,SIGMA,JOB,candidate)
        finding=a.panel_review(context,sample)
        check(f'authenticated bounded {label} is ComputationRefuted',
              finding['status']=='Refuted' and a.judge_court(context.court_view(),finding['court'])=='ComputationRefuted')
    evidence=a.make_step_evidence(native,material,0)
    for bad_index,label in ((0.0,'float'),(False,'bool')):
        forged=dict(evidence,target=replace(evidence['target'],index=bad_index))
        check(f'{label} opening index is InvalidEvidence',a.judge_court(native.court_view(),forged)=='InvalidEvidence')
        forged=dict(evidence,index=bad_index)
        check(f'{label} Court target index is InvalidEvidence',a.judge_court(native.court_view(),forged)=='InvalidEvidence')
    too_large=copy.deepcopy(material);too_large.trace_records[0]=b' '*513
    huge=replace(claim,trace_root=raw_root('trace',claim.attempt,too_large.trace_records))
    ledger,slot,panel=panel_ledger(huge)
    review=a.panel_review(ledger.native(slot),too_large)
    check('malicious oversized leaf is not labelled Refuted or Approved',
          review['status'] in {'MaterialsUnavailable','MaterialsUnauthenticated'}
          and review.get('required_scope')=={'kind':'trace','index':0}, review=review)
    outcome=ledger.apply_batch(slot,[],True,110,material_notices=[(panel[-1],review)])
    check('same-batch qualified material notice blocks oversized-root Final',
          outcome['final'] is False and len(outcome['material_holds'])==1
          and len(ledger.rows[slot]['pending'])==1 and not ledger.credits,outcome=outcome)
    check('original deadline never turns oversized-root hold into success',not ledger.finalize(slot,301))
    before=set(ledger.rows[slot]['pending'])
    reject('unselected actor cannot create a material hold',lambda:ledger.material_notice(slot,'outsider',review,111))
    reject('other original ID cannot create a material hold',lambda:ledger.material_notice(slot,panel[-1],dict(review,original_id='77'*32),111))
    reject('floating material scope index rejected',lambda:ledger.material_notice(slot,panel[-1],dict(review,required_scope={'kind':'trace','index':0.0}),111))
    check('rejected notices leave pending set unchanged',ledger.rows[slot]['pending']==before)
    for padding,label in ((None,'None'),([], 'list')):
        sample=copy.deepcopy(material);sample.trace_padding=padding
        review=a.panel_review(native,sample)
        check(f'{label} padding structure yields a material report',
              review['status'] in {'MaterialsUnavailable','MaterialsUnauthenticated'}
              and 'required_scope' in review)
    sample=copy.deepcopy(material); sample.output_records[1]=b' '*513
    review=a.panel_review(native,sample)
    check('oversized output points to the output scope',review.get('required_scope')=={'kind':'output','index':1})
    sample=copy.deepcopy(material); sample.weights[4]=float(sample.weights[4])
    review=a.panel_review(native,sample)
    check('noncanonical weight points to the model scope',review.get('required_scope')=={'kind':'model','index':4})
    model=a.model_tree(material.weights)
    for op in ('WEIGHT','EMBED'):
        i=next(i for i,ins in enumerate(native.program.instructions) if ins['op']==op)
        evidence=a.make_step_evidence(native,material,i)
        valid_index=evidence['weight'].index
        forged=dict(evidence,weight=model.opening((valid_index+1)%1060))
        check(f'{op} valid opening at wrong model index rejected',a.judge_court(native.court_view(),forged)=='InvalidEvidence')
    i=next(i for i,ins in enumerate(native.program.instructions) if ins['op']=='INPUT')
    evidence=a.make_step_evidence(native,material,i)
    forged=dict(evidence,input=native.input_tree.opening(1))
    check('INPUT valid opening at wrong token index rejected',a.judge_court(native.court_view(),forged)=='InvalidEvidence')
    changed=dict(C,nonce='66'*32)
    other_claim,other_material=a.build_claim(changed,SIGMA,JOB,WEIGHTS)
    other_native=a.build_native_context(changed,SIGMA,JOB,other_claim)
    forged=dict(evidence,original_id=other_claim.original_id)
    check('rewrite of original ID cannot transplant an old Court to new nonce',
          a.judge_court(other_native.court_view(),forged)=='InvalidEvidence')
    ledger,slot,panel=panel_ledger(claim)
    review=a.panel_review(ledger.native(slot),material)
    check('normal full replay remains approved',review['status']=='ApprovedFullReplay')
    first=ledger.apply_batch(slot,[],True,110)
    second=ledger.apply_batch(slot,[],True,111)
    check('correct claim issues one credit with atomic slot consumption',
          first['final'] is True and second['final'] is False
          and ledger.credits=={C['owner']:1} and slot in ledger.consumed)

if __name__=='__main__':
    error=None
    try:main()
    except Exception as e:
        error={'type':type(e).__name__,'message':str(e)}
    result={'review_type':'independent targeted regression, not production/native/network proof',
            'script_under_review_sha256':hashlib.sha256((ROOT/'an1_reference.py').read_bytes()).hexdigest(),
            'review_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            'checks':ROWS,'passed':sum(x['passed'] for x in ROWS),'failed':sum(not x['passed'] for x in ROWS),'error':error}
    destination=HERE/'portable_postfix_results.json'
    run_number=2
    while destination.exists():
        destination=HERE/f'portable_postfix_results_run{run_number}.json'
        run_number+=1
    destination.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='checks'},ensure_ascii=False,indent=2))
    if error:raise SystemExit(1)
