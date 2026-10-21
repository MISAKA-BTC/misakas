from pathlib import Path
import subprocess,re,importlib.util,json
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
base='8b5d3b723';paths=['consensus/src/pipeline/virtual_processor/processor/palw_accept.rs','consensus/src/pipeline/virtual_processor/processor/palw.rs','consensus/src/pipeline/virtual_processor/processor.rs','consensus/core/src/palw_state_v2/tests.rs'];projected=[];owned=[]
for path in paths:
 s=Path(path).read_text();before=subprocess.check_output(['git','show',base+':'+path],text=True);m=h.mask(s)
 for x in re.finditer(r'\.into_legacy_extras\(\)',m):owned.append({'file':path,'line':s[:x.start()].count('\n')+1})
 if path.endswith('palw_accept.rs'):
  a=s.index('/// Immutable output of the existing production resolver');b=s.index('impl VirtualStateProcessor {',a);decl=s[a:b]
  assert 'impl std::ops::DerefMut' not in decl
  assert 'Self { point: *point, extras }' in decl
  assert 'std::fmt::Debug::fmt(&self.extras, f)' in decl
  assert 'fn deref(&self) -> &Self::Target { &self.extras }' in decl
  assert '        self.extras\n' in decl
  assert s[b:].count('PalwResolvedBlockInputsV1::at(point, extras)')==2
  s=s[:a]+s[b:]
  s=s.replace('-> PalwResolvedBlockInputsV1 {','-> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {').replace('        PalwResolvedBlockInputsV1::at(point, extras)\n','        extras\n')
 s=s.replace('.into_legacy_extras()','').replace('extras.point().daa_score','point.daa_score')
 assert s==before,path
 projected.append(path)
assert len(owned)==13
p='consensus/core/src/palw_backend/operations.rs';old=subprocess.check_output(['git','show',base+':'+p],text=True);assert Path(p).read_text()==old.rstrip()+'\n'
report={'base':base,'production_and_guard_files_exact_after_inverse_boundary_substitution':projected,'explicit_owned_legacy_sites':owned,'canonical_resolver_constructions':2,'new_parallel_resolver':False,'wrapper_contract':'Only the existing resolver and its object overlay construct the private, point-bound, immutable wrapper. Deref reads legacy extras; no DerefMut. Owned legacy extraction is explicit. Debug delegates to the legacy value; no new implicit equality is introduced.','format_only_followup':'Trimmed the final empty line in backend/operations.rs; old backend evidence/pins remain unchanged.','scope':'Targeted source/data-flow projection. Native boundary tests and crate type checks are separate; not exhaustive execution equivalence.'}
Path('/tmp/palw-resolved-inputs-source-audit.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
