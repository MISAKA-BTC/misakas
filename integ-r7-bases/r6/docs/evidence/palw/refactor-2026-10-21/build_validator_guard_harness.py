from pathlib import Path
import importlib.util,re,json
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
def fn(s,name):
 m=h.mask(s);a=m.index('fn '+name+'(');b=m.index('{',a);d=1;i=b+1
 while d:d+=(m[i]=='{')-(m[i]=='}');i+=1
 return s[a:i]
p=r/'consensus/core/src/palw_source_guard_v1.rs';s=p.read_text();a=s.index('pub const PALW_PROCESSOR_SOURCE_FILES_V1:');b=s.index('];',a)+2;const=s[a:b]
loader='pub '+fn(s,'palw_processor_source_v1');loader=loader.replace('env!("CARGO_MANIFEST_DIR")',json.dumps(str(r/'consensus/core')))
p=r/'consensus/core/src/palw_attempt_rules_v1.rs';body=fn(p.read_text(),'t12_030_the_processors_attribution_calls_are_the_shared_gates');body=re.sub(r'include_str!\("([^"]+)"\)',lambda m:'include_str!('+json.dumps(str(p.parent/m[1]))+')',body)
Path('/tmp/palw-validator-guard-harness.rs').write_text('pub mod palw_source_guard_v1 {\n'+const+'\n'+loader+'\n}\n#[test]\n'+body+'\n')
print('Extracted original guard body and source inventory loader; only include/manifest paths made absolute. Not a full core-crate test run.')
