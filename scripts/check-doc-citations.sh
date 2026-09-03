#!/usr/bin/env bash
# Resolve every `path.rs:NNNN` citation in a document against a tree, and print the
# line each one actually lands on.
#
# The defect this exists for: a citation read from one branch and written into a
# document committed to another. It looks like rigour, it survives every test suite,
# and it is wrong in a way only a reader who re-opens the file can see. It cost this
# card six wrong citations in a single session — all read from `palw-adr0082-impl`,
# all written into a document living on `palw-testnet-5f`.
#
# This is a REPORT, not a verdict. It exits non-zero only when a file or line is
# missing outright; a citation that resolves to the wrong line still resolves, so
# the output has to be READ. That is deliberate — a gate that printed only "ok"
# here would be the same unfalsifiable check the card warns about everywhere else.
#
#   usage: scripts/check-doc-citations.sh <doc-path> [tree-ish]
#          tree-ish defaults to the working tree.
set -uo pipefail
doc="${1:?usage: check-doc-citations.sh <doc-path> [tree-ish]}"
tree="${2:-}"
root="$(git rev-parse --show-toplevel)"
rc=0

# The DOCUMENT is always the one in front of you — the working copy. Only the CODE
# moves with the tree-ish. Reading both from the same tree-ish answers a question
# nobody asked ("was this old document consistent with its own branch?") and hides
# the one that matters: do the citations in the document I am about to ship resolve
# against the tree I claim I read them from.
body=$(cat "$root/$doc") || { echo "cannot read $doc"; exit 2; }
if [ -n "$tree" ]; then
    listing=$(git ls-tree -r --name-only "$tree") || { echo "no such tree: $tree"; exit 2; }
    label="$tree"
else
    listing=$(git -C "$root" ls-files)
    label="the working tree"
fi

echo "citations in $doc, resolved against $label:"
cites=$(printf '%s' "$body" | grep -oE '[a-zA-Z0-9_/.-]+\.rs:[0-9]+' | sort -u)
[ -z "$cites" ] && { echo "  (none)"; exit 0; }

while read -r cite; do
    f="${cite%%:*}"; n="${cite##*:}"
    full=$(printf '%s\n' "$listing" | grep -E "(^|/)${f}$" | head -1)
    if [ -z "$full" ]; then
        printf '  %-44s !! NO SUCH FILE\n' "$cite"; rc=1; continue
    fi
    if [ -n "$tree" ]; then line=$(git show "$tree:$full" | sed -n "${n}p")
    else line=$(sed -n "${n}p" "$root/$full"); fi
    if [ -z "$line" ]; then
        printf '  %-44s !! LINE %s OUT OF RANGE (%s)\n' "$cite" "$n" "$full"; rc=1; continue
    fi
    printf '  %-44s %s\n' "$cite" "$(printf '%s' "$line" | sed 's/^[[:space:]]*//' | cut -c1-70)"
done <<< "$cites"

if [ "$rc" -ne 0 ]; then
    echo "-- at least one citation names a file or line that does not exist in $label."
else
    echo "-- every citation resolves. READ THE LINES: resolving is not the same as being right."
fi
exit "$rc"
