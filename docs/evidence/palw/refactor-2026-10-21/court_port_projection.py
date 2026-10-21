"""Recheck production function projections against preserved base 1b83142a2.
Run from the repository root at the court extraction checkpoint. Uses git and rustfmt; does not edit source.
"""
from pathlib import Path
import re,textwrap,subprocess,json,difflib
from court_port_projection_helpers import root, mask, edit, port
# Keep recorded checkpoint evidence intact when invoked at a later, different source revision.
import hashlib
pins = json.loads((Path(__file__).parent / 'court-source-hashes.json').read_text())
for path, expected in pins['sha256'].items():
    if hashlib.sha256((root / path).read_bytes()).hexdigest() != expected:
        raise SystemExit('Source differs from the court checkpoint; check out its commit before rerunning this audit: ' + path)
def funcs(s):
 m=mask(s);out={}
 for x in re.finditer(r'\bfn\s+(\w+)(?:<[^\n]*>)?\s*\(',m):
  d=1;i=x.end()
  while d and i<len(m):d+=(m[i]=='(')-(m[i]==')');i+=1
  sig=m[x.start():i];j=m.find('{',i)
  if j<0 or ';' in m[i:j]:continue
  d=1;k=j+1
  while d and k<len(m):d+=(m[k]=='{')-(m[k]=='}');k+=1
  out.setdefault(x[1],[]).append((s[x.start():k],sig, bool(re.match(r' +',s[s.rfind('\n',0,x.start())+1:x.start()]))))
 return out
def norm(s):
 # Trivia and optional trailing commas have no effect here. Literals were left unchanged by the port.
 return re.sub(r',([\)\]}>])',r'\1',re.sub(r'\s+','',s))
rows=[];detail=[]
for path in ['kaspad/src/palw_artifact_inventory.rs', 'kaspad/src/palw_audit_duty.rs', 'kaspad/src/palw_filer_false_valid.rs', 'kaspad/src/palw_filer_held.rs', 'kaspad/src/palw_filer_held_e2e.rs', 'kaspad/src/palw_filer_replay.rs', 'kaspad/src/palw_flat_source_responder.rs', 'kaspad/src/palw_float_source_responder.rs', 'kaspad/src/palw_fraud_filer.rs', 'kaspad/src/palw_history_recovery_actor.rs', 'kaspad/src/palw_kernel_verifier.rs', 'kaspad/src/palw_operator_da.rs', 'kaspad/src/palw_panel.rs', 'kaspad/src/palw_panel/gdn_filer_tick.rs', 'kaspad/src/palw_panel/gen_court.rs', 'kaspad/src/palw_panel/held_chunks.rs', 'kaspad/src/palw_panel/held_court.rs', 'kaspad/src/palw_panel/held_court_e2e.rs', 'kaspad/src/palw_panel/improve.rs', 'kaspad/src/palw_panel/p2_6_da_accusation_policy.rs', 'kaspad/src/palw_panel/tir_court.rs', 'kaspad/src/palw_public_prefix.rs', 'kaspad/src/palw_reporter_filer.rs', 'kaspad/src/palw_selected_event_responder.rs', 'kaspad/src/palw_tir_prefix_responder.rs', 'kaspad/src/palw_tir_public_custodian.rs', 'kaspad/src/palw_tir_public_filer_scheduler.rs', 'kaspad/src/palw_tir_public_owner.rs']:
 if not path.startswith('kaspad/src/') or not path.endswith('.rs'):continue
 old=funcs(subprocess.check_output(['git','show','1b83142a2:'+path],text=True));new=funcs(Path(path).read_text())
 for name,versions in old.items():
  for idx,(text,sig,_old_indent) in enumerate(versions):
   if name not in new or idx>=len(new[name]):continue
   actual,nsig,indented=new[name][idx]
   if 'CourtOutboxV1' not in nsig and name!='worker':continue
   expected=text
   for raw in ['pending','due','moved']:
    if re.search(r'\b'+raw+r':\s*&',sig):expected=edit(expected,r'\b'+raw+r'\b','court_'+raw)
   expected=port(expected)
   # The original fields below are scalar clock labels, not map variables.
   expected=re.sub(r'court_outbox: (?!&mut CourtOutboxV1)([^\n]+)',r'due: \1',expected) if name in ['tir_public_custodian_tick_v1','tir_public_owner_tick_v1'] else expected
   expected=expected.replace('&mut *books.court_outbox,\n                        &mut *books.court_outbox,','&mut *books.court_outbox,')
   expected=expected.replace('court_outbox.remove(', 'court_outbox.forget_due(')
   expected=expected.replace('court_outbox\n                    .entry((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false))\n                    .and_modify(|held| *held = (*held).min(until_daa.saturating_sub(1)));','court_outbox.tighten_existing_due((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false), until_daa.saturating_sub(1));')
   expected=expected.replace('current_daa: u64,\n        court_outbox: &CourtOutboxV1,\n        deadlines:', 'current_daa: u64,\n        court_outbox: &mut CourtOutboxV1,\n        deadlines:') if name=='date_queued_demands_v1' else expected
   form=('impl Placeholder {\n'+expected+'\n}') if indented else expected
   formatted=subprocess.run(['rustfmt','--edition','2024','--emit','stdout','--config-path',str(root/'.rustfmt.toml')],input=form,text=True,capture_output=True,check=True).stdout
   expected=funcs(formatted)[name][0][0]
   # Ignore comments, retain strings for explicit runtime-message equality.
   comment=r'//[^\n]*|/\*[\s\S]*?\*/'
   ee=norm(re.sub(comment,'',expected));aa=norm(re.sub(comment,'',actual))
   same=ee==aa;rows.append({'path':path,'function':name,'projected_source_matches':same})
   if not same:
    z=next((i for i,(a,b) in enumerate(zip(ee,aa)) if a!=b),min(len(ee),len(aa)))
    detail.append(path+':'+name+'\nexpected '+ee[max(0,z-70):z+180]+'\nactual   '+aa[max(0,z-70):z+180])
(Path(__file__).parent/'court-port-projection-differences.txt').write_text('\n\n'.join(detail)+'\n')
(Path(__file__).parent/'court-port-projection.json').write_text(json.dumps({'purpose':'Mechanical source projection audit; scoped to outbox-facing function bodies, not an exhaustive runtime equivalence proof. The projection preserves legacy ordering and API operations; optional commas and formatting are ignored.','functions':rows},indent=2)+'\n')
print('projected',sum(r['projected_source_matches'] for r in rows),'/',len(rows));print('\n'.join(detail[:5]))
