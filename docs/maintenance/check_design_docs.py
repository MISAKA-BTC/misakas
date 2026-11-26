#!/usr/bin/env python3
"""Check RFC20 design / ADR20 rationale, links and policy. No runtime checks."""
import sys
sys.dont_write_bytecode=True
from pathlib import Path
import re,json,argparse
from doclib import scan,ERRORS

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[2])
    ap.add_argument('--json-out',type=Path)
    a=ap.parse_args();root=a.root.resolve();d=root/'docs';errors=[]
    files=sorted((d/'rfc').glob('[0-9][0-9][0-9][0-9]-*.md'))
    if [p.name[:4] for p in files]!=[f'{i:04d}' for i in range(1,21)]:errors.append('expected exactly RFC0001..0020 with no duplicate')
    mp=json.loads((d/'maintenance/rfc-map.json').read_text())
    if len(mp['records'])!=71:errors.append('old-to-current map must cover all 71 source files')
    for row in mp['records']:
        if (root/row['old_path']).exists():errors.append('old RFC body retained: '+row['old_path'])
        for p in row['current_paths']:
            if not (root/p).is_file():errors.append('missing current target: '+p)
    for rel in ['docs/history','docs/archive','docs/maintenance/revisions','docs/rfc-v1','docs/rfc-v2','docs/adr-v1','docs/adr-v2']:
        if (root/rel).exists():errors.append('forbidden old-body/version tree: '+rel)
    for kind in ('rfc', 'adr'):
        if any(p.is_dir() for p in (d/kind).iterdir()):
            errors.append('unexpected old attachment/history directory: '+kind)
    adrs=sorted((d/'adr').glob('[0-9][0-9][0-9][0-9]-*.md'))
    if [p.name[:4] for p in adrs]!=[f'{i:04d}' for i in range(1,21)]:errors.append('expected exactly ADR0001..0020 with no duplicate')
    am=json.loads((d/'maintenance/adr-map.json').read_text())
    if len(am['records'])!=235:errors.append('ADR source map must cover 235 full paths')
    if len({r['old_path'] for r in am['records']})!=235:errors.append('duplicate old ADR map entry')
    for row in am['records']:
        if (root/row['old_path']).exists():errors.append('old ADR body retained: '+row['old_path'])
        for p in [row['primary_adr']]+row['related_adrs']+row['design_rfcs']:
            if not (root/p).is_file():errors.append('missing ADR mapping destination: '+p)
    for rp,ap in zip(files,adrs):
        rt=rp.read_text();at=ap.read_text()
        if '../adr/'+ap.name not in rt:errors.append('RFC lacks matching ADR: '+rp.name)
        if '../rfc/'+rp.name not in at:errors.append('ADR lacks matching RFC: '+ap.name)
        for h in ['## 背景','## 判断理由','## 比較した方針','## 帰結']:
            if h not in at:errors.append('ADR lacks rationale section: '+ap.name+' '+h)
        if not at.startswith('# ADR-'+ap.name[:4]+' —'):errors.append('ADR header mismatch: '+ap.name)
        if '```' in at or '\\[' in at:errors.append('detailed algorithm/formula block in ADR: '+ap.name)
        if len(at)<600:errors.append('ADR lacks substantive rationale: '+ap.name)
    transfers=json.loads((d/'maintenance/content-responsibility-map.json').read_text())['explicit_transfers']
    if not {'ADR_TO_RFC','RFC_TO_ADR'}.issubset({r['direction'] for r in transfers}):errors.append('one migration direction missing')
    current_prefixes=('docs/rfc/','docs/adr/','docs/governance/')
    current_exact={'docs/README.md','docs/INDEX.md','docs/STATUS.md','docs/AGENTS.md','docs/PRINCIPLES.md',
       'docs/maintenance/README.md','docs/maintenance/numbering.md','docs/adr/README.md','docs/adr/INDEX.md',
       'docs/adr/TOPICS.md',
       'docs/spec/README.md','docs/design/README.md','docs/design/palw/README.md',
       'docs/spec/palw/model-tiers.md','docs/spec/palw/model-allocation.md','docs/spec/palw/claim-certification.md',
       'docs/design/palw/models/README.md','docs/design/palw/models/versioned-kernels.md',
       'docs/design/palw/models/beyond-weight-files.md','docs/design/palw/models/qf1/README.md',
       'docs/design/palw/models/qf1/trace-schema.md'}
    link_errors=[r for r in scan(root) if r['status'] in ERRORS and (r['source'].startswith(current_prefixes) or r['source'] in current_exact)]
    errors.extend('current link: '+r['source']+' -> '+r['target']+'#'+r['fragment'] for r in link_errors)
    policies={name:json.loads((d/'design/palw'/path).read_text()) for name,path in {
      'admission':'models/mad2-admission-policy.json','qf1':'models/qf1-qwen3-8b-profile.json',
      'moe3':'economics/moe3-economic-policy.json','assessment':'models/mas1-admission-policy.json'}.items()}
    if policies['admission']['routes']!=['BFT_DECISION']:errors.append('admission has another path')
    if policies['qf1']['registration']['admission_basis']!=['BFT_DECISION']:errors.append('QF1 has another path')
    if policies['moe3']['model_governance']['admission_routes']!=['BFT_DECISION']:errors.append('MOE3 has another path')
    if policies['assessment']['creates_model_admission'] is not False:errors.append('assessment creates admission')
    for x in ['mathematical_proof_required','cost_measurement_evidence_required','utility_evaluation_evidence_required','hard_technical_failure_may_be_overridden','ledger_finality_authority']:
        if policies['admission']['bft'][x] is not False:errors.append('wrong BFT boundary: '+x)
    if not policies['admission']['control']['all_models_bft_managed']:errors.append('model management exemption')
    if policies['admission']['assurance']['claims_bypass_panel_court_or_gate_c'] is not False:errors.append('claim bypass')
    proposed=(d/'rfc/0013-model-work-accounting.md').read_text()
    if 'PROPOSED_DESIGN_NOT_ACTIVATED' not in proposed:errors.append('MWA1 status incorrectly promoted')
    storage=(d/'rfc/0018-storage-pruning-and-ibd.md').read_text()
    if '認証付き行アクセス【提案】' not in storage:errors.append('state map status incorrectly promoted')
    budget=(d/'rfc/0012-bonds-budgets-and-slashing.md').read_text()
    if 'account単位の最低担保【提案】' not in budget:errors.append('account-bond status incorrectly promoted')
    if 'L >= L_coll >= gain' not in budget:errors.append('budget design transfer missing')
    court=(d/'rfc/0009-court-and-claim-final.md').read_text()
    if '直列witness filingの時計【提案】' not in court:errors.append('filing-clock status incorrectly promoted')
    for p in files:
        t=p.read_text()
        if not t.startswith('# RFC-'+p.name[:4]+' —'):errors.append('header number mismatch: '+p.name)
        if len(t)<1000:errors.append('RFC lacks substantive body: '+p.name)
        if t.count('```')%2:errors.append('unbalanced code fences: '+p.name)
    result={'passed':not errors,'rfc_count':len(files),'adr_count':len(adrs),'source_rfc_map_count':len(mp['records']),'source_adr_map_count':len(am['records']),
      'current_link_errors':len(link_errors),'policy_routes':['BFT_DECISION'],'runtime_verified':False,
      'failures':errors,'scope':'Current document structure, local links, policy boundaries and proposal status only.'}
    if a.json_out:a.json_out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(result,ensure_ascii=False,indent=2));return 1 if errors else 0
if __name__=='__main__':raise SystemExit(main())
