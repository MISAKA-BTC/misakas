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
#          scripts/check-doc-citations.sh <doc-path> <tree-a> <tree-b>   # diff mode
#
# Two trees is the mode that matters at a freeze. One tree tells you the citations
# RESOLVE. Two tells you which ones DEPEND on which tree you meant — and those are
# the only ones a blanket "this document is impl-relative" note can hurt. Declaring
# a tree is not a null operation: it fixed six wrong citations in this card and
# silently inverted one that had been right, in the very sentence warning about the
# hazard. Everything that resolves identically on both trees is immune to the label.
# (Mode suggested by session 1c, who found that inversion by running exactly this.)
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

# ---- diff mode: resolve against two trees and show only what disagrees -------------
if [ -n "${3:-}" ]; then
    a="$2"; b="$3"; differ=0
    echo "citations in $doc that resolve DIFFERENTLY on $a vs $b:"
    printf '%s' "$body" | grep -oE '[a-zA-Z0-9_/.-]+\.rs:[0-9]+' | sort -u | while read -r cite; do
        f="${cite%%:*}"; n="${cite##*:}"
        la=$(git ls-tree -r --name-only "$a" | grep -E "(^|/)${f}$" | head -1)
        lb=$(git ls-tree -r --name-only "$b" | grep -E "(^|/)${f}$" | head -1)
        va=$([ -n "$la" ] && git show "${a}:${la}" | sed -n "${n}p" | sed 's/^[[:space:]]*//' || echo "<no such file>")
        vb=$([ -n "$lb" ] && git show "${b}:${lb}" | sed -n "${n}p" | sed 's/^[[:space:]]*//' || echo "<no such file>")
        if [ "$va" != "$vb" ]; then
            printf '  %s\n    %-9s %s\n    %-9s %s\n' "$cite" "$a" "${va:-<line absent>}" "$b" "${vb:-<line absent>}"
        fi
    done
    echo "-- citations not listed resolve identically on both trees, so the document's tree label cannot change what they mean."
    exit 0
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
    # `"${tree}:${full}"` and NOT `"$tree:consensus/..."`. Under zsh a `:` directly
    # after a parameter expansion begins a history modifier, and `c` is one of them —
    # so `"$b:consensus/x.rs"` expands to `palw-adr0082-implonsensus/x.rs`, git show
    # fails on a ref that never existed, and a piped `grep -c` reports 0 matches from
    # a command that never opened the file. It reads as evidence of ABSENCE and prints
    # no error anywhere. This repo's most-cited directory is `consensus/`, so the trap
    # fires on the common case. Keep both sides braced. (Found by session 1c.)
    if [ -n "$tree" ]; then line=$(git show "${tree}:${full}" | sed -n "${n}p")
    else line=$(sed -n "${n}p" "${root}/${full}"); fi
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
