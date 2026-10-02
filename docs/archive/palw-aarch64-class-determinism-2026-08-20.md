# PALW aarch64 CPU class — determinism measurement, 2026-08-20

> **This describes the WITHDRAWN float lane, and no longer applies to any network.** The CPU
> determinism class exists because llama.cpp ships hand-written per-ISA kernels whose reductions sum
> in different orders — a real property of that runtime. The execution family replaced it
> (ADR-0053): pinned integer arithmetic in this tree's own Rust, with **no `target_arch` branch on
> the execution path** and `runtime_class_id` left at zero, because the integer family's identity is
> its graph and not its host. **There is no CPU class today, and arm and x86 hosts are not
> separated** — for verifiers or producers. Kept as the record of what the float lane cost and of
> why the network left it. See `testnet11-node-operator.md` §2.


**What this is:** the gate-ledger §12 items 2 and 4 measurements, run on the **aarch64 CPU class**
(`misaka-palw-lite-cpu/aarch64-dotprod/v1`). Real inference against the pinned
Qwen3.5-2B-Q4_K_M, not a fixture.

**What this is NOT:** a contribution to gate item 1. That item asks for ≥ 3 CPU
microarchitectures **within one determinism class**, and aarch64 is a different class from x86_64
by construction — `vlt.rs`' `CPU_RUNTIME_CLASS` is scoped to the ISA, and its own comment says why
(`ggml/src/ggml-cpu/arch/` ships separate hand-written kernels; a NEON reduction and an AVX2
reduction sum a vector in different orders). Item 1 still needs a third **x86** μarch beside the
measured Broadwell and EPYC. What these numbers do is establish that the aarch64 class is
self-consistent, which is the property a committee **within** that class needs before it can form.

## Host

| | |
| --- | --- |
| CPU | Apple M4 Pro (`neon=1,dotprod=1`) |
| OS | macOS 26.4.1, 24 GB |
| Build | `MISAKA_PALW_CPU=1`, llama.cpp CPU-only tree (`GGML_METAL=OFF`, `GGML_BLAS=OFF`, `GGML_NATIVE=OFF`, `GGML_OPENMP=OFF`) |
| `runtime_class_id` | `27e4fb5a1c2da5dd…` |
| `runtime_manifest_hash_v2` | `2b0fb941ea6f5ec6…` (self-test run) |
| FP environment | `rounding=rne,ftz=0,daz=0` — canonical |

## Item 2 — reruns per machine

`--mode v2-replay-bench --runs 5`, every registered golden job. The corpus is the **registered
golden set (4 jobs)**, not 1,000 canonical prompts: the 1,000-prompt corpus does not exist on this
tree, and running 5 reruns of 4 jobs is not the item — it is the *rerun* half of it, measured, so
the remaining work is the corpus rather than the method.

| job | prefill | runs | `roots_identical_across_runs` |
| --- | --- | --- | --- |
| `golden-min-1tok-d1` | 1 | 5 | **true** |
| `golden-probe-12tok-d16` | 12 | 5 | **true** |
| `golden-prefill96-d16` | 96 | 5 | **true** |
| `golden-repeat8-d2` | 8 | 5 | **true** |

And the roots are the **registered** ones: `--mode v2-selftest` returns `status: pass` against the
`qwen35-2b-v2.cpu-aarch64.golden` set, so this build reproduces values recorded before it, not
merely values it agrees with itself about.

Timing, `golden-prefill96-d16`, 5 cold runs: p50 1,504 ms · p95 1,507 ms · p99 1,507 ms
(each run includes a ~900 ms model load).

## Item 4 — the condition matrix

Baseline root for `golden-prefill96-d16`: `10683fa2472ce6ae554e9d1dc9cf8fc4…`

| condition | result |
| --- | --- |
| cold (process per run) | **MATCH** (the bench above, ×5 per job) |
| restart ×2 | **MATCH** |
| concurrent ×4 (four cold workers at once) | **MATCH** ×4 |
| memory pressure (2 GB churned alongside) | **MATCH** |
| affinity | **not run** — see below |

**Affinity is not measured, and saying "match" for it would be a lie about the method.** macOS
has no `sched_setaffinity`; pinning threads to cores needs `thread_policy_set` with affinity
tags, which the OS treats as a *hint* and ignores on Apple Silicon. The condition is meaningful
on the Linux fleet and must be measured there.

## What this does and does not license

* **Does:** the aarch64 CPU class reproduces its registered roots across cold starts, restarts,
  four-way concurrency and memory pressure. A committee inside this class can compare roots.
* **Does not:** say anything about x86 hosts. Cross-class roots differ by design — the earlier
  measurement of 0/61 matching `gemm_trace_root` between x86 and Metal is the same phenomenon —
  so an aarch64 node and an x86 node must never be in one committee, which is exactly what the
  ISA-scoped `CPU_RUNTIME_CLASS` enforces.

## A defect found while measuring

`v2-manifest` reports `ggml_flags: {avx: true, avx2: true, fma: true, f16c: true, sse42: true}`
**on an arm64 host**. The values are real — the tree's `CMakeCache.txt` really contains
`GGML_AVX2:BOOL=ON` — but they are llama.cpp's *defaults* for options its arm64 kernel selection
never consults. The build script's own comment says these flags are "measured from the real build,
never declared"; on this arch they are declared by CMake and describe nothing.

**Determinism is not affected** (two hosts of one arch read the same defaults, and
`runtime_class_id` is ISA-scoped, so the classes still separate). What is affected is what the
manifest *claims to know*: an aarch64 manifest and an x86_64 manifest can carry an identical flag
set while running entirely different kernels. The honest evidence on this arch is
`host_cpu_features: "neon=1,dotprod=1"`, which is measured from the host.

**Fixed the same day.** The display document now emits the x86 word only on x86 and, on other
architectures, says `"not applicable on this architecture; see host_cpu_features"` — ABSENT rather
than `false`, because reporting `avx2: false` on arm64 would be a second untrue claim about a flag
nothing read. The arch-independent flags (native, openmp, blas, accelerate, cpu_all_variants)
print everywhere because they mean the same thing everywhere.

`runtime_manifest_hash_v2` still covers the full cache-derived set, so **no consensus identity
moved because of this change**. Verified by measurement: after the rebuild the manifest hash did
move, and the only input that moved with it was `worker_binary_sha256` (`807e625f…` → `ca3b2946…`)
— the cache hash, the linked-library hash and the `runtime_class_id` are byte-identical. A worker
code change moving the identity is the design; the flags did not move it. And `v2-selftest`
returns `status: pass` with all four golden roots unchanged, so the display fix does not touch
execution.

## Reproducing

```
MISAKA_PALW_CPU=1 MISAKA_LLAMA_SRC=<cpu-only llama.cpp tree> cargo build --release -p misaka-palw-worker
export MISAKA_PALW_GOLDEN=misaka-palw-worker/golden/qwen35-2b-v2.cpu-aarch64.golden
export MISAKA_PALW_GGUF=<pinned Qwen3.5-2B-Q4_K_M.gguf>
./target/release/palw-worker --mode v2-selftest
./target/release/palw-worker --mode v2-replay-bench --name golden-prefill96-d16 --runs 5
```
