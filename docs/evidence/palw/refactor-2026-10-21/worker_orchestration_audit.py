from pathlib import Path
import subprocess,tempfile,shutil,json,hashlib
r=Path(__file__).resolve().parents[4];t=Path(tempfile.mkdtemp(prefix='palw-worker-projection-'));(t/'kaspad/src/palw_panel').mkdir(parents=True);h=Path('docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');(t/h).parent.mkdir(parents=True);shutil.copyfile(r/h,t/h)
old=subprocess.check_output(['git','show','fd3c74aef:kaspad/src/palw_panel.rs']);(t/'kaspad/src/palw_panel.rs').write_bytes(old)
scripts=['port_material_pool.py','extract_panel_ingress_startup.py','extract_class_registration_actor.py','extract_court_progress.py','extract_seat_progress.py','analyze_tail.py','extract_collector_progress.py']
scratch=t/'projection-scratch';scratch.mkdir()
generators=Path(__file__).resolve().parent/'worker-projection/generators'
for path in generators.glob('*.py'):
 text=path.read_text().replace('/tmp/',str(scratch)+'/')
 (scratch/path.name).write_text(text)
for n in scripts:subprocess.run(['python3',str(scratch/n)],cwd=t,check=True,capture_output=True)
p=t/'kaspad/src/palw_panel/collector.rs';s=p.read_text().replace('let bond_key = tick.bond();','let bond_key = tick.bond();\n        let bond = bond_key.0;').replace('hash_one(&licence_spread, claim)','hash_one(licence_spread, claim)').replace('PalwCarrierSlotsV1::new((*last_lane))','PalwCarrierSlotsV1::new(*last_lane)');p.write_text(s)
subprocess.run(['python3',str(scratch/'extract_retirement.py')],cwd=t,check=True,capture_output=True)
p=t/'kaspad/src/palw_panel/retirement.rs';s=p.read_text().replace("    pub(super) challenged: &'a mut HashSet<Hash64>,\n",'');a=s.index('let RetirementRefsV1');s=s[:a]+s[a:].replace('            challenged,\n','',1);p.write_text(s)
p=t/'kaspad/src/palw_panel.rs';s=p.read_text();a=s.index('retirement::RetirementRefsV1');s=s[:a]+s[a:].replace('                    challenged: &mut challenged,\n','',1);p.write_text(s)
actual=(r/'kaspad/src/palw_panel.rs').read_bytes();expected=p.read_bytes()
import importlib.util
sp=importlib.util.spec_from_file_location('h',r/h);helper=importlib.util.module_from_spec(sp);sp.loader.exec_module(helper)
def worker(raw):
 source=raw.decode();m=helper.mask(source);a=m.index('pub async fn worker(');start=m.index('{',a);d=1;b=start+1
 while d:d+=(m[b]=='{')-(m[b]=='}');b+=1
 return source[a:b].encode()
assert worker(actual)==worker(expected),'worker orchestration differs from projected transformations' 
fragments=Path(__file__).resolve().parent/'worker-projection/fragments'
for path in fragments.iterdir():
 assert path.read_bytes()==(scratch/path.name).read_bytes(),path.name+' fragment provenance mismatch'
result={'base':'fd3c74aef','orchestration_worker_byte_exact':True,'worker_before_lines':len(worker(old).splitlines()),'worker_after_lines':len(worker(actual).splitlines()),'source_sha256':hashlib.sha256(actual).hexdigest(),'projection_directory':str(t),'scope':'Replay of documented scoped extractions plus compiler-required borrowed input adjustments. This checks complete worker assembly against the baseline, not exhaustive behavior equivalence.'}
Path(__file__).with_name('worker-orchestration-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
