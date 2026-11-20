"""Independent finite-field check of the public-certificate threshold.
This checks interpolation combinatorics, not BGW or PQ signatures.
Corrupt holders are allowed to sign arbitrary false values authentically.
"""
import itertools, json
from pathlib import Path
P=17; N=7; F=2; Q=N-F
xs=tuple(range(1,N+1))
true=(3,5,7)
def ev(a,x):return sum(c*pow(x,j,P) for j,c in enumerate(a))%P
checks=0; max_supported=0; witness=None
for corrupt in itertools.combinations(xs,F):
    honest=set(xs)-set(corrupt)
    for a in itertools.product(range(P),repeat=F+1):
        if a==true:continue
        # Corrupt actors may authenticate ANY chosen value on the false poly.
        supported=set(corrupt)|{x for x in honest if ev(a,x)==ev(true,x)}
        checks+=1
        if len(supported)>max_supported:
            max_supported=len(supported);witness=(corrupt,a,sorted(supported))
        assert len(supported)<Q
result={"field":P,"n":N,"f":F,"public_quorum":Q,
        "false_polynomial_and_corruption_cases":checks,
        "max_support_for_false_poly":max_supported,
        "weak_quorum_counterexample":witness,
        "public_quorum_rejects_all_false_polynomials":True,
        "production_crypto_test":False}
Path(__file__).with_suffix('.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result,indent=2))
