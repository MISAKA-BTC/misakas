#!/usr/bin/env python3
"""Verify the documented MOE1 delta and before archives, not runtime safety."""
from pathlib import Path
import hashlib,json,re

REV=Path(__file__).resolve().parent
D=REV.parents[2]
ROOT=D.parent
def read(p):return json.loads(p.read_text())
def sha(b):return hashlib.sha256(b).hexdigest()

baseline=read(REV/'baseline-manifest.json')
delta=read(REV/'changes.json')
changed={x['path']:x for x in delta['modified_existing']}
fail=[];unchanged=archived=0
for path,old in baseline['files'].items():
    p=ROOT/path
    if not p.is_file():fail.append('missing baseline path: '+path);continue
    data=p.read_bytes()
    if path not in changed:
        if sha(data)!=old['sha256']:fail.append('undeclared modification: '+path)
        else:unchanged+=1
        continue
    c=changed[path]
    if c['before_sha256']!=old['sha256']:fail.append('wrong baseline declaration: '+path)
    if sha(data)!=c['after_sha256']:fail.append('changed after review: '+path)
    a=ROOT/c['before_copy']
    if not a.is_file() or sha(a.read_bytes())!=old['sha256']:fail.append('before copy missing or changed: '+path)
    else:archived+=1

numbered={k:len(list((D/k).glob('[0-9][0-9][0-9][0-9]-*.md'))) for k in ['rfc','adr','rfc-v2','adr-v2']}
if numbered!={'rfc':64,'adr':227,'rfc-v2':20,'adr-v2':50}:fail.append('numbered document inventory differs: '+str(numbered))
for path in ['rfc/0063-additional-model-option-economics.md','adr/0231-additional-model-option-economics.md','governance/model-option-economics-transition.md']:
    if not (D/path).is_file():fail.append('missing new authority: '+path)
preservation=read(REV/'preservation-check.json')
accounting=read(REV/'accounting-check.json')
comparison=read(REV/'checker-comparison.json')
if preservation['status']!='PASS':fail.append('preservation checks failed')
if accounting['passed'] is not True:fail.append('integer accounting checks failed')
if comparison['no_unexplained_new_failures'] is not True:fail.append('unexplained new old-checker diagnostics')

out={'status':'PASS' if not fail else 'FAIL','scope':'declared documentation delta, exact before archives, preservation and integer-accounting report consistency',
     'baseline_files_checked':len(baseline['files']),'unchanged_existing_files':unchanged,
     'modified_existing_files':len(changed),'exact_before_archives':archived,
     'numbered_documents':numbered,'new_unexplained_checker_failures':comparison['unexplained_new_failures'],
     'old_checkers_all_pass':all(x['global_checker_passed'] for x in comparison['checkers'].values()),
     'runtime_implemented_or_tested':False,'network_activated':False,'failures':fail}
(REV/'revision-check.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(out,ensure_ascii=False,indent=2))
raise SystemExit(0 if not fail else 1)
