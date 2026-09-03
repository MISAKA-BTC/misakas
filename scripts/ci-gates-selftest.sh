#!/usr/bin/env bash
#
# **Prove the gates can fail.** A gate never seen to fail is not known to be a gate.
#
# `scripts/ci-toolchain-pin-check.py` answers two questions — is the compiler pinned, and does
# `scripts/ci-gates.sh` run what CI runs — and a checker that only ever prints OK is
# indistinguishable from one that looks at nothing. Every hole found in it so far was found by
# breaking it on purpose and NONE was found by reading it:
#
#   * the reading-step check accepted the COMMENT above the step, so deleting the step left
#     `toolchain: ${{ env.RUST_CHANNEL }}` expanding to the empty string -- which
#     `dtolnay/rust-toolchain` reads as "not given" and answers with its `stable` default -- and
#     the check said PIN OK because the words `rust-toolchain.toml` still appeared in the prose.
#   * parity compared with `in`, so `cargo doc --no-deps` "matched" a CI that ran
#     `cargo doc --no-deps --document-private-items`.
#   * parity only ever walked ONE direction. Adding `cargo audit` to ci.yaml, and deleting
#     `gate_doc` from the script, both left it green.
#
# This file applies each break, asserts the check goes RED and names the violation, restores, and
# finally asserts the untouched tree is GREEN. Seventeen breaks; any one of them passing is a hole.
#
#   bash scripts/ci-gates-selftest.sh
#
# Exit 0 = every break was caught and the restored tree is clean. Exit 1 = at least one break was
# NOT caught (a hole, printed as `*** HOLE ***`). Exit 3 = the fixture itself is broken.
#
# **It rewrites the working tree and restores it with `git checkout -- .`, so it REFUSES to run on
# a dirty tree.** An earlier version of this harness, run with uncommitted work present, threw that
# work away. It also hard-codes no line numbers: the first version did, and the moment a step was
# inserted into ci.yaml five breaks silently stopped being applied and were reported as holes. A
# fixture that cannot find its target is not a passing test, so a failed lookup exits 3.

set -uo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 3
CI=.github/workflows/ci.yaml
CHECK=scripts/ci-toolchain-pin-check.py

if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    echo "REFUSING: the working tree is dirty." >&2
    echo "  This harness edits tracked files and restores them with \`git checkout -- .\`, which" >&2
    echo "  would discard your uncommitted work. Commit or stash first." >&2
    git status --short >&2
    exit 3
fi
git rev-parse --git-dir >/dev/null 2>&1 || { echo "REFUSING: not a git work tree" >&2; exit 3; }

HOLES=0
CAUGHT=0
lineno() {
    local n
    n=$(grep -n -- "$1" "$CI" | head -1 | cut -d: -f1)
    [ -n "$n" ] || { echo "FIXTURE BROKEN: no line in $CI matches: $1" >&2; exit 3; }
    echo "$n"
}
restore() { git checkout -- . ; rm -f .github/workflows/extra.yaml; rm -rf scripts/__pycache__; }

# **In-place edits go through python3, not `sed -i`.** That flag's spelling is incompatible
# between BSD (macOS, which wants `-i ''`) and GNU (CI's ubuntu, where `''` is read as the
# script), and getting it wrong is silent in whichever direction it is wrong: written as
# `-i'' -e`, BSD sed took `-e` as the BACKUP SUFFIX and left `ci.yaml-e` and four more beside the
# files it edited. This harness's own closing "did I restore the tree" check is what caught that.
# python3 is already a hard dependency here -- the thing under test is a python script.
py_del()  { python3 -c 'import sys;f,a,b=sys.argv[1],int(sys.argv[2]),int(sys.argv[3]);L=open(f).read().splitlines(True);open(f,"w").write("".join(L[:a-1]+L[b:]))' "$@"; }
py_sub()  { python3 -c 'import sys;f,n,t=sys.argv[1],int(sys.argv[2]),sys.argv[3];L=open(f).read().splitlines(True);L[n-1]=t+"\n";open(f,"w").write("".join(L))' "$@"; }
py_repl() { python3 -c 'import sys;f,a,b=sys.argv[1],sys.argv[2],sys.argv[3];s=open(f).read();open(f,"w").write(s.replace(a,b))' "$@"; }
py_drop() { python3 -c 'import sys;f,pre=sys.argv[1],sys.argv[2];L=[l for l in open(f).read().splitlines(True) if not l.startswith(pre)];open(f,"w").write("".join(L))' "$@"; }
run() {
    local name="$1" expect="$2"; shift 2
    local out rc got
    out=$(python3 "$CHECK" "$@" 2>&1); rc=$?
    got=OK; [ $rc -ne 0 ] && got=FAIL
    if [ "$got" = "$expect" ]; then
        CAUGHT=$((CAUGHT + 1))
        printf '  %-54s [%s as expected]\n' "$name" "$got"
        [ "$got" = FAIL ] && echo "$out" | grep -E "VIOLATION|FAILED" | head -1 | sed 's/^ */        /'
    else
        HOLES=$((HOLES + 1))
        printf '  *** HOLE *** %-42s expected %s, got %s\n' "$name" "$expect" "$got"
    fi
}

TC=$(lineno '          toolchain: ${{ env.RUST_CHANNEL }}')
EXPORT=$(lineno 'echo "RUST_CHANNEL=$channel" >> "$GITHUB_ENV"')
STEP_START=$(lineno '      - name: Read the pinned toolchain')
STEP_END=$(lineno 'echo "pinned Rust toolchain: $channel"')
echo "fixture: $CI toolchain: line $TC, export line $EXPORT, reading step $STEP_START-$STEP_END"

echo "===== PIN ====="
run "0  untouched tree" OK
mv rust-toolchain.toml /tmp/ci-gates-selftest-rtt.bak
run "1a rust-toolchain.toml deleted" FAIL
mv /tmp/ci-gates-selftest-rtt.bak rust-toolchain.toml
py_repl rust-toolchain.toml 'channel = "1.93.0"' 'channel = "stable"'
run '1b channel = "stable" -- a floating channel' FAIL; restore
py_del "$CI" "$TC" "$TC";                         run "1c toolchain: line gone -> action default" FAIL; restore
py_del "$CI" "$STEP_START" "$STEP_END";           run "1d reading step deleted, its comment kept" FAIL; restore
py_sub "$CI" "$EXPORT" '          # echo "RUST_CHANNEL=$channel"'
run "1e the export line commented out" FAIL; restore
py_sub "$CI" "$TC" '          toolchain: stable';            run "1f one job installs stable" FAIL; restore
py_sub "$CI" "$TC" '          toolchain: 1.90.0';            run "1g a literal that disagrees with the toml" FAIL; restore
for f in .github/workflows/*.yaml; do py_repl "$f" 'uses: dtolnay/rust-toolchain@' 'uses: other/setup@'; done
run "1h no job installs a toolchain at all" FAIL; restore
cat > .github/workflows/extra.yaml <<'YML'
name: Extra
on: [push]
jobs:
  extra:
    runs-on: ubuntu-latest
    steps:
      - uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8
        with:
          toolchain: stable
YML
run "1i a NEW workflow file installs stable" FAIL; restore

echo "===== PARITY, script -> ci.yaml ====="
run "0  untouched tree" OK --parity
DOC=$(lineno '        run: cargo doc --no-deps')
py_sub "$CI" "$DOC" '        run: cargo doc --no-deps --document-private-items'
run "2a CI adds a flag; the gate is a prefix" FAIL --parity; restore
DT=$(lineno '        run: cargo test --doc$')
py_sub "$CI" "$DT" '        # run: cargo test --doc -- disabled'
run "2b gate commented out, comment still names it" FAIL --parity; restore
printf '\ngate_newthing()      { cargo nonexistent-subcommand; }\n' >> scripts/ci-gates.sh
run "2c script offers a gate CI does not run" FAIL --parity; restore

echo "===== PARITY, ci.yaml -> script ====="
python3 - <<'PY'
p = ".github/workflows/ci.yaml"
lines = open(p).read().splitlines()
i = lines.index("        run: cargo doc --no-deps")
lines[i + 1:i + 1] = ["      - name: Run cargo audit", "        run: cargo audit --deny warnings"]
open(p, "w").write("\n".join(lines) + "\n")
PY
run "2d ci.yaml grows a gate the script lacks" FAIL --parity; restore
py_drop scripts/ci-gates.sh 'gate_doc()'
run "2e script drops a gate CI still runs" FAIL --parity; restore
py_drop scripts/ci-gates.sh 'gate_derive_suite()'
run "2f the derive-suite gate is dropped" FAIL --parity; restore
python3 - <<'PY'
p = "scripts/ci-toolchain-pin-check.py"
s = open(p).read()
s = s.replace('    "cargo clippy -p kaspa-wasm --target wasm32-unknown-unknown":\n'
              '        "Check WASM32 -- needs `rustup target add wasm32-unknown-unknown`",\n', "")
open(p, "w").write(s)
PY
run "2g an excuse is deleted from CI_ONLY" FAIL --parity; restore

echo "===== the restored tree ====="
run "PIN" OK
run "PARITY" OK --parity

DIRTY="$(git status --porcelain)"
echo
echo "==============================================================="
printf 'ci-gates-selftest: %d break(s) caught, %d hole(s)\n' "$CAUGHT" "$HOLES"
if [ -n "$DIRTY" ]; then
    echo "ci-gates-selftest: RED -- the harness did not restore the tree:" >&2
    echo "$DIRTY" >&2
    exit 1
fi
echo "==============================================================="
[ "$HOLES" -eq 0 ] || { echo "ci-gates-selftest: RED -- $HOLES break(s) were NOT caught." >&2; exit 1; }
echo "ci-gates-selftest: GREEN -- every break was caught by name, and the tree is clean."
