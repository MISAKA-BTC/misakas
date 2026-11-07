#!/usr/bin/env python3
"""Offline no-new-regressions documentation checks; --strict also fails on baseline issues."""
from __future__ import annotations
import sys
sys.dont_write_bytecode=True
import argparse,json,re
from pathlib import Path
from collections import Counter,defaultdict
from doclib import scan,ERRORS,key

def main():
 a=argparse.ArgumentParser(description=__doc__);a.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[2]);a.add_argument('--strict',action='store_true');a.add_argument('--json-out',type=Path);args=a.parse_args()
 root=args.root.resolve();d=root/'docs';fail=[]
 if not d.is_dir():a.error('root must contain docs/')
 baseline=json.loads((d/'maintenance/known-link-issues.json').read_text());reg=json.loads((d/'maintenance/number-registry.json').read_text())
 allowed=Counter({r['key']:r['count'] for r in baseline['issues']});rows=scan(root);actual=Counter(key(r) for r in rows if r['status'] in ERRORS);unexpected=actual-allowed
 for k,n in unexpected.items():fail.append(f'new link issue x{n}: {k}')
 if args.strict and actual:fail.append(f'strict: {sum(actual.values())} retained missing targets')
 catalog=json.loads((d/'maintenance/decision-catalog.json').read_text())['records'];counts={}
 for kind in ['adr','rfc']:
  groups=defaultdict(list)
  for p in sorted((d/kind).glob('*.md')):
   if re.match(r'^\d{4}-',p.name):groups[p.name[:4]].append(p.relative_to(root).as_posix())
  for n,paths in groups.items():
   if len(paths)>1 and sorted(paths)!=sorted(reg['legacy_duplicates'].get(kind,{}).get(n,[])):fail.append(f'new {kind} collision {n}: {paths}')
  found={v for arr in groups.values() for v in arr};indexed={r['path'] for r in catalog if r['kind']==kind}
  if found!=indexed:fail.append(kind+' index is stale; run generate_indexes.py')
  counts[kind]=len(found)
 for p in d.iterdir():
  if p.is_file() and p.name not in {'README.md','INDEX.md','STATUS.md','AGENTS.md','PRINCIPLES.md'}:fail.append('unclassified root file: '+p.name)
 required=['README.md','INDEX.md','STATUS.md','AGENTS.md','PRINCIPLES.md','governance/design-freeze.md','governance/post-mainnet-scope.md','governance/parameter-register.md','adr/0181-design-freeze-and-controlled-change.md','adr/0182-defer-media-and-model-improvement-until-post-mainnet.md','rfc/0021-design-freeze-and-documentation-governance.md','rfc/0022-post-mainnet-media-and-model-improvement-scope.md']
 for name in required:
  if not (d/name).is_file():fail.append('missing required document: '+name)
 params=json.loads((d/'governance/parameters.json').read_text())['parameters'];seen=set()
 for r in params:
  if r['key'] in seen:fail.append('duplicate parameter key: '+r['key'])
  seen.add(r['key'])
  for field in ['key','symbol','baseline','unit','approval_state','guard','source']:
   if not r.get(field):fail.append(f'incomplete parameter {r.get("key")}: {field}')
  if not (root/r['source']).is_file():fail.append('missing parameter source: '+r['source'])
 # Check the documented current release policy, not actual node configuration.
 scope=json.loads((d/'governance/release-scope.json').read_text());features=scope['initial_mainnet_excluded']
 expected={'rfc0003.image-paths','rfc0003.video-paths','rfc0004.model-improvement-protocol'}
 if {r['feature'] for r in features}!=expected:fail.append('release-scope feature set changed; explicit policy review required')
 if any(r['enabled_by_policy'] is not False or r['policy']!='DEFERRED_POST_MAINNET' for r in features):fail.append('deferred feature enabled in policy; reviewed reopening required')
 if scope['automatic_activation_after_mainnet'] is not False:fail.append('automatic post-mainnet activation is prohibited by current policy')
 for name in ['rfc/0003-palw-generative-model-classes.md','rfc/0004-palw-model-improvement.md','spec/palw/17-model-improvement.md']:
  t=(d/name).read_text()[:3000]
  if 'mainnet 後へ延期・当面無効' not in t or 'post-mainnet-scope.md' not in t:fail.append('missing front-door deferral annotation: '+name)
 result={'mode':'strict' if args.strict else 'no-new-regressions','passed':not fail,'markdown_files':len(list(d.rglob('*.md'))),'targets':dict(Counter(r['status'] for r in rows)),'primary_documents':counts,'known_issue_occurrences':sum((actual&allowed).values()),'new_issue_occurrences':sum(unexpected.values()),'failures':fail,'limitations':['Offline, limited Markdown parser; not a full CommonMark renderer.','External URLs, code outside docs, protocol semantics and actual runtime/activation are not verified.','Release-scope checks validate documentation only, not node disablement.']}
 if args.json_out:args.json_out.parent.mkdir(parents=True,exist_ok=True);args.json_out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
 print(('PASS' if not fail else 'FAIL')+' — '+result['mode']);print(json.dumps({k:v for k,v in result.items() if k not in {'failures','limitations'}},ensure_ascii=False,indent=2))
 for f in fail[:30]:print('  '+f)
 if result['known_issue_occurrences']:print('NOTE: baseline missing references remain; PASS is not a clean strict audit.')
 return 0 if not fail else 1
if __name__=='__main__':raise SystemExit(main())
