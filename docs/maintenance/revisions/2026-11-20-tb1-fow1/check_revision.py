#!/usr/bin/env python3
"""Check this explicit documentation adoption, without hiding historical failures.

Run after the three existing checkers and reference fixtures. This validates
document preservation and the exact allowed scope additions, not BGW/PQ/LLM
security. All paths and old hashes are fixed by adoption-integrity.json.
"""
from pathlib import Path
import hashlib
import json
import re
import sys
import zipfile

REV=Path(__file__).resolve().parent
ROOT=REV.parents[3]
D=ROOT/'docs'
OUT=REV/'validation'
MARK='TB1-FOW1-ADOPTED-20261120'
errors=[]
stats={}

def sha(b):return hashlib.sha256(b).hexdigest()
def read(p):return json.loads(p.read_text())
def require(ok, message):
    if not ok:errors.append(message)
def normalized(t):
    t=re.sub(r'\n?<!-- '+MARK+r'-BEGIN -->.*?<!-- '+MARK+r'-END -->\n?', '\n',t,flags=re.S)
    first,sep,rest=t.partition('\n')
    return (first+'\n'+rest.lstrip('\n')).rstrip('\n')+'\n'

integrity=read(REV/'adoption-integrity.json')
for row in integrity['immutable_files']:
    p=ROOT/row['path']
    require(p.is_file() and sha(p.read_bytes())==row['sha256'],'Protected source changed: '+row['path'])
stats['immutable_files']=len(integrity['immutable_files'])

for row in integrity['current_books_with_body_preservation']:
    p=ROOT/row['path'];t=p.read_text()
    start='<!-- '+MARK+'-BEGIN -->';end='<!-- '+MARK+'-END -->'
    require(t.count(start)==t.count(end)==1,'Scope block count: '+row['path'])
    require(sha(normalized(t).encode())==row['normalized_original_sha256'],'Previous current-book body changed: '+row['path'])
    if '/rfc-v2/' in row['path'] and '## 原条文の統合収録' in t:
        require(t.index(end)<t.index('## 原条文の統合収録'),'RFC scope inserted into historical payload: '+row['path'])
    if '/adr-v2/' in row['path']:
        require(t.index(start)>t.rfind('<!-- ADR_SOURCE_END:'),'ADR scope inserted before historical payload ended: '+row['path'])
stats['current_books_preserved_with_explicit_scope']=len(integrity['current_books_with_body_preservation'])

six=integrity['authorized_six_scope_additions']
require(len(six)==6 and len({r['path'] for r in six})==6,'Expected six exact authorized legacy-checker deltas')
for row in six:
    p=ROOT/row['path'];t=p.read_text()
    start='<!-- '+MARK+'-BEGIN -->';end='<!-- '+MARK+'-END -->'
    require(sha(p.read_bytes())==row['after_sha256'],'Authorized addition bytes changed: '+row['path'])
    require(t.count(start)==t.count(end)==1,'Authorized addition has wrong marker count: '+row['path'])
    if t.count(start)==t.count(end)==1:
        i=t.index(start);j=t.index(end)+len(end)
        require(t[:i].strip()==t.splitlines()[0].strip(),'Scope block not immediately after title: '+row['path'])
        require(sha(t[i:j].encode())==row['scope_block_sha256'],'Scope block hash changed: '+row['path'])
    require(sha(normalized(t).encode())==row['original_normalized_sha256'],'Authorized notice also changed previous body: '+row['path'])
stats['exact_authorized_scope_additions']=len(six)

base=REV/'checker-baseline'
docs_before=read(base/'docs.json')['failures']
docs_after=read(OUT/'docs.json')['failures']
require(set(docs_before)==set(docs_after),'Document-link failure set differs from baseline')
stats['docs_checker']={'status':'FAIL','before':len(docs_before),'after':len(docs_after),'new':len(set(docs_after)-set(docs_before))}

rfc_before=read(base/'rfc.json')['failures']
rfc_text=(OUT/'rfc.stdout.log').read_text()+'\n'+(OUT/'rfc.stderr.log').read_text()
rfc_after=[x for x in rfc_text.splitlines() if x.startswith('FAIL:')]
require(set(rfc_before)==set(rfc_after),'RFC integrity failure set differs from baseline')
stats['rfc_checker']={'status':'FAIL','before':len(rfc_before),'after':len(rfc_after),'new':len(set(rfc_after)-set(rfc_before))}
(OUT/'rfc.json').write_text(json.dumps({'name':'rfc','exit_code':read(OUT/'rfc.run.json')['exit_code'],'failures':rfc_after,'failure_count':len(rfc_after)},ensure_ascii=False,indent=2)+'\n')

adr_before=read(base/'adr.json')['errors']
adr_after=read(OUT/'adr.json')['errors']
allowed={'Unchanged baseline files modified or missing: '+r['path'] for r in six}
require(set(adr_before).issubset(set(adr_after)),'A prior ADR checker failure was hidden')
require(set(adr_after)-set(adr_before)==allowed,'ADR checker delta is not exactly the six adopted scope notices')
stats['adr_checker']={'status':'FAIL','before':len(adr_before),'after':len(adr_after),
                      'new_authorized_scope_notices':len(allowed),
                      'new_unexplained':len(set(adr_after)-set(adr_before)-allowed)}

coverage=read(REV/'beacon-and-policy-coverage.json')
inventory=read(REV/'randomness-before.json')
records=coverage['all_numbered_documents_and_base_hits']
by_path={r['path']:r for r in records}
require(len(by_path)==len(records),'Duplicate coverage document')
for old in inventory['files']:
    new=by_path.get(old['path'])
    require(new is not None,'Missing original coverage document: '+old['path'])
    if new is None:continue
    require(new['sha256']==old['sha256'] and new['hit_count']==old['hit_count'],'Original inventory identity/count changed: '+old['path'])
    stripped=[{k:v for k,v in h.items() if k not in {'resolution','new_profile_rule'}} for h in new['hits']]
    require(stripped==old['hits'],'Original occurrence quotes or coordinates changed: '+old['path'])
    require(all(h.get('resolution') for h in new['hits']),'Unresolved old occurrence: '+old['path'])
    require(sha((ROOT/new['path']).read_bytes())==new['sha256_after'],'Coverage current hash stale: '+new['path'])
numbered={p.relative_to(ROOT).as_posix() for folder in ['rfc','adr','rfc-v2','adr-v2'] for p in (D/folder).glob('[0-9][0-9][0-9][0-9]-*.md')}
require(numbered=={r['path'] for r in records if r.get('numbered_document')},'Numbered-document coverage not exhaustive')
require(len(numbered)==359,'Unexpected numbered-document count')
require(sum(r.get('hit_count',0) for r in records)==5252,'Baseline occurrence count not preserved')
stats['covered_numbered_documents']=len(numbered)
stats['covered_baseline_occurrences']=sum(r.get('hit_count',0) for r in records)

for folder,expected in [('rfc-v2',20),('adr-v2',50)]:
    registry=read(D/folder/'registry.json')
    require(len(list((D/folder).glob('[0-9][0-9][0-9][0-9]-*.md')))==expected,'Current book count changed: '+folder)
    later={r['path'] for r in registry['post_consolidation_documents']}
    require(set(integrity['authority']).issubset(later),'New authorities missing in '+folder+' registry')

consumer=read(D/'design/palw/consensus/tb1-beacon-consumers.json')
cs={c['consumer']:c for c in consumer['consumers']}
require(consumer['enabled'] is False and consumer['status']=='ADOPTED_DESIGN_NOT_ACTIVATED','Consumer contract incorrectly activated')
require(consumer['global_constraints']['fallback_sources']==[],'Unexpected beacon fallback')
require(cs['FOW1_ADMISSION_ORDER']['new_independent_lottery'] is False,'Extra FOW1 admission lottery')
require(cs['MINING_TICKET']['extra_future_ticket_beacon'] is False,'Extra Ticket beacon')
require(cs['COURT_NEGATIVE_AND_MATERIAL']['waits_for_new_beacon'] is False,'New beacon wait on accepted remedy')
require({c['purpose'] for c in consumer['consumers'] if c['kind']=='independent_tb1_round'}=={'EXEC','PANEL','SCHEDULE','ROLE'},'Independent purpose map changed')
stats['consumer_rows']=len(cs)

archive=REV/'upstream/Misaka_PALW_TB1_Design_Bundle.zip'
require(sha(archive.read_bytes())=='f94c88ea686fb8899decef808b856209750f1f8900f5dd7811fee81d7e9c5697','Upstream attachment bytes changed')
ref=D/'reference/palw-tb1/v1'
with zipfile.ZipFile(archive) as z:
    for leaf,up in [('beacon_model.py','reference/beacon_model.py'),('test_beacon_model.py','reference/test_beacon_model.py'),('bgw_certificate_check.py','reviews/bgw_certificate_check.py')]:
        require((ref/leaf).read_bytes()==z.read('Misaka_PALW_TB1_Design_Bundle/'+up),'Upstream reference code changed: '+leaf)
fixture=read(ref/'results.json')
require(fixture.get('tests_run')==29 and fixture.get('success') is True and fixture.get('failures')==fixture.get('errors')==0,'Fixture suite not successful')
arithmetic=read(ref/'bgw_certificate_check.json')
require(arithmetic.get('false_polynomial_and_corruption_cases')==103152 and arithmetic.get('public_quorum_rejects_all_false_polynomials') is True,'Arithmetic reference check failed')
stats['reference_fixture_tests']=29
stats['separate_reference_arithmetic_cases']=103152
stats['production_cryptography_or_LLM_cost_verified']=False

out={'passed':not errors,'scope':'exact documentation adoption, source preservation and reference-fixture checks only',
     'historical_checkers_are_not_marked_pass':True,'statistics':stats,'errors':errors}
(OUT/'revision-check.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(out,ensure_ascii=False,indent=2))
sys.exit(0 if not errors else 1)
