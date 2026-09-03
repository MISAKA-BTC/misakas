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

printf '%s\n' "-- values read from that tree ----------------------------------------------"

# `git grep <rev>` keeps the rev as its own argument: it cannot be eaten by zsh's history
# modifier the way "$tree:path" can. (See zsh-colon-c-eats-consensus — `c` after `$var:` is one.)
read_const() { # <pattern> -> the first 64-hex or 32-hex literal on the matching line
  git grep -h -E "$1" "$tree" -- '*.rs' 2>/dev/null | grep -oE '[0-9a-f]{32,64}' | head -1 || true
}

check "source_tree_sha256" "$PREDICTED_SOURCE_TREE_SHA" "$(read_const 'source_tree_sha256|SOURCE_TREE_SHA256')"
check "t11 fingerprint"    "$PREDICTED_T11_FP"          "$(read_const 'PALW_RC_SHIPPED_FINGERPRINT|palw_rc_fingerprint')"
check "devnet fingerprint" "$PREDICTED_DEVNET_FP"       "$(read_const 'DEVNET_SHIPPED_FINGERPRINT|devnet_fingerprint')"
check "fp golden"          "$PREDICTED_FP_GOLDEN"       "$(read_const 'GOLDEN_VECTOR_IDS|golden_vector_ids')"

printf '%s\n' "-- summary -----------------------------------------------------------------"
if [ "$fail" -eq 0 ]; then
  printf '  all predictions hold. The pin and the tree it pins are the same object.\n'
else
  die "$fail value(s) differ from the prediction.

        A difference is NOT a signal to update the prediction. It means something moved
        between the prediction and the paste, and the ceremony's whole claim is that
        nothing did. NAME THE CHANGE THAT MOVED IT, or stop.

        'It is probably fine' is the sentence this script exists to interrupt."
fi
