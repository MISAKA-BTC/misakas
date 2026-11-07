from pathlib import Path
import subprocess,json,hashlib
r=Path.cwd();s=(r/'consensus/src/pipeline/virtual_processor/processor/palw_accept.rs').read_text();a=s.index('/// Immutable output of the existing production resolver');b=s.index('impl VirtualStateProcessor {',a);decl=s[a:b]
make='''pub(super) fn make(point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
 extras: kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1) -> PalwResolvedBlockInputsV1 {
 PalwResolvedBlockInputsV1::at(point, extras)
}
'''
header='mod production {\n'+decl+make+'}\n'
setup='''let mut point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
 block: Default::default(), daa_score: 200, blue_score: 1, subsidy: 0,
};
let extras = kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {canonical_work_daa:Some(100),..Default::default()};
'''
positive=header+'''#[test] fn the_snapshot_keeps_its_point_values_and_legacy_diagnostics() {
'''+setup+'''let debug = format!("{extras:?}");
let resolved = production::make(&point, extras);
point.daa_score = 300;
assert_eq!(resolved.point().daa_score, 200);
assert_eq!(resolved.canonical_work_daa, Some(100));
assert_eq!(format!("{resolved:?}"), debug);
let mut facts = resolved.into_legacy_extras();
facts.class_court_windows.insert(Default::default(), 99);
assert_eq!(facts.canonical_work_daa, Some(100));
assert_eq!(point.daa_score, 300);
}
'''
negative=header+'fn main() {\n'+setup+'''let mut resolved = production::make(&point, extras);
resolved.canonical_work_daa = None;
}
'''
Path('/tmp/palw-resolved-positive.rs').write_text(positive);Path('/tmp/palw-resolved-negative.rs').write_text(negative)
lib=Path(json.loads((r/'docs/evidence/palw/refactor-2026-10-21/resolved-inputs-compatibility-library.json').read_text())['path']);common=['rustc','--edition=2024','--extern','kaspa_consensus_core='+str(lib),'-L','dependency='+str(r/'target/release/deps')]
subprocess.run(common+['--test','/tmp/palw-resolved-positive.rs','-o','/tmp/palw-resolved-positive'],check=True)
subprocess.run(['/tmp/palw-resolved-positive','--test-threads=1'],check=True)
refusal=subprocess.run(common+['--emit=metadata','/tmp/palw-resolved-negative.rs','-o','/tmp/palw-resolved-negative.rmeta'],capture_output=True,text=True);Path('/tmp/palw-resolved-negative-diagnostic.log').write_text(refusal.stderr);assert refusal.returncode!=0 and 'error[E0594]' in refusal.stderr and 'DerefMut' in refusal.stderr,refusal.stderr
report={'positive_snapshot_test':1,'readonly_assignment_compile_refusal':{'exit_code':refusal.returncode,'diagnostic':'E0594 / DerefMut missing'},'scope':'Actual new production wrapper declaration extracted verbatim; fixture constructor added inside its module. Linked against the recorded unchanged core input types. Native source/compile-fail harness, not a full consensus-crate test run.','core_library':str(lib),'core_library_sha256':hashlib.sha256(lib.read_bytes()).hexdigest()};Path('/tmp/palw-resolved-boundary-tests.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
