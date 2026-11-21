#!/usr/bin/env python3
"""Check QF1 documents and preservation. Never certify Qwen or native runtime."""
from pathlib import Path
from hashlib import sha256
import json
import re
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def digest(p):
    return sha256(p.read_bytes()).hexdigest()


def main():
    source = json.loads((HERE/'source-manifest.json').read_text())
    change = json.loads((HERE/'change-manifest.json').read_text())
    amended = {r['path']:r for r in change['changed_existing_files']}
    errors = []; kept = 0; backups = 0; payloads = 0; books = 0
    for name, old in source['files'].items():
        p = ROOT/name
        if not p.is_file():
            errors.append('Missing input: '+name); continue
        if name in amended:
            r = amended[name]; before = ROOT/r['preserved_before']
            if not before.is_file() or digest(before) != old:
                errors.append('Before mismatch: '+name)
            else:
                backups += 1
                old_bytes=before.read_bytes();new_bytes=p.read_bytes()
                if re.match(r'docs/(?:rfc|adr)-v2/\d{4}-.*\.md$', name):
                    stripped=re.sub(b'\n<!-- QF1-ADOPTED-20261120-BEGIN -->.*?<!-- QF1-ADOPTED-20261120-END -->\n\n',b'',new_bytes,count=1,flags=re.S)
                    if stripped!=old_bytes:
                        errors.append('Prior consolidated book content changed: '+name)
                    else:
                        books += 1
                for marker in ['## 原条文の統合収録','## 原条文の完全収録（履歴）']:
                    m=marker.encode()
                    if m in old_bytes:
                        payloads += 1
                        if m not in new_bytes or old_bytes.split(m,1)[1] != new_bytes.split(m,1)[1]:
                            errors.append('Historical payload changed: '+name)
            if digest(p) != r['after_sha256']:
                errors.append('Undeclared later edit: '+name)
        elif digest(p) != old:
            errors.append('Input changed without record: '+name)
        else:
            kept += 1
    for r in change['new_design_files']:
        p=ROOT/r['path']
        if not p.is_file() or digest(p)!=r['sha256']:
            errors.append('New design differs from manifest: '+r['path'])
    for kind, count in [('rfc',20),('adr',50)]:
        reg=json.loads((ROOT/f'docs/{kind}-v2/registry.json').read_text())
        if len(reg['documents']) != count: errors.append('Book count changed: '+kind)
        if reg['latest_adoption']['new_profile']!='QF1': errors.append('Missing QF1 adoption: '+kind)
        if reg['latest_adoption']['runtime_changed'] or reg['latest_adoption']['activation_changed']:
            errors.append('Runtime/activation falsely changed')
    p=json.loads((ROOT/'docs/design/palw/models/qf1-qwen3-8b-profile.json').read_text())
    for key in ['runtime_enabled','wire_ids_assigned','native_implemented_here','instance_bound',
                'qwen_executed_here','qwen_quality_measured','G_COLL_proved','G_COST_proved']:
        if p[key] is not False: errors.append('Unsupported completion flag: '+key)
    g=p['geometry']
    if [g[k] for k in ['layers','hidden_size','intermediate_size','query_heads','kv_heads','head_dim']] != [36,4096,12288,32,8,128]:
        errors.append('Wrong Qwen geometry')
    if p['readout']['roles_per_decision']!=37 or p['readout']['step_numeric_bytes']!=37*4096*2:
        errors.append('Readout shape mismatch')
    if p['trace']['readout_chunk_bytes']!=4096 or p['trace']['max_compressions_per_court_chunk']!=32:
        errors.append('Hash chunk bound mismatch')
    for group,key in [('generation','final_selected_token_extra_forward'),('readout','direct_context_fields_in_z'),
                      ('readout','hash_feedback_into_model'),('registration','unknown_mining_allowed'),
                      ('registration','bft_resolve_replaces_theorem')]:
        if p[group][key] is not False: errors.append('Unsafe profile flag: '+group+'.'+key)
    for name in change['required_integration_points']:
        t=(ROOT/name).read_text()
        if 'QF1-ADOPTED-20261120-BEGIN' not in t or '0066-qwen3-fow1-execution-and-readout.md' not in t:
            errors.append('Missing current integration: '+name)
    for name in source['files']:
        if '/maintenance/revisions/' in name and name in amended:
            errors.append('Prior revision evidence was edited: '+name)
    result={'passed':not errors,'scope':'document preservation and declared contract only',
            'input_files':len(source['files']),'unchanged_input_files':kept,
            'changed_existing_files':len(amended),'exact_before_backups':backups,
            'unchanged_historical_payloads_in_amended_books':payloads,
            'prior_consolidated_book_bytes_preserved_after_notice_removal':books,
            'new_design_files':len(change['new_design_files']),'errors':errors,
            'not_verified':['Qwen inference','Qwen tokenizer','Qwen quality','G-COLL','G-COST',
                            'native Court','TB1/BFT runtime','activation']}
    print(json.dumps(result,ensure_ascii=False,indent=2))
    return 0 if not errors else 1


if __name__=='__main__':
    raise SystemExit(main())
