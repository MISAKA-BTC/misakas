#!/usr/bin/env bash
# PALW v2 determinism-class probe — ONE fleet host's class line.
#
# Run this on every host that must share a determinism class, collect the emitted
# JSON lines, then feed them all to `misaka-palw-v2-class-compare.py`.
#
# What this measures, and why it is not the existing golden self-test: the boot
# self-test proves a host reproduces ITS OWN goldens. That is a self-consistency
# check and it passes on a host that agrees with nobody. The class claim — "these
# hosts compute the same trace" — is a statement ABOUT PAIRS OF HOSTS, and nothing
# in the tree measured it. The PALW-Ollama flavor shipped on an 8-seed campaign that
# looked like agreement because the quantity it compared was a constant; this probe
# exists so the replacement does not repeat that.
#
# Usage:
#   MISAKA_PALW_GGUF=/path/to/qwen35-2b.gguf \
#     scripts/misaka-palw-v2-class-probe.sh ./target/release/palw-worker [label]
#
# Emits one JSON object on stdout (the "class line"). Diagnostics go to stderr.
set -euo pipefail

WORKER="${1:?usage: misaka-palw-v2-class-probe.sh <palw-worker> [label]}"
LABEL="${2:-$(hostname -s 2>/dev/null || echo unknown)}"
: "${MISAKA_PALW_GGUF:?MISAKA_PALW_GGUF must point at the pinned GGUF}"

[ -x "$WORKER" ] || { echo "not executable: $WORKER" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ── 1. Runtime identity, with NO golden set registered ────────────────────────────
# Registering goldens deliberately CHANGES runtime_manifest_hash_v2 (the manifest
# carries golden_vector_root). So identity must be read unregistered, or every host
# would differ trivially on its own generated set and tell us nothing.
echo "[class-probe] $LABEL: reading runtime identity (no golden registered)" >&2
env -u MISAKA_PALW_GOLDEN "$WORKER" --mode v2-manifest > "$WORK/manifest.json"

# ── 2. Generate this host's goldens over the fixed probe corpus ───────────────────
# The corpus is inputs-only and compiled into the worker (golden_probe_inputs);
# expectations are MEASURED here. Two hosts in one class must measure the same ones.
echo "[class-probe] $LABEL: generating goldens over the fixed probe corpus" >&2
env -u MISAKA_PALW_GOLDEN "$WORKER" --mode v2-golden-gen --out "$WORK/host.golden" >&2

# ── 3. Read back the per-job trace roots ─────────────────────────────────────────
MISAKA_PALW_GOLDEN="$WORK/host.golden" "$WORKER" --mode v2-golden-show > "$WORK/golden.json"

# ── 4. Confirm the host passes its own goldens (self-consistency floor) ──────────
# A failure here means the runtime is not even deterministic against itself, which
# must be fixed before any cross-host question is meaningful.
SELFTEST=pass
if ! MISAKA_PALW_GOLDEN="$WORK/host.golden" "$WORKER" --mode v2-selftest >&2 2>"$WORK/selftest.err"; then
  SELFTEST=FAIL
  echo "[class-probe] $LABEL: SELF-TEST FAILED — see below" >&2
  tail -20 "$WORK/selftest.err" >&2 || true
fi

# Keep the generated set: the comparator's decisive step runs each host's set on
# every OTHER host, and that needs the actual file.
OUT_GOLDEN="palw-v2-golden.${LABEL}.bin"
cp "$WORK/host.golden" "./$OUT_GOLDEN"
echo "[class-probe] $LABEL: golden set written to ./$OUT_GOLDEN" >&2

python3 - "$LABEL" "$SELFTEST" "$OUT_GOLDEN" "$WORK/manifest.json" "$WORK/golden.json" <<'PY'
import json, sys, platform
label, selftest, golden_path, man_path, gold_path = sys.argv[1:6]
man = json.load(open(man_path))
gold = json.load(open(gold_path))
line = {
    "schema": "misaka.palw.v2-class-line.v1",
    "label": label,
    "selftest": selftest,
    "golden_file": golden_path,
    "host": {
        "machine": platform.machine(),
        "system": platform.system(),
        "release": platform.release(),
    },
    # Identity — hosts must match on ALL of these to be in one class by construction.
    "runtime_class_id": man["runtime_class_id"],
    "runtime_manifest_hash_v2": man["runtime_manifest_hash_v2"],
    "model_profile_id": man["model_profile_id"],
    "shape_profile_id_v2": man["shape_profile_id_v2"],
    "tokenizer_id_v2": man["tokenizer_id_v2"],
    "trace_scheme_id_v2": man["trace_scheme_id_v2"],
    "worker_binary_sha256": man["worker_binary_sha256"],
    "cmake_cache_sha256": man["cmake_cache_sha256"],
    "llama_static_library_sha256": man["llama_static_library_sha256"],
    "ggml_flags": man["ggml_flags"],
    "fp_environment_probe": man.get("fp_environment_probe"),
    "fp_environment_canonical": man.get("fp_environment_canonical"),
    # The measured arithmetic — this is what the class claim is actually about.
    "golden_root": gold["golden_root"],
    "jobs": {j["name"]: {"expected_root": j["expected_root"], "expected_cu": j["expected_cu"]} for j in gold["jobs"]},
}
print(json.dumps(line, sort_keys=True))
PY
