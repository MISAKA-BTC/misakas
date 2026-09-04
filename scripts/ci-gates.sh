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
GATE_IDS="toolchain-pin workflow-parity artifact-conformance artifact-stranger artifact-roundtrip artifact-thirdparty fmt clippy pq-guard check build-devnet-prealloc check-evm-send nextest derive-suite doctest hashes-no-asm doctest-hashes-no-asm doc example-kip10"

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
        build-devnet-prealloc) echo "the devnet-prealloc feature compiles, tests and benches included" ;;
        check-evm-send)       echo "misaka-cli's evm-send feature compiles" ;;
        nextest)              echo "cargo nextest run" ;;
        derive-suite)         echo "misaka-palw-derive WITH its binaries — the confinement gate needs palw-evm-runner" ;;
        doctest)              echo "cargo test --doc" ;;
        hashes-no-asm)        echo "kaspa-hashes without asm, lib + benches" ;;
        doctest-hashes-no-asm) echo "kaspa-hashes without asm, doctests" ;;
        doc)                  echo "cargo doc --no-deps" ;;
        example-kip10)        echo "the kip-10 example still runs under legacy-secp256k1" ;;
    esac
}

# ---------------------------------------------------------------------------------------------
# The runner.
#
# `run_gate <id> <evidence> -- <argv...>`
#
# `evidence` is what the log MUST contain for the gate to count as having run: one or more
# extended regexes joined by `@@`, ALL of which must match. An empty evidence means "no output is
# the pass" (rustfmt). This is rule 1: `cargo nextest run` that selected zero tests exits 0, and a
# gate that accepts that is measuring nothing.
#
# `@@` exists because one regex is not enough for the checkers that refuse a LIST of things. The
# conformance selftest refuses five named injuries; an evidence of `every injury refused` (its
# last line) passes a build in which one of the five was quietly dropped -- the checker's coverage
# shrinks and no number a reader sees shrinks with it. So the five names ARE the evidence. The
# same failure was seen elsewhere in this tree today: a fuzz gate reported `gate_refused 400/400`
# while gating nothing, and only a tally assertion caught it.
#
# The count of required patterns is printed on every run, passing or failing (rule 2).
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
    local ev rest found=0 want=0 missing=""
    rest="$evidence"
    while [ -n "$rest" ]; do
        case "$rest" in
            *"@@"*) ev="${rest%%@@*}"; rest="${rest#*@@}" ;;
            *)      ev="$rest";       rest="" ;;
        esac
        want=$((want + 1))
        if grep -Eq "$ev" "$log"; then
            found=$((found + 1))
        else
            missing="$missing$(printf '\n    MISSING EVIDENCE  /%s/' "$ev")"
        fi
    done
    [ "$want" -gt 0 ] && printf '    evidence: %d of %d required pattern(s) found in the log\n' "$found" "$want"

    if [ "$rc" -ne 0 ]; then
        verdict="FAILED rc=$rc"
    elif [ -n "$missing" ]; then
        # Rule 1: exit 0 with the evidence absent is a failure, not a pass.
        verdict="FAILED rc=0 and $((want - found)) of $want required pattern(s) never appeared -- this gate did not run, or ran less than it claims"
        printf '%s\n' "$missing"
        rc=1
    fi

    gate_coverage "$id" "$log"
    expected_reds "$rc" "$log"
    missing_tool_hint "$log"

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

# **The reds that are somebody else's, named rather than tolerated.**
#
# Three failures in this tree close at the cut's SINGLE `transformer_id` / preset re-pin, which
# belongs to the integrator. A contributor who "fixes" one of them moves a published derivation's
# id a second time. They are still FAILURES here -- nothing below suppresses a red -- but a reader
# who cannot tell a known red from a new one ends up ignoring the colour, which is how a gate goes
# quiet without going green.
#
# `palw_freeprompt_v3::tests::golden_vector_ids_are_frozen` is matched by its MODULE PATH: there is
# a namesake `golden_vector_ids_are_frozen` in `palw_derived_v1.rs` that is NOT expected to fail,
# and matching the bare name would excuse it.
#
# **Matched on a FAILING line, and only for a gate that failed** (mainnet audit, 2026-09-05).
# `grep -q <name>` matches cargo-nextest's `PASS` line just as well as its `FAIL` line, and this
# ran unconditionally -- before the `rc` check -- so a completely green run printed
# "known red: shipped_presets_have_pinned_fingerprints" every time, and a red run could name a
# test that had in fact passed. A reader who cannot tell a known red from a new one ends up
# ignoring the colour, which is the exact failure this function's own comment names.
expected_reds() {
    local rc="$1" log="$2"
    # A gate that passed has no reds to explain; saying otherwise trains the reader to skip the line.
    [ "$rc" -eq 0 ] && return 0
    # `FAIL`/`FAILED` on the SAME line as the name: nextest prints `FAIL [ 0.1s] <path>`, libtest
    # prints `test <path> ... FAILED`, so either order is matched and a PASS line is not.
    known_red() {
        grep -qE "FAIL(ED)?.*$1|$1.*FAILED" "$log" 2>/dev/null
    }
    known_red "shipped_presets_have_pinned_fingerprints" &&
        printf '    known red: shipped_presets_have_pinned_fingerprints -- closes at the cut'"'"'s single re-pin (integrator)\n'
    known_red "palw_freeprompt_v3::tests::golden_vector_ids_are_frozen" &&
        printf '    known red: palw_freeprompt_v3::tests::golden_vector_ids_are_frozen -- same re-pin. NOT the namesake in palw_derived_v1.rs\n'
    grep -qE "source_tree_sha256.*MISMATCH|MISMATCH.*source_tree_sha256" "$log" 2>/dev/null &&
        printf '    known red: the transformer_id pins -- a87cc282 rustfmt-ed misaka-palw-derive/src/kinds/scene.rs, which moved all eight ids. Same re-pin.\n'
    known_red "the_transformer_ids_are_the_ones_this_build_was_pinned_with" &&
        printf '    known red: transformer_id_pin::the_transformer_ids_are_the_ones_this_build_was_pinned_with -- the Rust half of the same re-pin.\n'
    return 0
}

# A gate that fails because a TOOL is absent is still a failure -- "skipped" is how a gate goes
# quiet -- but it should say so in one line instead of leaving a cargo error to be decoded. CI
# installs these with `taiki-e/install-action`; a laptop usually has not.
missing_tool_hint() {
    local log="$1"
    if grep -q "no such command: \`\?nextest" "$log" 2>/dev/null || grep -q "no such command: .nextest" "$log" 2>/dev/null; then
        printf '    missing tool: cargo-nextest. Install it with `cargo install cargo-nextest --locked`\n'
        printf '                  (CI installs it with taiki-e/install-action). This is a FAILED gate, not a skip.\n'
    fi
    if grep -q "no such command: .deny" "$log" 2>/dev/null; then
        printf '    missing tool: cargo-deny. Install it with `cargo install cargo-deny --locked`\n'
        printf '                  (CI installs it with taiki-e/install-action). This is a FAILED gate, not a skip.\n'
    fi
}

# Rule 2: say what was covered, from the log, for every gate — passing or failing.
gate_coverage() {
    local id="$1" log="$2" n
    # Note for anyone adding a counter here: `grep -c` prints `0` AND exits 1 on no match, so
    # `|| echo 0` produces the two-line string "0\n0". Use `|| true`, and guard for empty.
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
                   "$(grep -cE '^ +(Checking|Compiling) ' "$log" 2>/dev/null || true)"
            ;;
        check)
            printf '    coverage: %s crate(s) checked\n' "$(grep -cE '^ +(Checking|Compiling) ' "$log" 2>/dev/null || true)"
            ;;
        pq-guard)
            # This one already names each of its six sub-gates; echo them rather than reducing
            # six answers to one word.
            grep -E '^(== \[|OK:|advisories |PQ CI guard)' "$log" 2>/dev/null | sed 's/^/    /' || true
            ;;
        doc)
            printf '    coverage: %s crate(s) documented\n' "$(grep -cE '^ +(Documenting|Compiling|Checking) ' "$log" 2>/dev/null || true)"
            ;;
        nextest)
            grep -E '^ +Summary ' "$log" 2>/dev/null | sed 's/^ */    coverage: /' || true
            # The genesis card's SS6 table cites four suites by crate. All four are in
            # `default-members` (Cargo.toml:126,135,139,147), so `cargo nextest run` runs them --
            # but "it should be covered" is the claim this project keeps being wrong about, so
            # COUNT them out of the log instead of asserting it.
            #
            # The pattern matches nextest's per-test line, `    PASS [   0.011s] <crate> <test>`.
            # It was written on a machine with no cargo-nextest installed, so it is SELF-CHECKED
            # below rather than trusted: four zeros under a Summary that says tests ran means the
            # parse is wrong, and a coverage line that is wrong is worse than no coverage line.
            printf '    coverage: the card'"'"'s four cited suites, counted from this run:\n'
            local c n total=0
            for c in kaspa-consensus-core misaka-palw-base0 misaka-cli kaspad; do
                n=$(grep -cE "^ +(PASS|FAIL|TRY|SKIP|LEAK)[^ ]* +\[[^]]*\] +$c(::| )" "$log" 2>/dev/null || true)
                [ -n "$n" ] || n=0
                total=$((total + n))
                printf '      %-22s %s test(s)\n' "$c" "$n"
            done
            if [ "$total" -eq 0 ] && grep -qE '^ +Summary .*[1-9][0-9]* tests? run' "$log" 2>/dev/null; then
                printf '      UNVERIFIED: the Summary says tests ran and none were attributed to a crate --\n'
                printf '                  this breakdown could not be parsed from this nextest version'"'"'s output.\n'
                printf '                  Read the Summary line above; do NOT read these four zeros as coverage.\n'
            fi
            ;;
        derive-suite)
            printf '    coverage: %s suite(s) reported a test result line; %s binary target(s) built\n' \
                   "$(grep -c '^test result:' "$log" 2>/dev/null || true)" \
                   "$(grep -cE '^ +Running .*(palw-evm-runner|palw-derive)' "$log" 2>/dev/null || true)"
            ;;
        hashes-no-asm)
            grep -E '^ +Summary ' "$log" 2>/dev/null | sed 's/^ */    coverage: /' || true
            ;;
        doctest|doctest-hashes-no-asm)
            printf '    coverage: %s\n' "$(grep -c '^test result:' "$log" 2>/dev/null || true) doctest binaries reported a 'test result:' line"
            ;;
        build-devnet-prealloc|check-evm-send)
            printf '    coverage: %s crate(s) compiled\n' "$(grep -cE '^ +(Checking|Compiling) ' "$log" 2>/dev/null || true)"
            ;;
        example-kip10)
            grep -E '^ +Running ' "$log" 2>/dev/null | sed 's/^ */    coverage: ran /' || true
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
    # **The gate provisions its own oracles, or says the check did not run.**
    #
    # These three libraries are deliberately NOT workspace dependencies -- the whole value of this
    # gate is that they were written by people who have never seen this repository. That made the
    # gate depend on whoever happened to have installed them, so it was green on one machine and a
    # FAILURE everywhere else, which reads as a defect in the tree rather than an absent oracle.
    #
    # So: build a venv beside the target dir and install them. If that cannot be done -- no network,
    # no pip, a locked-down runner -- the gate SKIPS and says so, because a machine that cannot
    # fetch a parser has learned nothing about these artifacts either way, and reporting that as a
    # red sends a reader into code that is fine. A skip here is never a pass: it prints what did not
    # run, and `run_gate`'s evidence patterns will not match, so it cannot be mistaken for coverage.
    if ! "$py" -c 'import mido, stl' 2>/dev/null; then
        local venv="$REPO_ROOT/target/ci-gates/thirdparty-venv"
        if [ ! -x "$venv/bin/python" ]; then
            echo "provisioning the foreign parsers (they are not workspace dependencies, by design)"
            python3 -m venv "$venv" >/dev/null 2>&1 || {
                echo "SKIPPED: no venv could be created; the third-party check did NOT run"
                return 0
            }
            "$venv/bin/pip" install --quiet mido numpy-stl pygltflib >/dev/null 2>&1 || {
                echo "SKIPPED: the foreign parsers could not be installed; the check did NOT run"
                echo "  (offline runner? install them yourself: pip install mido numpy-stl pygltflib)"
                return 0
            }
        fi
        py="$venv/bin/python"
        echo "interpreter: $py (provisioned)"
    fi
    "$py" -c 'import importlib.metadata as md, mido, stl
print("libraries: mido", md.version("mido"), "/ numpy-stl", md.version("numpy-stl"))' || {
        echo "SKIPPED: the foreign parsers are not importable; the check did NOT run"
        return 0
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
# **No `--lib`, deliberately, and do not "optimise" it back in.**
# `--lib` builds no binaries, so `palw-evm-runner` is not on disk, and ADR-0079's confinement gate
# REFUSES rather than falling back in-process — seven reds that a `--lib` run hides and that mean
# the opposite of what they look like (they are the gate working, not the tree broken). The whole
# package is the unit that reproduces what the genesis card's SS6 table cites.
gate_derive_suite()  { cargo test -p misaka-palw-derive; }
gate_doctest()       { cargo test --doc; }
gate_hashes_no_asm() { cargo nextest run -p kaspa-hashes --features=no-asm --benches; }
gate_doc()           { cargo doc --no-deps; }

# Four gates the ci.yaml -> script direction of `workflow-parity` found the day it was written:
# judgments the Test Suite job makes that nobody could run before pushing. Each line below is
# the byte-identical `run:` line of ci.yaml, which is what keeps the parity check green.
gate_build_devnet_prealloc()  { cargo build --features devnet-prealloc --tests --benches; }
gate_check_evm_send()         { cargo check --locked -p misaka-cli --bin misaka --features evm-send; }
gate_doctest_hashes_no_asm()  { cargo test --doc -p kaspa-hashes --features=no-asm; }
gate_example_kip10()          { cargo run -p kaspa-txscript --example kip-10 --features legacy-secp256k1; }

dispatch() {
    case "$1" in
        toolchain-pin)        run_gate "$1" 'PIN OK'                    -- gate_toolchain_pin ;;
        workflow-parity)      run_gate "$1" 'PARITY OK'                 -- gate_workflow_parity ;;
        # The five injuries BY NAME, not the summary line: losing one shrinks the checker's
        # coverage and shrinks no number a reader sees.
        artifact-conformance) run_gate "$1" 'refused +midi: header declares 2 tracks@@refused +midi: end-of-track short of the chunk end@@refused +glb: declared total length is 4 bytes over@@refused +stl: count says 3, file holds 1@@refused +glb: alpha 0.5 with the OPAQUE default@@selftest: every injury refused by name' -- gate_artifact_conformance ;;
        # All four oracles, not just a verdict: oracle 1 is the id pins, 2 the corpus goldens, 3
        # that nothing derives unnamed, 4 the verify path round-tripped and then tampered with.
        artifact-stranger)    run_gate "$1" '== oracle 1:@@== oracle 2:@@== oracle 3:@@== oracle 4:@@dsl_hash\+artifact_hash MATCH@@a tampered artifact_hash is caught' -- gate_artifact_stranger ;;
        artifact-roundtrip)   run_gate "$1" 'covered kinds:@@[0-9]+ MIDI, [0-9]+ STL@@OK .*\.mid@@OK .*\.stl'  -- gate_artifact_roundtrip ;;
        # `--require` makes a missing library a red; the library VERSIONS must be in the log too,
        # or the oracle that answered is unknown.
        artifact-thirdparty)  run_gate "$1" 'libraries: mido [0-9]@@numpy-stl [0-9]@@AGREE .*mido:@@AGREE .*numpy-stl:@@[0-9]+ agreed, 0 disagreed' -- gate_artifact_thirdparty ;;
        fmt)                  run_gate "$1" ''                          -- gate_fmt ;;
        clippy)               run_gate "$1" 'Finished|Checking'         -- gate_clippy ;;
        pq-guard)             run_gate "$1" '.'                         -- gate_pq_guard ;;
        check)                run_gate "$1" 'Finished|Checking'         -- gate_check ;;
        build-devnet-prealloc) run_gate "$1" 'Finished|Compiling'        -- gate_build_devnet_prealloc ;;
        check-evm-send)       run_gate "$1" 'Finished|Checking'         -- gate_check_evm_send ;;
        nextest)              run_gate "$1" 'Summary \['                -- gate_nextest ;;
        derive-suite)         run_gate "$1" 'test result:@@running [0-9]+ test'  -- gate_derive_suite ;;
        doctest)              run_gate "$1" 'test result:'              -- gate_doctest ;;
        hashes-no-asm)        run_gate "$1" 'Summary \['                -- gate_hashes_no_asm ;;
        doctest-hashes-no-asm) run_gate "$1" 'test result:'             -- gate_doctest_hashes_no_asm ;;
        doc)                  run_gate "$1" 'Finished|Generated|Documenting' -- gate_doc ;;
        example-kip10)        run_gate "$1" 'Finished|Running'          -- gate_example_kip10 ;;
        # Rule 1 again, one level up: an unknown id used to print a line to stderr and return
        # 125, which NOTHING read -- the loop below ignored dispatch's status -- so `ci-gates.sh
        # clipy` (one p) printed `GREEN -- 0 gate(s) passed` and exited 0. A wrapper that exits 0
        # having run nothing is the exact failure this file was written against, and it was in
        # this file. It is a counted failure now, with a name, like any other.
        *)
            FAILED=$((FAILED + 1))
            SUMMARY="$SUMMARY$(printf '\n  %-22s %s' "$1" "FAILED: no such gate -- try --list")"
            printf '\n=== gate %-20s FAILED: no such gate. `bash scripts/ci-gates.sh --list` names them all.\n' "$1" >&2
            return 125
            ;;
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
    printf '\nMANUAL, declared in advance rather than left as a gap -- both need model artifacts on\n'
    printf 'disk that neither CI nor a clean checkout has, so no runner can honestly claim them:\n'
    printf '  palw-model-gate           the dense lane, against a real GGUF\n'
    printf '  palw-qwen36-model-gate    the QWEN36 lane, likewise\n'
    printf '  misaka-palw-artifact-conformance.py demo --derive-bin target/debug/palw-derive\n'
    printf '                            a REAL GLB read semantically; the CI gate covers glTF only\n'
    printf '                            through the selftest injuries (0 GLB is printed each run)\n'
    printf '\nKnown reds, all closing at the cut'"'"'s single re-pin (integrator, not a contributor):\n'
    printf '  shipped_presets_have_pinned_fingerprints\n'
    printf '  palw_freeprompt_v3::tests::golden_vector_ids_are_frozen  (NOT the palw_derived_v1.rs namesake)\n'
    printf '  artifact-stranger oracle 1 -- the eight transformer_id pins, moved by a87cc282\n'
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

RAN=0
for g in $SELECTED; do
    dispatch "$g"
    RAN=$((RAN + 1))
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
# A run that judged NOTHING is not a pass. `PASSED` and `RAN` disagreeing means a gate neither
# passed nor was counted as failed, which would be a bug in this script rather than in the tree --
# say so instead of printing a colour.
if [ "$PASSED" -eq 0 ] || [ "$PASSED" -ne "$RAN" ]; then
    printf 'ci-gates: RED -- %d gate(s) selected, %d ran, %d passed, %d failed. A green over zero\n' \
           "$(set -- $SELECTED; echo $#)" "$RAN" "$PASSED" "$FAILED"
    printf '          judgements is not a green; this is a bug in ci-gates.sh, not a verdict on the tree.\n'
    exit 124
fi
printf 'ci-gates: GREEN -- %d gate(s) passed, and %d were selected.\n' "$PASSED" "$RAN"
exit 0
