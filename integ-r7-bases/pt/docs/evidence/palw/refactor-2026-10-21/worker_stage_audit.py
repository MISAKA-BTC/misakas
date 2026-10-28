from pathlib import Path
import re, importlib.util,json,hashlib,subprocess
r=Path(__file__).resolve().parents[4];fragments=Path(__file__).resolve().parent/'worker-projection/fragments';sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
def close(s,a):
 m=h.mask(s);d=1;i=a+1
 while d:d+=(m[i]=='{')-(m[i]=='}');i+=1
 return i

def norm(s):
 # Preserve literals exactly; remove comments/whitespace and formatter-added trailing call commas.
 out=[];i=0
 while i<len(s):
  if s[i].isspace():i+=1;continue
  if s.startswith('//',i):
   z=s.find('\n',i);i=len(s) if z<0 else z;continue
  if s.startswith('/*',i):
   d=1;i+=2
   while d:
    if s.startswith('/*',i):d+=1;i+=2
    elif s.startswith('*/',i):d-=1;i+=2
    else:i+=1
   continue
  raw=re.match(r'(?:b)?r(#+)?"',s[i:])
  if raw:
   end='"'+(raw[1] or '');z=s.index(end,i+raw.end())+len(end);out.append(s[i:z]);i=z;continue
  if s[i]=='"':
   z=i+1
   while z<len(s):
    if s[z]=='\\':z+=2
    elif s[z]=='"':z+=1;break
    else:z+=1
   out.append(re.sub(r'\\\n[ \t]*', '', s[i:z]));i=z;continue
  out.append(s[i]);i+=1
 # rustfmt may add trailing commas at call/struct boundaries. This normalized audit
 # alone does not prove Rust semantics; type/runtime checks are separate.
 return re.sub(r',(?=[)}\]])','', ''.join(out))

def fnbody(file,name):
 s=(r/'kaspad/src/palw_panel'/file).read_text();a=h.mask(s).index('fn '+name+'(');a=h.mask(s).index('{',a);b=close(s,a);return s[a+1:b-1]
checks={}
def eq(n,a,b):
 def fmt(t):
  proc=subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input='async fn projected() {\n'+t+'\n}',text=True,capture_output=True)
  if proc.returncode:raise RuntimeError(proc.stderr)
  return proc.stdout
 aa,bb=norm(fmt(a)),norm(fmt(b));ok=aa==bb;checks[n]=ok
 if not ok:
  import difflib
  Path(__file__).with_name('worker-audit-'+n+'.diff').write_text('\n'.join(difflib.unified_diff(aa.split(';'),bb.split(';'))));print('DIFFER',n)
# Bodies copied verbatim except bindings that were explicitly changed to borrowed inputs.
x=fnbody('court_progress.rs','court_progress_tick_v1');a=x.index('// --- the court');b=x.rindex('CourtProgressResultV1 {');eq('court',(fragments/'court-progress-original-stage.rs').read_text(),x[a:b])
x=fnbody('seat_progress.rs','seat_progress_tick_v1');a=h.mask(x).index('for index in order');x=x[a:];x=h.edit(x,r'\*vertex_equivocated\b','vertex_equivocated');eq('seat',(fragments/'seat-progress-original-stage.rs').read_text(),x)
x=fnbody('collector.rs','collector_tick_v1');a=h.mask(x).index('if self.config.fee_outpoint.is_some()');x=x[a:]
x=x.replace('PalwCarrierSlotsV1::new(*last_lane)', 'PalwCarrierSlotsV1::new(last_lane)')
for n in ['held_before','readiness_waiting','last_lane','canonical_last_daa']:x=h.edit(x,r'\(\*'+n+r'\)',n);x=h.edit(x,r'\*'+n+r'\b',n)
x=x.replace('hash_one(licence_spread, claim)','hash_one(&licence_spread, claim)');eq('collector',(fragments/'collector-original-stage.rs').read_text(),x)
x=fnbody('retirement.rs','retire_tick_v1');x=x[x.index('// Forget what the chain'):];eq('retirement',(fragments/'retirement-original-stage.rs').read_text(),x)
x=fnbody('ingress.rs','drain_panel_inbox_v1');x=x[x.index('while let Ok(event)'):];x=x.replace('evict_after_gossip(claim, first_seen)','evict_after_gossip(claim, &mut first_seen)').replace('shard_arrivals,','&mut shard_books.arrivals,').replace('receipt_arrivals,','&mut receipt_arrivals,');eq('ingress',(fragments/'panel-ingress-before-extraction.rs').read_text(),x)
x=fnbody('startup.rs','install_material_cache_service_v1');eq('startup',(fragments/'panel-startup-before-extraction.rs').read_text(),x)
x=fnbody('registration.rs','advance_v1');a=h.mask(x).index('if let Some((txid, sent_daa))');x=x[a:];x=x[:x.rindex('false')]
mp={'self.object':'class_registration','self.inflight':'class_registration_inflight','self.done':'class_registration_done','self.left_pool':'class_registration_left_pool','self.drops':'class_registration_drops','self.known_id':'class_registration_known_id'}
# Explicitly added fmt argument reproduces the old implicitly captured local value.
x=re.sub(r',\s*class_registration_drops = self.drops\s*\)', '\n)',x)
for aa,bb in mp.items():x=h.edit(x,re.escape(aa)+r'\b',bb)
x=h.edit(x,r'\bservice\b','self');x=x.replace('return true;', 'continue;');eq('registration',(fragments/'class-registration-original-advance.rs').read_text(),x)
# The old low-level admission and interval policy remain byte-unchanged in the parent.
base=subprocess.check_output(['git','show','fd3c74aef:kaspad/src/palw_panel.rs'],text=True)
cur=(r/'kaspad/src/palw_panel.rs').read_text()
for name in ['pool_admit_material_v1','pool_sweep_material_v1','pool_interval_opening']:
 def body(src):
  a=h.mask(src).index('fn '+name+'(');b=h.mask(src).index('{',a);return src[a:close(src,b)]
 eq(name,body(base),body(cur))
# Correlated material owner construction and the gossip-only global eviction.
x=fnbody('material_pool.rs','new');x=x[:x.index('Self {')]
eq('pool-construction-order',(fragments/'material-pool-original-initialization.rs').read_text().replace('let mut ', 'let '),x)
x=fnbody('material_pool.rs','evict_after_gossip')
for old,new in [('self.values','materials'),('self.bytes','pool_bytes'),('self.arrival','pool_arrival')]:x=h.edit(x,re.escape(old)+r'\b',new)
eq('gossip-eviction',(fragments/'material-pool-original-eviction.rs').read_text(),x)
out={'base':'fd3c74aef','checks':checks,'all_passed':all(checks.values()),'scope':'Normalized moved-stage bodies, explicit borrowed scalar inversions, literal values preserved (escaped line indentation normalized). Both fragments formatted identically; comments/whitespace/trailing formatter commas ignored. This is targeted structural evidence, not exhaustive execution equivalence.'}
Path(__file__).with_name('worker-stage-audit.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2))
