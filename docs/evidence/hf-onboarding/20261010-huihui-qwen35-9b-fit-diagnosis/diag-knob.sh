#!/usr/bin/env bash
# H1 successor diagnostic: re-fit an existing layer-prefix run with other policy knobs, reusing its calibration statistics
# (no recalibration). Usage: diag-knob.sh <base diag dir name> <tag> [pack build opts...]   e.g. diag-knob.sh hui-L4-c512 h1p0 --headroom16 1.0
set -uo pipefail
BASE=$1; TAG=$2; shift 2
RR=/Users/wata/Downloads/MISAKA-wt-b/wh-h1-run
PC=${PALW_CLASS_BIN:-$RR/bin/int-2c89ca386/palw-class}
SRC=$RR/diag/$BASE; D=$RR/diag/$BASE-$TAG
case $BASE in hui-L1-*) P=$RR/diag/prefix-hui-L1;; hui-L4-*) P=$RR/diag/prefix-hui-L4;; q08-L1-*) P=$RR/diag/prefix-q08-L1;; q08-L4-*) P=$RR/diag/prefix-q08-L4;; *) echo "unknown base"; exit 1;; esac
CL=${BASE##*-c}
mkdir -p $D; cp -n $SRC/eval.json $D/eval.json; mkdir -p $D/hfref; cp -n $SRC/hfref/* $D/hfref/
rm -rf $D/pack $D/class.palwtir $D/class.palwtir.chunks
/usr/bin/time -l $PC pack build --model $P --out $D/class.palwtir --pack $D/pack --stats-in $SRC/pack/stats.json --context $CL --name $BASE-$TAG \
   --hf-reference $D/hfref --prompts 1 --prefill 2 --decode 1 --stream --allow-out-of-tolerance "$@" > $D/build.out 2> $D/build.err
echo "build rc=$?"; grep -E "against the reference|rror" $D/build.err | cut -c1-300
rm -rf $D/class.palwtir $D/class.palwtir.chunks
