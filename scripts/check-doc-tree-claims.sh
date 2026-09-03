#!/usr/bin/env bash
# check-doc-tree-claims.sh <doc> <tree-a> <tree-b>
#
# Finds the class of defect `check-doc-citations.sh` CANNOT see.
#
# That script resolves `file.rs:NNNN` citations against a named tree. But a sentence like
#
#     "palw-a16-fp-worker's MODEL_ID is the v5 512 row"
#
# carries no file and no line, so there is nothing for a citation checker to resolve — and it is
# TRUE on one tree and FALSE on another. On 2026-09-03 that sentence was one command away from
# being filed as a defect against the card that (correctly) contained it: it holds on
# `palw-adr0082-impl` and not on `palw-testnet-5f`, and the card named no tree.
#
# So this script asks a different question: for every code identifier the document mentions,
# DOES ITS DEFINITION DIFFER BETWEEN THE TWO TREES? Every hit is a sentence that must name its
# tree — not because the sentence is wrong, but because it cannot be checked without one.
#
# This is a REPORT, not a verdict. A differing identifier is not a defect; a differing identifier
# in a sentence with no tree named is. Only a human can see the second, so read the output.
#
# Exit 0 always unless the inputs are bad: a non-zero here would tempt someone to "fix" the
# report by removing identifiers from prose, which is the wrong direction.
set -euo pipefail

die() { printf '%s\n' "$*" >&2; exit 2; }
[ $# -eq 3 ] || die "usage: $0 <doc> <tree-a> <tree-b>"
doc=$1; a=$2; b=$3

[ -f "$doc" ] || die "no such document: $doc"
# The document is read from the WORKING COPY on purpose. Reading it from a tree-ish answers
# "was this old document consistent with its own branch", which nobody asked.
for t in "$a" "$b"; do
  git rev-parse --verify -q "${t}^{tree}" >/dev/null || die "not a tree-ish: $t"
done

# Backtick-quoted tokens that look like Rust items: SCREAMING_CONSTS, snake_fns, paths with `::`.
# Deliberately NOT bare words — a document says "the worker" constantly and means prose.
mapfile -t idents < <(
  grep -oE '`[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*`' "$doc" \
    | tr -d '`' | sort -u \
    | grep -E '^[A-Z][A-Z0-9_]{4,}$|::' || true
)

[ ${#idents[@]} -gt 0 ] || { echo "no code identifiers found in $doc"; exit 0; }

printf 'doc   %s\n' "$doc"
printf 'trees %s  vs  %s\n' "$a" "$b"
printf '%s\n' "-- identifiers whose DEFINITION differs between the trees ------------------"

differ=0 same=0 absent=0
for id in "${idents[@]}"; do
  # The last path segment is what a definition line actually spells.
  leaf=${id##*::}
  # `git grep <rev>` takes the rev as its own argument, so it can never be eaten by a shell
  # history modifier the way `git show "$t:path"` can under zsh. See zsh-colon-c-eats-consensus.
  da=$(git grep -h -E "(const|static|fn|struct|enum|type)[[:space:]]+${leaf}\b" "$a" -- '*.rs' 2>/dev/null | sed 's/^[[:space:]]*//' | sort -u || true)
  db=$(git grep -h -E "(const|static|fn|struct|enum|type)[[:space:]]+${leaf}\b" "$b" -- '*.rs' 2>/dev/null | sed 's/^[[:space:]]*//' | sort -u || true)

  if [ -z "$da" ] && [ -z "$db" ]; then
    absent=$((absent + 1))
    continue
  fi
  if [ "$da" = "$db" ]; then
    same=$((same + 1))
    continue
  fi
  differ=$((differ + 1))
  printf '\n  %s\n' "$id"
  printf '    %-22s %s\n' "$a" "$(printf '%s' "${da:-<absent>}" | head -1 | cut -c1-96)"
  printf '    %-22s %s\n' "$b" "$(printf '%s' "${db:-<absent>}" | head -1 | cut -c1-96)"
done

printf '\n%s\n' "-- summary -----------------------------------------------------------------"
printf '  %3d identifiers differ between the trees   <- every sentence using one must name a tree\n' "$differ"
printf '  %3d identical on both trees                <- safe to mention without a tree\n' "$same"
printf '  %3d not found as a definition in either    <- prose, or defined outside *.rs\n' "$absent"

if [ "$differ" -gt 0 ]; then
  cat <<'NOTE'

  A differing identifier is not a defect. A differing identifier in a sentence that names no
  tree is one, because the sentence is unfalsifiable as written: a reader who checks it on the
  other tree concludes the document is wrong, and a reader who checks it on this one concludes
  it is right, and neither has learned anything about the code.
NOTE
fi
