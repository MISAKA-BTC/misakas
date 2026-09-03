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
# WITHDRAWN 2026-09-04, named by 3e BEFORE the extraction that will move it: the a16-v5 family the
# cut ships is 5e's per-kernel family (28 vectors, 409,069 B, five carriers; drilled_kernel_ids with
# the gate `reachable ⊆ drilled`), replacing impl's one-carrier entry whose six leaves cannot have
# adjudicated the ten kernels it declares (ADR-0069's forbidden object). Adopting it moves the
# certified-set root (court_e2e_root) and with it the testnet-11 fingerprint. The eight transformer
# ids, source_tree, fp golden and premine do not move (none read the family).
#   value held until then: 71efa66480211731e3dc6fa2312ed73f7ed11b93372a19a55ac66ef39b65920e
# The new value comes ONLY from a table whose `extracted_from` is the tip that carries the family.
PREDICTED_T11_FP=WITHDRAWN-family-adoption-see-above
# WITHDRAWN 2026-09-03: devnet genesis now registers the same class set as testnet-11
# (floor + graph-v5@512 + QWEN36) so the drill rehearses the chain being cut. That moves this
# fingerprint by design. The prediction is NOT updated by guessing — the new value comes from
# the extraction table, and until it does this check is DISABLED rather than made to pass.
#
# A pin that moves for a NAMED cause, named in advance and in writing by the person moving it,
# is the only kind this ceremony accepts. A pin that moves and is then quietly re-predicted is
# the thing this script exists to prevent, and editing this line to the new value would be
# exactly that.
# NAMED MOVE, 2026-09-04 — the withdrawal above is resolved by two causes, each written down by
# the person moving it BEFORE the extraction, and each with its own intermediate value:
#   (a) 09a71652  devnet genesis mirrors testnet-11's class set   c0da0c90… -> 24d55f6d…
#   T1  a733b21e  devnet close ceiling derived from its carrier   24d55f6d… -> 34c7e482…
#       count (81,920 -> 83,333, a ruleset field)
# The value below is the extractor's, from the table with `extracted_from 971b2eff…`, not a
# guess and not a re-prediction: it is written here only because both moves are named.
# WITHDRAWN AGAIN 2026-09-04 — third named move, named by 3e before the extraction: the per-kernel
# family's certified-set root (e4f97110, PALW_RC_COURT_E2E_ROOT_BYTES = e649e7c0…) is in devnet's
# bundle too, so the same commit that moves the t11 fingerprint moves this one. Value held until then:
#   34c7e4829eadb996e50871ed9bf32055fe4f54057e66814a6bab1c54b67bd8e1   (after (a) and T1)
# The new value comes ONLY from a table whose `extracted_from` is a tip at or after e4f97110.
PREDICTED_DEVNET_FP=WITHDRAWN-family-root-see-above
PREDICTED_FP_GOLDEN=c940b5c36ee40846087e6c5927d6e6b5
PREDICTED_PREMINE_BUILDS=ba2612417e7e0817
# The genesis hash the re-pin will write: PALW_RC_GENESIS.hash recomputed over the NEW utxo_commitment by
# premine.rs::print_premine_commitment (the node's own header hasher). Predicted here from a SECOND
# computation — this session's worktree at 09a71652, premine.rs/genesis.rs byte-identical to 971b2eff —
# and independently reported by 3e's finalize as ad30b5cb965ad305…9b33edb7. The family adoption does not
# read the premine, so it is predicted unchanged on the family tip. The table's line is
# t11_genesis_hash_implied; <absent> is a DIFF here on purpose (the printer printed nothing).
PREDICTED_T11_GENESIS=ad30b5cb965ad305dfa1dc7516935763ea2623105581b83bb9359c7247157d36b0f8003b337cdad366e3895c8f159e99332be16e258b144dddf483bf9b33edb7

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

# **The table must say which tree it came from.** Without this the guard compares five values
# from an unknown run against a tree it verified separately, and the two halves are joined only
# by whoever typed the path — which is the same lost join as a `--report` that carries ids and
# omits whether the run was valid, and as a green suite over a closed door. A table extracted
# from a DIFFERENT commit can hold five correct-looking values and mean nothing about this one.
#
# Required line:   extracted_from <full sha of the tree the extractor ran on>
grep -qE '^extracted_from[[:space:]]+[0-9a-f]{40}$' "$table" || die "the table has no \`extracted_from <sha>\` line.

        Five values from an unnamed run prove nothing about this tree. Have the extractor
        print the commit it ran on, or this guard is comparing two things nobody joined."
from_sha=$(awk '$1=="extracted_from" {print $2}' "$table")
tree_sha=$(git rev-parse "$tree")
if [ "$from_sha" != "$tree_sha" ]; then
  die "the table was extracted from a DIFFERENT commit.

        table says   $from_sha
        checking     $tree_sha  ($tree)

        Values from another run are not evidence about this one, however right they look."
fi
printf '  OK    %-22s %s\n' "table provenance" "extracted_from $(printf '%s' "$from_sha" | cut -c1-12)… == $tree"

get() { awk -v k="$1" '$1==k {print $2; found=1} END{ if(!found) print "<absent>" }' "$table"; }
check "source_tree_sha256" "$PREDICTED_SOURCE_TREE_SHA" "$(get source_tree_sha256)"
check "t11 fingerprint"    "$PREDICTED_T11_FP"          "$(get t11_fingerprint)"
check "devnet fingerprint" "$PREDICTED_DEVNET_FP"       "$(get devnet_fingerprint)"
check "fp golden"          "$PREDICTED_FP_GOLDEN"       "$(get fp_golden)"
check "premine builds"     "$PREDICTED_PREMINE_BUILDS"  "$(get premine_builds)"
check "t11 genesis (implied)" "$PREDICTED_T11_GENESIS"   "$(get t11_genesis_hash_implied)"

printf '%s\n' "-- summary -----------------------------------------------------------------"
if [ "$fail" -eq 0 ]; then
  printf '  all six predictions hold, against a table computed from the frozen tree.\n'
else
  die "$fail value(s) differ from the prediction.

        A difference is NOT a signal to update the prediction. It means something moved
        between the prediction and the paste, and the ceremony's whole claim is that
        nothing did. NAME THE CHANGE THAT MOVED IT, or stop.

        And distinguish the two failures: <absent> means the table does not carry that
        name — the check did not run. A hex value that differs means it ran and disagreed.
        'It is probably fine' is the sentence this script exists to interrupt."
fi
