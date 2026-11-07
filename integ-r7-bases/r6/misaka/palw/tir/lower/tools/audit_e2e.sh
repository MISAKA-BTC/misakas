#!/usr/bin/env bash
# End-to-end audit of one or more audit models (tools/audit_e2e.py): lower + calibrate at 512 +
# artifact (palw-tir-fidelity), declare the class at 512 positions under the network's ceilings
# (palw-class declare-layout, testnet-12), its closes against the carriable cap (close-sizes),
# greedy ids against HF's (palw-tir-generate --compare) and the teacher-forced logits against HF's
# (audit_compare.py). One summary line a model.
#   TOOLS=<dir with palw-tir-fidelity, palw-tir-generate> PC=<palw-class> PY=<python> audit_e2e.sh AUDIT_DIR name...
set -u
A=$1; shift
for n in "$@"; do
  d=$A/$n
  "$TOOLS/palw-tir-fidelity" "$d" --calib "$d/calib.json" --artifact-out "$d/lowered.palwtir" --json > "$d/fid.json" 2> "$d/fid.err" \
    || { echo "$n FAIL lowering: $(grep -v '^\[' "$d/fid.err" | tail -1)"; continue; }
  nodes=$(sed -n 's/.*TIR program v1: \([0-9]*\) blocks, \([0-9]*\) layers, \([0-9]*\) nodes.*/\3/p' "$d/fid.err" | head -1)
  "$PC" declare-layout --network testnet-12 --max-context 512 --out "$d/class.palwtir" "$d/lowered.palwtir" > "$d/declare.txt" 2>&1
  adm=$(grep -oE "ADMISSIBLE|REFUSED" "$d/declare.txt" | head -1)
  tile=$(sed -n 's/.*layout .*commit tiles (\(.*\) distinct).*/\1/p' "$d/declare.txt" | head -1)
  if [ "$adm" != ADMISSIBLE ]; then echo "$n nodes $nodes declare ${adm:-ERROR}: $(grep -E 'REFUSED|rror' "$d/declare.txt" | head -1 | cut -c1-220)"; continue; fi
  "$PC" close-sizes --network testnet-12 "$d/class.palwtir" > "$d/close.txt" 2> "$d/close.err"
  close=$(tail -1 "$d/close.txt" | sed -n 's/.*worst \([0-9]*\) bytes.*/\1/p'); fits=$(grep -c "every close fits" "$d/close.txt")
  "$TOOLS/palw-tir-generate" "$d/class.palwtir" --prompts "$d/hf.json" --new-tokens 12 --compare --out "$d/gen.json" 2> "$d/gen.err"
  "$TOOLS/palw-tir-generate" "$d/class.palwtir" --sequences "$d/hf.json" --logits-out "$d/int.f32" 2>> "$d/gen.err"
  greedy=$($PY -c "import json; v=json.load(open('$d/gen.json')); print(f\"{v['identical']}/{v['total']}\")" 2>/dev/null)
  tf=$($PY "$(dirname "$0")/audit_compare.py" "$d" 2>/dev/null)
  echo "$n nodes $nodes ADMISSIBLE tiles {$tile} worst-close $close fits $fits greedy $greedy tf $tf"
done
