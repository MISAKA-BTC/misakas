#!/usr/bin/env python3
"""Validate committed source identity and complete RFC census; not protocol correctness."""
import hashlib,json,subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
def main():
    summary=json.loads((HERE/'summary.json').read_text())
    assert summary['audit_status']=='COMPLETE'
    manifest=json.loads((HERE/'source-manifest.json').read_text())
    assert manifest['baseline']==summary['baseline']
    for row in manifest['files']:
        data=subprocess.check_output(['git','show',summary['baseline']+':'+row['path']],cwd=ROOT)
        assert hashlib.sha256(data).hexdigest()==row['sha256'],row['path']
    cats=summary['categories']
    assert sorted(n for rows in cats.values() for n in rows)==list(range(1,39))
    assert {k:len(v) for k,v in cats.items()}==summary['category_counts']
    assert summary['verified_complete_rfcs']==cats['VERIFIED_COMPLETE']
    assert sorted(cats['VERIFIED_COMPLETE']+cats['CODE_COMPLETE_ACCEPTANCE_PENDING'])==summary['code_complete_rfcs']
    coverage=json.loads((HERE/'coverage.json').read_text())
    assert sorted(r['rfc'] for r in coverage['rfcs'])==list(range(1,39))
    assert sum(r['requirements_rechecked'] for r in coverage['rfcs'])==141
    citations=json.loads((HERE/'citation-resolution.json').read_text())
    assert citations['reference_count']==354 and citations['all_marker_resolved']
    for ref in citations['references']:
        target=ref.get('replacement',ref)
        lines=subprocess.check_output(['git','show',summary['baseline']+':'+target['path']],cwd=ROOT,text=True).splitlines()
        marker=target['marker']
        assert marker is None or any(marker in line for line in lines),(target['path'],marker)
    runtime=json.loads((HERE/'runtime-revalidation.json').read_text())
    assert runtime['unique_passing_tests']==158
    print(json.dumps({'passed':True,'baseline':summary['baseline'],'rfcs':38,'requirements_rechecked':141,'source_files':len(manifest['files']),'code_complete':len(summary['code_complete_rfcs']),'acceptance_complete':len(summary['verified_complete_rfcs'])},ensure_ascii=False,indent=2))
if __name__=='__main__':main()
