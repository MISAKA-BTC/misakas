#!/usr/bin/env bash
# H1 successor: headroom16 sweep with the fidelity tool on a prefix (fast: ~2 min each). Usage: fid-sweep.sh <base> <prefixdir> <positions> h1 h2 ...
set -uo pipefail
RR=/Users/wata/Downloads/MISAKA-wt-b/wh-h1-run; WT=/Users/wata/Downloads/MISAKA-wt-b/wh-h1
FID=$RR/bin/h1-fidelity-6570d66c6-dirty/palw-tir-fidelity; PY=/Users/wata/Downloads/MISAKA-wt-b/tir-venv/bin/python
BASE=$1; P=$2; NP=$3; shift 3
D=$RR/diag/$BASE
for h in "$@"; do
  O=$RR/diag/fid-$BASE-h$h; rm -rf $O; mkdir -p $O
  HF_HUB_OFFLINE=1 $FID $P --calib $D/calib.json --eval $D/eval.json --stats-in $D/pack/stats.json --headroom16 $h --exec --logits-out $O > $O/out.json 2> $O/err.log
  echo "== headroom16=$h $(grep -E 'top-1' $O/err.log | cut -c11-120)"
  $PY -I $WT/tests/hf-onboarding/kl_compare.py --vocab 248320 --positions $NP hf=$D/hfref/hf-reference.f32 int=$O/int.f32 | grep "hf ->" 
  rm -f $O/float.f32
done
