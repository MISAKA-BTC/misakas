#!/usr/bin/env bash
#
# **The gates CI runs, run here.** One command, `bash scripts/ci-gates.sh`.
#
# The failure this exists to prevent, in the words of the day it was written: the first job in
# `.github/workflows/ci.yaml` had been red for an unknown length of time and nobody knew, because
# everybody runs `cargo test` and nobody runs the job that runs first. `./check` — the command
# CONTRIBUTING.md tells contributors to run before a PR — could not have told them: it ran
# `cargo fmt --all` (which REWRITES the tree instead of failing) and threw that exit code away,
# and it ran clippy with `--workspace`, which overrides `default-members` and dies in
# `misaka-palw-worker`'s build script before linting anything.
#
# Three rules this script is built around, each of which this repository has been burned by:
#
#   1. **An exit code is not a result.** Every gate's log is searched for the evidence that the
#      gate actually RAN — a nextest `Summary [...]` line, a `test result:` line, a `Finished`.
#      A suite that exits 0 having compiled nothing fails here. `verify-by-reading-the-log-not-
#      the-exit-code` is a real incident in this tree's history, not a slogan.
#   2. **A checker that prints its verdict without printing its coverage is unfalsifiable.**
#      Every gate prints WHAT it checked, and the summary prints one line per gate with its own
#      exit code — not a single aggregate PASS.
#   3. **No pipes around a gate.** A wrapper that exited 0 while four suites were red happened
#      three times in one week here, every time because the exit code read was `tail`'s. Nothing
#      below pipes a gate's command anywhere: output is redirected to a log file, and the log is
#      read afterwards. `pipefail` is set as a belt on top of that, not instead of it.
#
# Usage:
#   bash scripts/ci-gates.sh                # every gate, in CI's own order
#   bash scripts/ci-gates.sh --list         # what the gates are and which CI job each mirrors
#   bash scripts/ci-gates.sh fmt clippy     # only these
#   bash scripts/ci-gates.sh --group fast   # the gates that need no cargo build
#
# Exit status: 0 only if every selected gate passed. Otherwise the number of failed gates
# (capped at 120). Logs are kept under $CI_GATES_LOG_DIR (default: target/ci-gates).
#
# Portability: bash 3.2 (macOS's) and bash 5 (Linux runners). No `timeout` — it does not exist on
# macOS. No GNU-only flags.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 125
LOG_DIR="${CI_GATES_LOG_DIR:-$REPO_ROOT/target/ci-gates}"
mkdir -p "$LOG_DIR" || exit 125

# The PALW fixture tag family is model-free proof-of-work, honoured on devnet only
# (`kaspa_pow::palw::fixture_permitted_on`). ci.yaml sets it for the same reason: CI has no 1.2 GB
# LLM to replay. Exported once here so every gate below sees exactly what the CI job sees.
export MISAKA_PALW_POW_FIXTURE="${MISAKA_PALW_POW_FIXTURE:-1}"

# ---------------------------------------------------------------------------------------------
# Gate table.  id | group | ci job it mirrors | one-line description
#
# `fast` is everything that needs no cargo build: it runs in well under a minute on a cold tree
# and is what a pre-commit hook or an impatient human should run. `build` needs a toolchain and
# a compiled workspace.
# ---------------------------------------------------------------------------------------------
GATE_IDS="toolchain-pin workflow-parity artifact-conformance artifact-stranger artifact-roundtrip artifact-thirdparty fmt clippy pq-guard check nextest doctest hashes-no-asm doc"

gate_group() {
    case "$1" in
        toolchain-pin|workflow-parity|artifact-*) echo fast ;;
        *) echo build ;;
    esac
}

gate_job() {
    case "$1" in
        toolchain-pin)        echo "Toolchain pin" ;;
        workflow-parity)      echo "Gates (parity)" ;;
        artifact-*)           echo "Derived artifacts" ;;
        fmt|clippy|pq-guard)  echo "Lints" ;;
        check)                echo "Check" ;;
        *)                    echo "Test Suite" ;;
    esac
}

gate_desc() {
    case "$1" in
        toolchain-pin)        echo "rust-toolchain.toml exists, and no workflow job installs a floating toolchain" ;;
        workflow-parity)      echo "every build gate below is spelled the same way in ci.yaml" ;;
        artifact-conformance) echo "the format checker refuses five specific injuries BY NAME" ;;
        artifact-stranger)    echo "the Python re-derivation agrees with the shipped Rust's own oracles" ;;
        artifact-roundtrip)   echo "artifacts re-derived in Python are then opened by the format checker" ;;
        artifact-thirdparty)  echo "foreign parsers (mido, numpy-stl) agree with the DSL about MEANING" ;;
        fmt)                  echo "cargo fmt --all -- --check" ;;
        clippy)               echo "cargo clippy --tests --benches --examples -- -D warnings" ;;
        pq-guard)             echo "secp256k1 isolation + cargo-deny advisories (ADR-0019 SS14)" ;;
        check)                echo "cargo check --tests --benches" ;;
        nextest)              echo "cargo nextest run" ;;
        doctest)              echo "cargo test --doc" ;;
        hashes-no-asm)        echo "kaspa-hashes without asm, lib + benches + doctests" ;;
        doc)                  echo "cargo doc --no-deps" ;;
    esac
}

# ---------------------------------------------------------------------------------------------
# The runner.
#
# `run_gate <id> <evidence-regex> -- <argv...>`
#
# `evidence-regex` is what the log MUST contain for the gate to count as having run. An empty
# regex means "no output is the pass" (rustfmt). This is rule 1: `cargo nextest run` that
# selected zero tests exits 0, and a gate that accepts that is measuring nothing.
# ---------------------------------------------------------------------------------------------
FAILED=0
PASSED=0
SUMMARY=""

run_gate() {
    local id="$1"; shift
    local evidence="$1"; shift
    [ "$1" = "--" ] && shift
    local log="$LOG_DIR/$id.log"

    printf '\n=== gate %-20s [%s]  %s\n' "$id" "$(gate_job "$id")" "$(gate_desc "$id")"
    printf '    $ %s\n' "$*"

    # No pipe. The command's stdout and stderr go to the log; the exit code is the command's own.
    "$@" >"$log" 2>&1
    local rc=$?

    local verdict="ok"
    if [ "$rc" -ne 0 ]; then
        verdict="FAILED rc=$rc"
    elif [ -n "$evidence" ] && ! grep -Eq "$evidence" "$log"; then
        # Rule 1: exit 0 with no evidence in the log is a failure, not a pass.
        verdict="FAILED rc=0 but the log never matched /$evidence/ -- this gate did not run"
        rc=1
    fi

    gate_coverage "$id" "$log"

    if [ "$rc" -eq 0 ]; then
        PASSED=$((PASSED + 1))
        printf '    -> %s  (log: %s)\n' "$verdict" "${log#"$REPO_ROOT"/}"
    else
        FAILED=$((FAILED + 1))
        printf '    -> %s  (log: %s)\n' "$verdict" "${log#"$REPO_ROOT"/}"
        printf '    ---- last 40 lines ----\n'
        tail -40 "$log" | sed 's/^/    | /'
        printf '    -----------------------\n'
    fi
    SUMMARY="$SUMMARY$(printf '\n  %-22s %s' "$id" "$verdict")"
    return $rc
}

# Rule 2: say what was covered, from the log, for every gate — passing or failing.
gate_coverage() {
    local id="$1" log="$2" n
    case "$id" in
        fmt)
            n=$(grep -c '^Diff in ' "$log" 2>/dev/null || true)
            printf '    coverage: rustfmt %s over the whole workspace; %s file(s) differ\n' \
                   "$(cargo fmt --version 2>/dev/null | head -1)" "${n:-0}"
            ;;
        clippy)
            n=$(grep -cE '^(warning|error)(\[|:)' "$log" 2>/dev/null || true)
            printf '    coverage: %s; %s diagnostic line(s); %s crate(s) linted\n' \
                   "$(cargo clippy --version 2>/dev/null | head -1)" "${n:-0}" \
                   "$(grep -cE '^ +(Checking|Compiling) ' "$log" 2>/dev/null || echo 0)"
            ;;
        check)
            printf '    coverage: %s crate(s) checked\n' "$(grep -cE '^ +(Checking|Compiling) ' "$log" 2>/dev/null || echo 0)"
            ;;
        nextest|hashes-no-asm)
            grep -E '^ +Summary ' "$log" 2>/dev/null | sed 's/^ */    coverage: /' || true
            ;;
        doctest)
            printf '    coverage: %s\n' "$(grep -c '^test result:' "$log" 2>/dev/null || echo 0) doctest binaries reported a 'test result:' line"
            ;;
        artifact-*|toolchain-pin|workflow-parity)
            # These print their own coverage; echo the lines that carry it. Indentation varies
            # between the scripts, so the anchors are the words, not the columns -- an earlier
            # version of this pattern required two leading spaces and silently printed nothing
            # for the one gate whose output has none.
            grep -E '^[[:space:]]*(refused|OK|AGREE|SKIP|WRONG|FAIL)[[:space:]]|^[[:space:]]*(selftest:|libraries:|interpreter:|covered kinds:|checked [0-9]|PIN OK|PARITY OK|rust-toolchain\.toml pins|SELFTEST FAILED|PIN FAILED|PARITY FAILED)|^[0-9]+ agreed' "$log" 2>/dev/null \
                | sed 's/^/    /' || true
            ;;
    esac
}

# ---------------------------------------------------------------------------------------------
# The fast gates.
# ---------------------------------------------------------------------------------------------

# `rust-toolchain.toml` is the pin; this asserts the pin exists AND that nothing in
# `.github/workflows/` quietly installs something else. `dtolnay/rust-toolchain`'s `toolchain`
# input defaults to `stable` (checked in the pinned action's own action.yml), so a job that omits
# it installs whatever `stable` was that morning -- the skew that produced six consecutive CI
# reds read as green in this repository's own record.
gate_toolchain_pin() {
    python3 "$REPO_ROOT/scripts/ci-toolchain-pin-check.py"
}

# The gate that keeps THIS FILE honest: a command here that CI does not run is a lie, and a
# command CI runs that is not here is a gate nobody can run before pushing. Checked by string.
gate_workflow_parity() {
    python3 "$REPO_ROOT/scripts/ci-toolchain-pin-check.py" --parity
}

ARTIFACT_DIR="$LOG_DIR/artifacts"

gate_artifact_conformance() {
    python3 "$REPO_ROOT/scripts/misaka-palw-artifact-conformance.py" selftest
}

gate_artifact_stranger() {
    python3 "$REPO_ROOT/scripts/misaka-palw-derive-stranger.py" \
        --crate-root "$REPO_ROOT/misaka-palw-derive" selftest
}

# The two selftests above prove the checkers REFUSE damage. Neither proves either one ACCEPTS a
# real artifact, and a checker that refuses everything is as useless as one that accepts
# everything. So: re-derive real artifacts with the independent Python transformer (no cargo, no
# Rust binary -- this gate keeps working on a tree that does not compile), then open them with
# the format checker.
#
# Coverage, stated because a silent gap is the failure mode: the stranger implements
# `music/smf/v1` and the axis-aligned subset of `cad/stl/v1`. It does NOT implement
# `scene/glb/v1`, so no GLB reaches the checker here. The glTF path is covered by
# `artifact-conformance`'s two GLB injuries (one of them the real OPAQUE/alpha defect) and, for
# real files, by `misaka-palw-artifact-conformance.py demo --derive-bin`, which needs the Rust
# binary and is not run in CI.
gate_artifact_roundtrip() {
    rm -rf "$ARTIFACT_DIR"
    mkdir -p "$ARTIFACT_DIR" || return 1
    local rc=0
    python3 "$REPO_ROOT/scripts/misaka-palw-derive-stranger.py" \
        --crate-root "$REPO_ROOT/misaka-palw-derive" derive \
        --transformer music/smf/v1 \
        --answer "$REPO_ROOT/misaka-palw-derive/corpus/music/02-two-track-chords.json" \
        --out "$ARTIFACT_DIR" >/dev/null || rc=1
    python3 "$REPO_ROOT/scripts/misaka-palw-derive-stranger.py" \
        --crate-root "$REPO_ROOT/misaka-palw-derive" derive \
        --transformer cad/stl/v1 \
        --answer "$REPO_ROOT/misaka-palw-derive/corpus/cad/05-boolean-box-with-a-notch.json" \
        --out "$ARTIFACT_DIR" >/dev/null || rc=1
    [ "$rc" -eq 0 ] || { echo "the stranger could not re-derive the corpus answers"; return 1; }

    # Assert the coverage rather than assuming it: if the derive step silently produced nothing,
    # `check` over an empty list would have nothing to refuse.
    local mids stls
    mids=$(find "$ARTIFACT_DIR" -name '*.mid' | wc -l | tr -d ' ')
    stls=$(find "$ARTIFACT_DIR" -name '*.stl' | wc -l | tr -d ' ')
    echo "covered kinds: $mids MIDI, $stls STL, 0 GLB (the stranger does not implement scene/glb/v1)"
    [ "$mids" -ge 1 ] && [ "$stls" -ge 1 ] || { echo "expected at least one .mid and one .stl"; return 1; }

    python3 "$REPO_ROOT/scripts/misaka-palw-artifact-conformance.py" check \
        "$ARTIFACT_DIR"/*.mid "$ARTIFACT_DIR"/*.stl
}

# `--require` is the whole point: without it a machine with no `mido` prints SKIP and exits 0,
# and a green run would mean "nothing was learned". With it, a missing library is a failure.
gate_artifact_thirdparty() {
    local py="${CI_GATES_THIRDPARTY_PYTHON:-python3}"
    if [ ! -d "$ARTIFACT_DIR" ]; then
        echo "artifact-roundtrip did not run; there is nothing for a foreign parser to open"
        return 1
    fi
    echo "interpreter: $py ($("$py" -c 'import sys;print(sys.version.split()[0])' 2>/dev/null))"
    # Name the versions: an oracle whose version is unknown reports an unknown opinion. (`mido`
    # has no `__version__` attribute -- the distribution metadata is the only spelling that
    # works for both of these.)
    "$py" -c 'import importlib.metadata as md, mido, stl
print("libraries: mido", md.version("mido"), "/ numpy-stl", md.version("numpy-stl"))' || {
        echo "install them first:  python3 -m pip install mido numpy-stl pygltflib"
        echo "or point CI_GATES_THIRDPARTY_PYTHON at an interpreter that has them"
        return 1
    }
    "$py" "$REPO_ROOT/scripts/misaka-palw-artifact-thirdparty.py" --require "$ARTIFACT_DIR"
}

# ---------------------------------------------------------------------------------------------
# The build gates -- byte-for-byte the `run:` lines of ci.yaml's Lints / Check / Test Suite jobs.
# `workflow-parity` above fails if any of these strings stops appearing in ci.yaml.
# ---------------------------------------------------------------------------------------------
gate_fmt()           { cargo fmt --all -- --check; }
gate_clippy()        { cargo clippy --tests --benches --examples -- -D warnings; }
gate_pq_guard()      { bash scripts/pq-ci-guard.sh; }
gate_check()         { cargo check --tests --benches; }
gate_nextest()       { cargo nextest run; }
gate_doctest()       { cargo test --doc; }
gate_hashes_no_asm() { cargo nextest run -p kaspa-hashes --features=no-asm --benches; }
gate_doc()           { cargo doc --no-deps; }

dispatch() {
    case "$1" in
        toolchain-pin)        run_gate "$1" 'PIN OK'                    -- gate_toolchain_pin ;;
        workflow-parity)      run_gate "$1" 'PARITY OK'                 -- gate_workflow_parity ;;
        artifact-conformance) run_gate "$1" 'every injury refused'      -- gate_artifact_conformance ;;
        artifact-stranger)    run_gate "$1" 'checks agree|SELFTEST'     -- gate_artifact_stranger ;;
        artifact-roundtrip)   run_gate "$1" 'covered kinds:'            -- gate_artifact_roundtrip ;;
        artifact-thirdparty)  run_gate "$1" 'agreed,'                   -- gate_artifact_thirdparty ;;
        fmt)                  run_gate "$1" ''                          -- gate_fmt ;;
        clippy)               run_gate "$1" 'Finished|Checking'         -- gate_clippy ;;
        pq-guard)             run_gate "$1" '.'                         -- gate_pq_guard ;;
        check)                run_gate "$1" 'Finished|Checking'         -- gate_check ;;
        nextest)              run_gate "$1" 'Summary \['                -- gate_nextest ;;
        doctest)              run_gate "$1" 'test result:'              -- gate_doctest ;;
        hashes-no-asm)        run_gate "$1" 'Summary \['                -- gate_hashes_no_asm ;;
        doc)                  run_gate "$1" 'Finished|Generated|Documenting' -- gate_doc ;;
        *) echo "unknown gate '$1'; try --list" >&2; return 125 ;;
    esac
}

usage_list() {
    printf '%-22s %-8s %-18s %s\n' GATE GROUP "CI JOB" DESCRIPTION
    local g
    for g in $GATE_IDS; do
        printf '%-22s %-8s %-18s %s\n' "$g" "$(gate_group "$g")" "$(gate_job "$g")" "$(gate_desc "$g")"
    done
    printf '\nNot covered here (they need a runner this is not): Check (x86_64-pc-windows-msvc),\n'
    printf 'Check no_std, Check/Test/Build WASM32, Build Linux Release (musl).\n'
}

SELECTED=""
while [ $# -gt 0 ]; do
    case "$1" in
        --list) usage_list; exit 0 ;;
        --group)
            shift
            [ $# -gt 0 ] || { echo "--group needs fast|build|all" >&2; exit 125; }
            for g in $GATE_IDS; do
                if [ "$1" = all ] || [ "$(gate_group "$g")" = "$1" ]; then SELECTED="$SELECTED $g"; fi
            done
            [ -n "$SELECTED" ] || { echo "no gates in group '$1'" >&2; exit 125; }
            ;;
        -h|--help) usage_list; exit 0 ;;
        -*) echo "unknown option $1" >&2; exit 125 ;;
        *) SELECTED="$SELECTED $1" ;;
    esac
    shift
done
[ -n "$SELECTED" ] || SELECTED="$GATE_IDS"

echo "ci-gates: $REPO_ROOT"
echo "ci-gates: toolchain $(rustc --version 2>/dev/null || echo '<no rustc>')"
echo "ci-gates: gates ->$SELECTED"

for g in $SELECTED; do
    dispatch "$g"
done

# Rule 2 again: one line per gate with its own exit code, never a single aggregate word.
printf '\n===============================================================\n'
printf 'ci-gates summary (%d passed, %d FAILED)%s\n' "$PASSED" "$FAILED" "$SUMMARY"
printf '===============================================================\n'
if [ "$FAILED" -ne 0 ]; then
    printf 'ci-gates: RED -- %d gate(s) failed. Logs in %s\n' "$FAILED" "${LOG_DIR#"$REPO_ROOT"/}"
    [ "$FAILED" -gt 120 ] && FAILED=120
    exit "$FAILED"
fi
printf 'ci-gates: GREEN -- %d gate(s) passed.\n' "$PASSED"
exit 0
