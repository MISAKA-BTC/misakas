#!/usr/bin/env bash
# Which unmerged branches would MOVE `misaka-palw-derive/src` if merged into the
# release branch — i.e. which ones still have to land before `transformer_id` can be
# pinned once and stay pinned.
#
# This exists because "derive/src is frozen" is a sweep and never a recollection,
# and because the two obvious sweeps both answer a different question:
#
#   git diff <rel> <branch> -- <path>   compares END STATES. A stale branch that
#                                       merely predates the work reports every file
#                                       as different. It found ELEVEN, most of them
#                                       abandoned branches from weeks ago.
#   git log <rel>..<branch> -- <path>   counts COMMITS on the branch. It over-reports
#                                       a branch whose commits produce a tree the
#                                       release already has. It found TWENTY-THREE,
#                                       including palw-adr0082-impl, whose derive/src
#                                       is byte-identical to the release branch's.
#
# The question that matters is neither: it is whether the MERGE RESULT's derive/src
# tree differs from the release branch's. Git's tree object is a content hash of that
# directory, so comparing merge-tree output against it is exact and cheap. That found
# FOUR. Eleven, twenty-three and four are all honest counts of different things, and
# only the last one predicts whether the pin will go stale.
#
# DEMONSTRATED at the moment it mattered, 2026-09-03. `palw-artifact-names-genesis-row`
# was still sitting at its old tip while its content had been merged into
# `palw-merge-resolved`. Against that base this script reports "no move" — correctly,
# because the merge result's subtree already equals the base's. The `git log` form would
# have reported it as a blocker, and the reaction to a phantom blocker at a freeze is to
# WAIT for a branch that is already in. Subsumption is the case that separates the three
# questions, and it is the case that actually occurs.
#
#   usage: scripts/check-derive-freeze.sh [release-branch] [path]
set -uo pipefail
rel="${1:-palw-testnet-5f}"
path="${2:-misaka-palw-derive/src}"

base=$(git rev-parse "${rel}:${path}" 2>/dev/null) || { echo "no such path on ${rel}: ${path}"; exit 2; }
echo "${rel}:${path} tree = ${base}"
echo

found=0
for b in $(git branch --format='%(refname:short)' --no-merged "$rel" 2>/dev/null); do
    # cheap pre-filter: no commits touching the path means the merge cannot move it
    [ "$(git log --oneline "${rel}..${b}" -- "$path" 2>/dev/null | wc -l | tr -d ' ')" = "0" ] && continue
    mt=$(git merge-tree --write-tree "$rel" "$b" 2>/dev/null | head -1)
    if [ -z "$mt" ]; then
        echo "  CONFLICTS  ${b}  (resolve, then re-run — a conflicted merge can move it either way)"
        found=$((found+1)); continue
    fi
    st=$(git rev-parse "${mt}:${path}" 2>/dev/null)
    if [ "$st" != "$base" ]; then
        echo "  MOVES IT   ${b}  -> ${st:0:12}"
        found=$((found+1))
    fi
done

echo
if [ "$found" -eq 0 ]; then
    echo "-- nothing queued would move ${path}. It is frozen, and the pin can be taken."
else
    echo "-- ${found} branch(es) still to land. Pinning now means pinning twice."
fi
exit 0
