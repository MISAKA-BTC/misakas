"""Build a reviewable same-source candidate; a changed manifest invalidates it."""
import hashlib, json, os, pathlib, subprocess, sys, time
from source_manifest import ROOT, manifest
OUT = pathlib.Path(__file__).parent
ENV = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0')
CONFIG = ['--config','profile.dev.package.kaspa-consensus-core.opt-level=0','--config','profile.dev.package.kaspa-consensus.opt-level=0']
COMMANDS = [
 ('unified-build.log', ['cargo','test','-p','kaspa-consensus-core','-p','kaspa-consensus','-p','kaspad','-p','kaspa-p2p-flows','-p','kaspa-p2p-lib','-p','misaka-palw-sdk','-p','kaspa-mining','--features','kaspa-consensus/evm','--lib','--no-run','-j','2']+CONFIG),
 ('activation-tests.log', ['cargo','test','-p','kaspa-consensus-core','--test','palw_t12_g14_17000_arming','--test','palw_t12_17400_activation_readiness','--test','t12_fleet_identity_247ade1f9','--test','t12_deploy_kit_constants','--test','palw_t12_full_activation_16000','-j','2']+CONFIG+['--','--nocapture','--test-threads=2']),
 ('node-build.log', ['cargo','build','-p','kaspad','--bin','kaspad','--features','evm','-j','2']+CONFIG),
]
(OUT/'toolchain.txt').write_text(subprocess.check_output(['rustc','--version','--verbose'],cwd=ROOT,text=True))
before = manifest()
(OUT/'source-before-build.json').write_text(json.dumps(before,indent=2)+'\n')
results = []
for filename, command in COMMANDS:
    start = time.time()
    with (OUT/filename).open('w') as log:
        result = subprocess.run(command,cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
    results.append({'command':command,'environment_overrides':{k:ENV[k] for k in ['CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG']},'log':filename,'exit_code':result.returncode,'elapsed_seconds':round(time.time()-start,2)})
    (OUT/'build-results.json').write_text(json.dumps(results,indent=2)+'\n')
    print(json.dumps(results[-1]),flush=True)
    if result.returncode: sys.exit(result.returncode)
after = manifest()
(OUT/'source-after-build.json').write_text(json.dumps(after,indent=2)+'\n')
if before != after: raise SystemExit('source changed during build: candidate invalid, rebuild required')
print('source stable',before['aggregate_sha256'],flush=True)
