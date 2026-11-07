from pathlib import Path
import importlib.util,re,json
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'kaspad/src/palw_panel.rs';s=p.read_text();m=h.mask(s);start=m.index('pub async fn worker(');a=m.index('{',start)
def close(m,a,left='(',right=')'):
 d=1;i=a+1
 while d:d+=(m[i]==left)-(m[i]==right);i+=1
 return i
end=close(m,a,'{','}');worker=s[start:end];wm=h.mask(worker)
init_a=wm.index('        let mut materials:');seq=wm.index('let mut pool_arrival_seq:',init_a);init_b=wm.index(';',seq)+1;initial=worker[init_a:init_b];Path('/tmp/material-pool-original-initialization.rs').write_text(initial)
while_a=wm.index('while materials.len() > PANEL_POOL_MAX_CLAIMS');while_brace=wm.index('{',while_a);while_b=close(wm,while_brace,'{','}');eviction=worker[while_a:while_b];Path('/tmp/material-pool-original-eviction.rs').write_text(eviction)
# Keep map construction order exactly materials -> intervals -> bytes -> arrivals -> sequence.
new='''//! Node-local received material and interval owners. The existing admission,
//! retention and resume algorithms stay authoritative; these owners keep the
//! material byte counter and arrival index together and expose no raw material writer.
use super::*;

pub(super) struct PoolBooksV1 {
    pub(super) material: MaterialPoolV1,
    pub(super) intervals: IntervalOpeningPoolV1,
}
pub(super) struct MaterialPoolV1 {
    values: HashMap<Hash64, Vec<Vec<u8>>>,
    bytes: usize,
    arrival: HashMap<Hash64, u64>,
    sequence: u64,
}
pub(super) struct IntervalOpeningPoolV1 {
    values: HashMap<(Hash64, u32), Vec<Vec<u8>>>,
}
impl PoolBooksV1 {
    pub(super) fn new() -> Self {
'''+initial.replace('let mut ','let ')+'''
        Self {
            material: MaterialPoolV1 { values: materials, bytes: pool_bytes, arrival: pool_arrival, sequence: pool_arrival_seq },
            intervals: IntervalOpeningPoolV1 { values: interval_openings },
        }
    }
}
impl MaterialPoolV1 {
    pub(super) fn map(&self) -> &HashMap<Hash64, Vec<Vec<u8>>> { &self.values }
    pub(super) fn admit(&mut self, claim: Hash64, bytes: Vec<u8>) {
        pool_admit_material_v1(&mut self.values, &mut self.arrival, &mut self.sequence, &mut self.bytes, claim, bytes);
    }
    pub(super) fn sweep(&mut self, keep: impl Fn(&Hash64) -> bool) {
        pool_sweep_material_v1(&mut self.values, &mut self.arrival, &mut self.bytes, keep);
    }
    /// Only the gossip arrival path applied this whole-pool eviction. Retained and
    /// remade captures keep their original admission path; no new eviction policy.
    pub(super) fn evict_after_gossip(&mut self, claim: Hash64, first_seen: &mut HashMap<Hash64, u64>) {
'''
em=h.mask(eviction);edits=[]
for x in re.finditer(r'\b(materials|pool_bytes|pool_arrival)\b',em):edits.append((x.start(),x.end(),{'materials':'self.values','pool_bytes':'self.bytes','pool_arrival':'self.arrival'}[x[1]]))
for aa,bb,t in reversed(edits):eviction=eviction[:aa]+t+eviction[bb:]
new+=eviction+'''
    }
}
impl IntervalOpeningPoolV1 {
    pub(super) fn map(&self) -> &HashMap<(Hash64, u32), Vec<Vec<u8>>> { &self.values }
    /// Existing segment and public-court consumers may discard unusable openings.
    /// Separate ownership keeps that borrow disjoint from the material reader.
    pub(super) fn resume_mut(&mut self) -> &mut HashMap<(Hash64, u32), Vec<Vec<u8>>> { &mut self.values }
    pub(super) fn admit(&mut self, claim: Hash64, interval: u32, bytes: Vec<u8>, pairs: usize) -> bool {
        PalwPanelService::pool_interval_opening(&mut self.values, claim, interval, bytes, pairs)
    }
}
'''
(r/'kaspad/src/palw_panel/material_pool.rs').write_text(new)
# Replace correlated writer calls before the remaining read substitutions.
edits=[(init_a,init_b,'        let mut pool_books = PoolBooksV1::new();'),(while_a,while_b,'pool_books.material.evict_after_gossip(claim, &mut first_seen);')]
for x in re.finditer(r'\b(pool_admit_material_v1|pool_sweep_material_v1|Self::pool_interval_opening)\(',wm):
 op=wm.index('(',x.start());ce=close(wm,op);depth=0;last=op+1;parts=[]
 for i in range(op+1,ce-1):
  c=wm[i]
  if c in '([{':depth+=1
  elif c in ')]}':depth-=1
  elif c==',' and depth==0:parts.append(worker[last:i].strip());last=i+1
 tail=worker[last:ce-1].strip()
 if tail:parts.append(tail)
 if x[1]=='pool_admit_material_v1':assert len(parts)==6;call='pool_books.material.admit('+', '.join(parts[4:])+')'
 elif x[1]=='pool_sweep_material_v1':assert len(parts)==4;call='pool_books.material.sweep('+parts[3]+')'
 else:assert len(parts)==5;call='pool_books.intervals.admit('+', '.join(parts[1:])+')'
 edits.append((x.start(),ce,call))
for aa,bb,t in sorted(edits,reverse=True):worker=worker[:aa]+t+worker[bb:]
# Convert only free worker bindings, not struct field labels or other objects' fields.
wm=h.mask(worker);edits=[]
for x in re.finditer(r'\b(materials|interval_openings)\b',wm):
 name=x[1];a=x.start();b=x.end();pre=wm[:a];post=wm[b:]
 if re.match(r'\s*:',post) or re.search(r'(?:\.|::)\s*$',pre):continue
 ref=re.search(r'&\s*(mut\s+)?$',pre)
 if ref:
  aa=ref.start();mutable=ref[1] is not None
  assert not(mutable and name=='materials'),'unconverted material writer'
  t='pool_books.material.map()' if name=='materials' else 'pool_books.intervals.'+('resume_mut()' if mutable else 'map()');edits.append((aa,b,t))
 else:
  mutate=name=='interval_openings' and bool(re.match(r'\s*\.\s*(retain|remove|insert|entry|get_mut|clear)\b',post))
  t='pool_books.material.map()' if name=='materials' else 'pool_books.intervals.'+('resume_mut()' if mutate else 'map()');edits.append((a,b,t))
for aa,bb,t in sorted(edits,reverse=True):worker=worker[:aa]+t+worker[bb:]
s=s[:start]+worker+s[end:];needle='mod carrier_chain;';assert s.count(needle)==1;s=s.replace(needle,'mod material_pool;\nuse material_pool::PoolBooksV1;\n\n'+needle);p.write_text(s)
Path('/tmp/material-pool-port-metadata.json').write_text(json.dumps({'base':'fd3c74aef','correlated_writers':5,'gossip_global_eviction_only':True,'initialization_fragment':'material-pool-original-initialization.rs','eviction_fragment':'material-pool-original-eviction.rs'},indent=2)+'\n')
print('installed private pool owners and worker reader/writer substitutions')
