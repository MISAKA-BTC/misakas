#!/bin/bash
# Run some census entries through the harness and print one line per entry (and the failing stage, if any).
#
#   census_subset.sh id,id,... [adapters-dir]
#
# Needs the census fixtures (`gen_fixtures.py --entries census_v2.json ...`, see docs/design/palw/tir/corpus-v2.md §7).
# One cargo command at a time, JOBS=1, two test threads.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HOME/Downloads/MISAKA-wt-b/corpus-target} CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_TEST_THREADS=2 OMP_NUM_THREADS=2
OUT=${PALW_CORPUS_REPORT:-/tmp/census-sub.json}
PALW_CORPUS_ONLY=$1 PALW_CORPUS_MANIFEST=$HERE/census_v2.json PALW_CORPUS_SPECS=$HERE/census-specs \
  PALW_CORPUS_FIXTURES=${PALW_CORPUS_FIXTURES:-$HOME/Downloads/MISAKA-wt-b/corpus-fixtures-census} \
  PALW_CORPUS_ADAPTERS=${2:-$HERE/census-adapters} PALW_CORPUS_REPORT=$OUT \
  cargo test -p misaka-palw-tir-lower --test corpus_v2 corpus_v2_full -- --ignored --nocapture 2>&1 | grep -E '^\[' | cut -c1-210
python3 - "$OUT" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
for e in r['entries']:
    rd = e['read']
    err = ''
    if e['level'] == 'C':
        for k in ('user', 'none', 'auto'):
            x = rd.get(k) or {}
            if x and not x.get('ok'):
                err = k + ': ' + (x.get('failure') or {}).get('error', '')[:360]
                break
    print(f"{e['id']:24} {e.get('level_label') or e['level']} via={e.get('via')} failed_stage={e.get('failed_stage')} {err}")
    for k, v in e.get('stages', {}).items():
        if not v.get('ok'):
            print('    stage', k, json.dumps(v)[:420])
PY
