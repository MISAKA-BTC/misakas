#!/bin/bash
set -eu
printf '%s\n' "$$" >/tmp/misaka-positive-50d-legacy-owner.pid
q=/tmp/misaka-relay-quota.X3oX1R
if mountpoint -q "$q/mnt"; then exit 125; fi
mount -o loop,prjquota "$q/disk" "$q/mnt"
trap 'umount /tmp/misaka-relay-quota.X3oX1R/mnt' EXIT
export TMPDIR="$q/mnt/test"
xfs_io -c stat "$TMPDIR" >/tmp/misaka-positive-50d-legacy-quota.log
xfs_quota -x -c 'report -p -b -n' "$q/mnt" >>/tmp/misaka-positive-50d-legacy-quota.log
python3 - <<'PY'
from pathlib import Path
import hashlib,json,os,subprocess,time
binary=Path('/dev/shm/misaka-positive-pre-isolated-target/debug/deps/kaspa_consensus-a625c93f0f85ef22')
expected='583efd6d80d36d6de19d4bc9db21d0e96a6700fcf686975fa634253f4289cadd'
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(8<<20),b''):h.update(b)
 return h.hexdigest()
assert sha(binary)==expected,'frozen native binary changed'
quota=Path('/tmp/misaka-positive-50d-legacy-quota.log').read_text()
assert 'fsxattr.projid = 713' in quota and '0x200' in quota
rows=[line.split() for line in quota.splitlines() if line.startswith('#713')]
assert len(rows)==1 and rows[0][3]=='262144','original project hard bound required'
names=[
'g14_kernel_route_replay_and_reorg_reach_the_same_roots',
'g14_kernel_route_survives_a_node_restart_over_the_same_database',
'g14_kernel_route_survives_a_pruned_import',
'g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_across_reorg_replay_and_redemption',
]
listing=subprocess.check_output([str(binary),'g14_kernel_route','--list'],text=True)
report={'source_commit':'50d5eda4cd92c506eef20b042a6f85b505fd4877','binary':str(binary),'binary_sha256':expected,
 'scope':'Legacy G14 native regression on frozen 50d only. Not later source, Positive Assignment/Case acceptance, real-model PositivePassed, Gate C or Positive WorkerPayment.',
 'results':[]}
for short in names:
 found=[s.removesuffix(': test') for s in listing.splitlines() if s.endswith('::'+short+': test')]
 assert len(found)==1,(short,found)
 command=[str(binary),'--exact',found[0],'--nocapture','--test-threads=1']
 started=time.monotonic()
 log=Path('/tmp/misaka-positive-50d-legacy-'+short+'.log')
 with log.open('w') as out:
  process=subprocess.run(command,stdout=out,stderr=subprocess.STDOUT)
 Path(str(log).removesuffix('.log')+'.exit').write_text(str(process.returncode)+'\n')
 text=log.read_text()
 passed=process.returncode==0 and 'test result: ok. 1 passed;' in text
 report['results'].append({'test':found[0],'command':command,'exit_code':process.returncode,'exactly_one_passed':passed,'seconds':time.monotonic()-started,'log_sha256':sha(log)})
 print(short,'exit',process.returncode,'one_passed',passed,flush=True)
report['binary_sha256_after']=sha(binary)
assert report['binary_sha256_after']==expected
Path('/tmp/misaka-positive-50d-legacy-results.json').write_text(json.dumps(report,indent=2)+'\n')
raise SystemExit(0 if all(r['exactly_one_passed'] for r in report['results']) else 1)
PY
