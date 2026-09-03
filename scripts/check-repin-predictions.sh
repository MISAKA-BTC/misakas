#!/usr/bin/env bash
# check-repin-predictions.sh <tree-ish>
#
# The re-pin ceremony's mechanical guard. Every value below was PREDICTED before the freeze, from
# a tree whose `misaka-palw-derive/src` hash is recorded here. At the paste, each value is read
# again from the frozen tree and must EQUAL its prediction.
#
# Why this is a script and not a checklist line: tonight three separate rules that existed only as
# prose failed to fire for the people who wrote them, minutes after writing them. The three that
# DID fire were all mechanical. A rule earns its keep once it is attached to a trigger.
#
# The rule it enforces is 1c's, from the mutation experiment:
#     "a red pin protects nothing while it is red, and a re-genesis is precisely the window
#      where several are red at once."
# So: any mismatch STOPS. Not warns. The operator then names the change that moved it, or the
# ceremony does not proceed.
#
# usage: check-repin-predictions.sh palw-adr0082-impl
set -euo pipefail

die() { printf '\n  STOP: %s\n' "$*" >&2; exit 1; }
[ $# -eq 1 ] || { echo "usage: $0 <tree-ish>"; exit 2; }
tree=$1
git rev-parse --verify -q "${tree}^{tree}" >/dev/null || die "not a tree-ish: $tree"

# ---- the predictions, and the tree they were predicted from -------------------------------
PREDICTED_DERIVE_SRC=4969f8dc051cac3165a051b439efb65f7630f026
PREDICTED_SOURCE_TREE_SHA=637858dba5ea5e34b9459a580b2b81d1361aecf450bc615a4ee9621d4953a988
PREDICTED_T11_FP=71efa66480211731e3dc6fa2312ed73f7ed11b93372a19a55ac66ef39b65920e
PREDICTED_DEVNET_FP=c0da0c9024d68b94b95010d1566cb1d535a818cd0727d9978906b0a2a8b13692
PREDICTED_FP_GOLDEN=c940b5c36ee40846087e6c5927d6e6b5
PREDICTED_PREMINE_BUILDS=ba2612417e7e0817

fail=0
check() { # name expected actual
  if [ "$2" = "$3" ]; then
    printf '  OK    %-22s %s\n' "$1" "$(printf '%s' "$2" | cut -c1-24)…"
  else
    printf '  DIFF  %-22s\n        predicted %s\n        read      %s\n' "$1" "$2" "${3:-<empty>}"
    fail=$((fail + 1))
  fi
}

printf 'tree      %s  (%s)\n' "$tree" "$(git rev-parse --short "$tree")"
printf '%s\n' "-- the tree the predictions came from ---------------------------------------"

# THIS IS THE FIRST CHECK ON PURPOSE. Every other prediction is a function of these bytes, so a
# moved derive/src invalidates the whole set at once and there is no point reading the rest.
actual_derive=$(git rev-parse "${tree}:misaka-palw-derive/src")
check "derive/src tree" "$PREDICTED_DERIVE_SRC" "$actual_derive"
[ "$fail" -eq 0 ] || die "derive/src moved. Every prediction below is a function of these bytes,
        so they are ALL stale — do not read them and do not paste them. Name the commit
        that touched misaka-palw-derive/src, re-derive, and re-predict."

printf '%s\n' "-- the predicted values, against an EXTRACTION table ----------------------"

# The first version of this script grepped the tree for the predicted values and reported all
# four as DIFF because it found none of them. It was right to stop and wrong about why:
# **these values are what the re-pin WRITES, not what the source currently holds.** The tree
# legitimately contains the OLD pins until the paste. Grepping for the answer cannot work, and
# a check that cannot read reports "different" when it means "absent" — which is safe exactly
# once and misleading every other time.
#
# So the table comes from the extractor (5b's finalize prints it), passed as a file of
#     <name> <value>
# lines. No table, no verdict — and saying so is the point.
table=${TABLE:-}
if [ -z "$table" ] || [ ! -f "$table" ]; then
  cat <<'NEED'
  NO EXTRACTION TABLE. This script cannot answer the question from the tree alone.

  Run it as:   TABLE=/path/to/extracted-pins.txt scripts/check-repin-predictions.sh <tree>
  where the file holds one `<name> <value>` per line, from the extractor that computes them.

  What IS verified above, and is the precondition for the rest: derive/src equals the tree the
  predictions were made from. If that had moved, every prediction would be stale and no table
  would be worth comparing.
NEED
  exit 0
fi

get() { awk -v k="$1" '$1==k {print $2; found=1} END{ if(!found) print "<absent>" }' "$table"; }
check "source_tree_sha256" "$PREDICTED_SOURCE_TREE_SHA" "$(get source_tree_sha256)"
check "t11 fingerprint"    "$PREDICTED_T11_FP"          "$(get t11_fingerprint)"
check "devnet fingerprint" "$PREDICTED_DEVNET_FP"       "$(get devnet_fingerprint)"
check "fp golden"          "$PREDICTED_FP_GOLDEN"       "$(get fp_golden)"
check "premine builds"     "$PREDICTED_PREMINE_BUILDS"  "$(get premine_builds)"

printf '%s\n' "-- summary -----------------------------------------------------------------"
if [ "$fail" -eq 0 ]; then
  printf '  all five predictions hold, against a table computed from the frozen tree.\n'
else
  die "$fail value(s) differ from the prediction.

        A difference is NOT a signal to update the prediction. It means something moved
        between the prediction and the paste, and the ceremony's whole claim is that
        nothing did. NAME THE CHANGE THAT MOVED IT, or stop.

        And distinguish the two failures: <absent> means the table does not carry that
        name — the check did not run. A hex value that differs means it ran and disagreed.
        'It is probably fine' is the sentence this script exists to interrupt."
fi
