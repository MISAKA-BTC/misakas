#!/usr/bin/env bash
# The CI gates, runnable as ONE local command, in CI's order (t11 Relaunch 5f card §6/§7).
#
# "The gate you never ran locally is the gate you have never seen fail." CI's Lints job pins
# rustc 1.93.0 for fmt and clippy (.github/workflows/ci.yaml), so a local run on another
# toolchain measures a different lint set and a different formatter; this script refuses to run
# the two on anything else. Everything runs `-p` per crate — `--workspace` needs the vendored
# llama.cpp for misaka-palw-worker — and every test crate is the card's list, `--lib` only where
# the card says `--lib` (misaka-palw-derive is NOT --lib: its confinement gate needs the runner bin).
#
#   scripts/misaka-ci-gates.sh            # everything, stop at the first red
#   scripts/misaka-ci-gates.sh fmt clippy # a subset, by name
#
# Exit status is the first failing gate's; the log says which gate by name.
set -euo pipefail
cd "$(dirname "$0")/.."
PIN=1.93.0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export MISAKA_PALW_POW_FIXTURE="${MISAKA_PALW_POW_FIXTURE:-1}"

gate() { echo; echo "=== gate: $1 ==="; shift; "$@"; }
need_pin() {
  if ! rustup run "$PIN" rustc --version >/dev/null 2>&1; then
    echo "gate needs rustc $PIN (CI's Lints pin): rustup toolchain install $PIN --component rustfmt clippy" >&2
    exit 2
  fi
}

run_fmt()     { need_pin; gate fmt     cargo "+$PIN" fmt --all -- --check; }
run_clippy()  { need_pin; gate clippy  cargo "+$PIN" clippy --tests --benches --examples -- -D warnings; }
run_core()    { gate consensus-core cargo test -p kaspa-consensus-core --lib; }
run_base0()   { gate base0          cargo test -p misaka-palw-base0 --lib; }
run_derive()  { gate derive         cargo test -p misaka-palw-derive; }
run_cli()     { gate cli            cargo test -p misaka-cli; }
run_kaspad()  { gate kaspad         cargo test -p kaspad --lib; }
run_selftest(){ gate artifact-selftest python3 scripts/misaka-palw-artifact-conformance.py selftest; }
run_stranger(){ gate stranger          python3 scripts/misaka-palw-derive-stranger.py; }
run_third()   { gate third-party       python3 scripts/misaka-palw-artifact-thirdparty.py --require; }

ALL=(fmt clippy core base0 derive cli kaspad selftest stranger third)
if [ $# -eq 0 ]; then set -- "${ALL[@]}"; fi
for g in "$@"; do
  case "$g" in
    fmt|clippy|core|base0|derive|cli|kaspad|selftest|stranger|third) "run_$g" ;;
    *) echo "unknown gate: $g (one of: ${ALL[*]})" >&2; exit 2 ;;
  esac
done
echo; echo "all requested gates green: $*"
