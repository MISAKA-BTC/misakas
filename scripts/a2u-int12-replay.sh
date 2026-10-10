#!/usr/bin/env zsh
# scripts/a2u-int12-replay.sh — replay the A2U pin test's chains through the live testnet-12 build itself
# (docs/design/palw/a2-uniformity-new-kinds.md §5).
#
#   scripts/a2u-int12-replay.sh <work-dir> <dump> [<dump>…]
#
# <dump>: a file the newer build's pin test wrote (`A2U_INT12_REPLAY_OUT=<dir> cargo test -p kaspa-consensus --lib t12_a2u`
# writes <dir>/a2u-launch.borsh and <dir>/a2u-release.borsh). <work-dir>: where the live build's source tree and its own cargo target
# go (kept between runs; ~150 MB of source, a kaspa-consensus test build of target).
#
# What it does: extracts the source of the live release (`git archive`, no worktree), drops
# consensus/src/pipeline/virtual_processor/tests/a2u_int12_replay.rs.int12 in as a module of its tests, and runs that one test on
# the dumps. The live build's own harness, fence lists and code judge every block; any verdict, root, refusal or view a newer build
# reads differently below its fences fails it. Builds go through the shared build gate (buildslot.sh), as every lane's do.
set -eu
LIVE=0b1c11b87                     # testnet-12's live release: int-12, rcore/int-12
REPO=${0:A:h:h}
BUILDSLOT=${BUILDSLOT:-$HOME/Downloads/MISAKA-wt-b/buildslot.sh}
(( $# >= 2 )) || { print -u2 "usage: $0 <work-dir> <dump> [<dump>…]"; exit 2; }
WORK=${1:A}; shift
dumps=()
for d in "$@"; do [[ -f $d ]] || { print -u2 "no such dump: $d"; exit 2; }; dumps+=(${d:A}); done
TREE=$WORK/int12-src
if [[ ! -f $TREE/.a2u-tree-$LIVE ]]; then
  rm -rf $TREE && mkdir -p $TREE
  git -C $REPO archive --format=tar $LIVE > $WORK/int12-src.tar
  tar -x -C $TREE -f $WORK/int12-src.tar && rm $WORK/int12-src.tar
  touch $TREE/.a2u-tree-$LIVE
fi
TESTS=$TREE/consensus/src/pipeline/virtual_processor/tests
cp $REPO/consensus/src/pipeline/virtual_processor/tests/a2u_int12_replay.rs.int12 $TESTS/a2u_int12_replay.rs
grep -q '^mod a2u_int12_replay;' $TREE/consensus/src/pipeline/virtual_processor/tests.rs \
  || print '\n// A2U: the newer build'"'"'s chains, replayed through this (the live) build — scripts/a2u-int12-replay.sh.\nmod a2u_int12_replay;' \
       >> $TREE/consensus/src/pipeline/virtual_processor/tests.rs
cd $TREE
A2U_INT12_REPLAY_IN=${(j:,:)dumps} CARGO_TARGET_DIR=$WORK/int12-target CARGO_INCREMENTAL=0 \
  $BUILDSLOT cargo test --offline -p kaspa-consensus --lib a2u_int12_replay -- --nocapture
