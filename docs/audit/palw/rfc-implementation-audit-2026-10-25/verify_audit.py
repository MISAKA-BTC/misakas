#!/usr/bin/env python3
"""Verify all 38 audit rows, their cited baseline sources and the exact counts.
This validates evidence identity and arithmetic, not protocol correctness.
"""
from pathlib import Path
from collections import Counter
import hashlib,json,re,subprocess
BASELINE='0ec2efc4c3ead4d2b63c994d4744c59545a75626'
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
def number(value):
    return value if isinstance(value,int) else int(re.search(r'\d+',str(value)).group())
def records():
    rows=[]
    for name in ['rfc01-12.json','rfc13-25.json','rfc26-38.json']:
        part=json.loads((HERE/name).read_text())
        assert isinstance(part,list),name
        for r in part:
            r=dict(r);r['rfc']=number(r['rfc']);r['source_report']=name;rows.append(r)
    assert sorted(r['rfc'] for r in rows)==list(range(1,39)), 'missing or duplicate RFC row'
    for r in rows:
        assert r['requirements'],r['rfc']
        assert r['primary_document'].startswith(f"docs/rfc/{r['rfc']:04d}-"),r['rfc']
        if r['complete']:
            assert r['implementation_status']=='COMPLETE',r['rfc']
            assert r['acceptance_status'] not in ['PARTIAL','MISSING','UNKNOWN','EXTERNAL_GATE_PENDING'],r['rfc']
        if r['implementation_status'] in ['DEFERRED','SUPERSEDED','ABSENT']:
            assert not r['complete'],r['rfc']
    return sorted(rows,key=lambda r:r['rfc'])
def category(r):
    if r['complete']:return 'VERIFIED_COMPLETE'
    if r['implementation_status']=='COMPLETE':return 'CODE_COMPLETE_ACCEPTANCE_PENDING'
    if r['implementation_status'] in ['PARTIAL','UNKNOWN']:return 'PARTIAL_OR_UNVERIFIED'
    return r['implementation_status']
def references(value):
    if isinstance(value,dict):
        if 'path' in value and 'line' in value:yield value
        for v in value.values():yield from references(v)
    elif isinstance(value,list):
        for v in value:yield from references(v)
def baseline(path):
    return subprocess.check_output(['git','show',f'{BASELINE}:{path}'],cwd=ROOT,stderr=subprocess.DEVNULL)
def main():
    rows=records();bycategory={};cache={};count=0
    for r in rows:
        bycategory.setdefault(category(r),[]).append(r['rfc'])
        baseline(r['primary_document'])
        for ref in references(r):
            path=ref['path']
            if path not in cache:cache[path]=baseline(path)
            data=cache[path];lines=data.decode(errors='replace').splitlines();line=int(ref['line'])
            assert 1<=line<=len(lines),(path,line)
            marker=ref.get('symbol_or_match',ref.get('symbol_or_excerpt',ref.get('symbol_or_anchor')))
            if marker:assert marker in lines[line-1],(path,line,marker)
            expected_sha=ref.get('sha256',ref.get('baseline_sha256'))
            if expected_sha:assert hashlib.sha256(data).hexdigest()==expected_sha,path
            count+=1
    manifest=json.loads((HERE/'source-manifest.json').read_text())
    for f in manifest['files']:
        assert hashlib.sha256(baseline(f['path'])).hexdigest()==f['sha256'],f['path']
    expected=json.loads((HERE/'summary.json').read_text())
    result={'baseline':BASELINE,'rfc_count':len(rows),'code_complete_rfcs':[r['rfc'] for r in rows if r['implementation_status']=='COMPLETE'],'verified_complete_rfcs':[r['rfc'] for r in rows if r['complete']],'categories':bycategory,'category_counts':{k:len(v) for k,v in bycategory.items()},'validated_code_citations':count,'unique_cited_code_files':len(cache),'source_manifest_files':len(manifest['files'])}
    assert result==expected,(result,expected)
    print(json.dumps({'passed':True,**result},ensure_ascii=False,indent=2))
if __name__=='__main__':main()
