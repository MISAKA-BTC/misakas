#!/usr/bin/env python3
"""Validate MAS1 document preservation and contracts, not model or runtime safety."""
from pathlib import Path
from hashlib import sha256
import json
import re
import sys

sys.dont_write_bytecode=True
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def digest(p):
    return sha256(p.read_bytes()).hexdigest()

def main():
    source=json.loads((HERE/'source-manifest.json').read_text())
    changes=json.loads((HERE/'change-manifest.json').read_text())
    updated={r['path']:r for r in changes['changed_existing_files']}
    errors=[];kept=0;backups=0;payloads=0
    for name,old_hash in source['files'].items():
        p=ROOT/name
        if not p.is_file():
            errors.append('Input file missing: '+name);continue
        if name in updated:
            r=updated[name];b=ROOT/r['preserved_before']
            if not b.is_file() or digest(b)!=old_hash:errors.append('Before bytes not preserved: '+name)
            else:backups+=1
            if digest(p)!=r['after_sha256']:errors.append('Changed file differs from manifest: '+name)
            if b.is_file():
                for mark in ['## 原条文の統合収録','## 原条文の完全収録（履歴）']:
                    a=b.read_bytes();now=p.read_bytes();m=mark.encode()
                    if m in a:
                        payloads+=1
                        if m not in now or a.split(m,1)[1]!=now.split(m,1)[1]:errors.append('Historical payload changed: '+name)
        elif digest(p)!=old_hash:errors.append('Undeclared change: '+name)
        else:kept+=1
    for r in changes['new_design_files']:
        p=ROOT/r['path']
        if not p.is_file() or digest(p)!=r['sha256']:errors.append('New design file differs: '+r['path'])
    for kind,count in [('rfc',20),('adr',50)]:
        reg=json.loads((ROOT/f'docs/{kind}-v2/registry.json').read_text())
        if len(reg['documents'])!=count:errors.append('Consolidated count changed: '+kind)
        latest=reg['latest_adoption']
        if latest['new_profile']!='MAS1' or latest['runtime_changed'] or latest['activation_changed']:errors.append('Incorrect latest adoption: '+kind)
    p=json.loads((ROOT/'docs/design/palw/models/mas1-admission-policy.json').read_text())
    false_fields=['runtime_enabled','wire_ids_assigned','native_implemented_here','qwen_cost_theorem_proved','automatic_shortcut_detection_complete']
    for key in false_fields:
        if p[key] is not False:errors.append('Unsupported completion claim: '+key)
    for path in [('unknown_policy','new_model_mining_allowed'),('unknown_policy','bft_timeout_auto_pass'),('bft','unknown_to_pass_operation'),('bft','ledger_finality_authority'),('bft','fork_choice_authority'),('release','burn_clears_security_stop'),('release','model_resolution_clears_rule_version_stop'),('release','generation_change_clears_stop_or_open_case')]:
        if p[path[0]][path[1]] is not False:errors.append('Unsafe document flag: '+'.'.join(path))
    if p['bft']['n']!='3*f+1' or p['bft']['q']!='2*f+1' or p['bft']['minimum_f']!=1:errors.append('BFT contract mismatch')
    if not {'G_EXEC','G_COLL','G_COST','G_USE'}.issubset(p['gates']):errors.append('Missing gate')
    t=json.loads((ROOT/'docs/design/palw/consensus/tb1-beacon-consumers.json').read_text())
    cs={c['consumer']:c for c in t['consumers']}
    if cs['MODEL_PRACTICAL_EVALUATION']['purpose']!='MODEL_AUDIT' or cs['MODEL_EVALUATION_PANEL']['purpose']!='PANEL':errors.append('Evaluation purpose mismatch')
    if cs['MODEL_SAFETY_STOP']['waits_for_new_beacon'] is not False:errors.append('Stop waits for new entropy')
    # Check the authored integration points without treating this as native execution.
    for name in changes['required_integration_points']:
        txt=(ROOT/name).read_text()
        if 'MAS1-ADOPTED-20261120-BEGIN' not in txt or '0065-model-admission-screening-and-bft-stop.md' not in txt:errors.append('Missing integration notice: '+name)
    result={'passed':not errors,'scope':'document preservation and declared contract only','input_files':len(source['files']),'unchanged_input_files':kept,'changed_existing_files':len(updated),'exact_before_backups':backups,'unchanged_historical_payloads_in_amended_books':payloads,'new_design_files':len(changes['new_design_files']),'errors':errors,'not_verified':['automatic model analysis','Qwen cost theorem','native state machine','live BFT','LLM usefulness measurements','activation']}
    print(json.dumps(result,ensure_ascii=False,indent=2))
    return 0 if not errors else 1

if __name__=='__main__':raise SystemExit(main())
