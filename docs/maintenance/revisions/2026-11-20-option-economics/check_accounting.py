#!/usr/bin/env python3
"""Independent integer examples and conservation checks for the MOE1 document.

This is not the native monetary engine, AMM implementation, EVM implementation,
consensus simulator, security proof, or a proof of economic profitability.
"""
from pathlib import Path
import json

REV=Path(__file__).resolve().parent
D=REV.parents[2]
policy=json.loads((D/'design/palw/economics/moe1-economic-policy.json').read_text())
assert policy['status']=='ADOPTED_DESIGN_NOT_ACTIVATED'
assert policy['registration']['additional_registration_and_market_creation']=='one atomic transition'
assert policy['registration']['normal_registered_without_market_state'] is False
assert policy['registration']['genesis_and_fork_registration_creates_market'] is False

count=0
samples=[]
# Wide integers exercise the documented widened-multiplication requirement.
amounts=list(range(0,10001))+[2**32-1,2**32,2**64-2,2**64-1]
for R in amounts:
    B=(R*500)//1000
    Drem=R-B
    assert B+Drem==R and 0<=B<=R and Drem>=0
    # Independent half-split characterization, including odd values.
    assert B==R//2 and Drem==(R+1)//2
    count+=1
    for kind in ('GENESIS','FORK_DESIGNATED'):
        row=policy['reward'][kind]
        M=R*row['miner_permille']//1000
        assert M==R and row['buyback_permille']==row['panel_from_R']==row['owner_from_R']==row['other_role_from_R']==0
        count+=1
    # This tests the explicitly marked old 20% remainder example, not a universal pool.
    P=Drem*200//1000
    M=Drem-P
    assert B+P+M==R and P==Drem//5 and 0<=M<=R
    count+=1
    if R in (0,1,2,999,1000,1001,2**64-1):
        samples.append({'R_min_units':R,'additional_example':{'buyback':B,'panel_20pct_of_remainder_example':P,'miner_example':M},'genesis_miner':R,'fork_designated_miner':R})

trade_samples=[]
for G in amounts:
    burn=G*policy['normal_amm_trade']['burn_permille']//1000
    owner=G*policy['normal_amm_trade']['owner_total_permille']//1000
    net=G-burn-owner
    assert burn+owner+net==G and min(burn,owner,net)>=0
    assert burn==G//20 and owner==(3*G)//100
    # Residue avoids the loss caused by independently rounding the 92% leg.
    assert 0<=net-(G*920//1000)<=2
    count+=1
    if G in (0,1,19,20,33,34,999,1000,1001,2**64-1):
        trade_samples.append({'gross_min_units':G,'burn':burn,'owner_total':owner,'net':net})

assert next(x for x in samples if x['R_min_units']==1000)['additional_example']=={'buyback':500,'panel_20pct_of_remainder_example':100,'miner_example':400}
assert next(x for x in trade_samples if x['gross_min_units']==1000)=={'gross_min_units':1000,'burn':50,'owner_total':30,'net':920}
assert policy['protocol_buyback']['extra_bili_burn_permille']==policy['protocol_buyback']['owner_direct_permille']==0
assert policy['direct_position_transfer']['amm_burn_permille']==policy['direct_position_transfer']['owner_permille']==0
out={'passed':True,'scope':'documented integer allocation and conservation examples only',
     'amounts_checked':len(amounts),'conservation_cases':count,
     'native_or_evm_implemented':False,'economic_profitability_proved':False,
     'reward_examples':samples,'trade_examples':trade_samples}
(REV/'accounting-check.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({k:v for k,v in out.items() if k not in ['reward_examples','trade_examples']},ensure_ascii=False))
