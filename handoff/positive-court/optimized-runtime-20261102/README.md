# Bounded optimized runtime and producer membership — 2026-11-02

Implementation: `6cd0f4dbc`. The original trained short job still does **not**
complete Node checking. Both measured executions terminate with SIGXCPU under
the unchanged 240-second CPU limit. This is a local performance increment,
not an accepted native claim, full Node verdict or activation profile.
The [full seven-stage requirements](../COMPLETION-REQUIREMENTS.md) remain in force.

## Change and unchanged checking

The producer dependency walk now uses a randomized `HashSet` for membership.
It never iterates that set: the existing pending stack selects traversal,
and ordered source/parameter sets select canonical material. Visit charges
still occur before membership lookup; pending/unique frame limits, refusal
ordering and all cumulative caps remain. Node arithmetic, material
authentication, canonical closure, relation plan and final acceptance are
unchanged. No numerical result or saved verdict is cached.

The existing 1,024-byte/frame logical owner is retained. A compile-time check
requires private `Task` size at most 64 bytes. The repository pins Rust 1.93.0;
its [standard-library dependency manifest](https://raw.githubusercontent.com/rust-lang/rust/1.93.0/library/Cargo.toml)
selects hashbrown 0.16.1. The [pinned table implementation](https://raw.githubusercontent.com/rust-lang/hashbrown/v0.16.1/src/raw/mod.rs)
rounds capacity to power-of-two buckets, keeps spare capacity and temporarily
holds old/new tables on growth. For this insertion-only walk, our inference is
that eight transient buckets per admitted frame conservatively cover that
growth, including small tables. With keys at most 64 bytes and pending storage,
this stays within the retained frame envelope with margin. This is a review of
the pinned implementation, not a general `HashSet` allocation contract or a
physical RSS quota; a toolchain/layout change requires review.

Existing normal, false-Panel, false-Miner and dynamic/state fixtures now rebuild
each material at the same relation/frontier with a fresh membership seed.
They compare serialized producer fields/target byte for byte and every field
of the cumulative material-meter delta. Two simultaneous test exports have a
separate prepaid owner. The tuple is test-local, not a new peer/native codec.
This repeated material walk does not repeat the numerical Panel execution;
the actual trained measurement still constructs each material once.

## Executed validation and build profile

[Final runtime](hashed-walk-runtime-first.json) passes **35 tests**: 11 existing
full-relation, four record-authentication and 20 wide Panel tests. Two opt-in
cases are ignored by this default run. [Selected strict Clippy](hashed-walk-clippy-first.json)
passes with `--no-deps -- -D warnings`; it checks the executor's selected three
test targets, not the entire workspace. [Formatting and whitespace checks](whitespace-check.json)
pass for the two changed Rust files, with child-module traversal disabled.
Unrelated child-module formatting and raw-log whitespace are preserved.

Before changing the walk, the same 35 tests also passed in
[optimized-runtime-r2.json](optimized-runtime-r2.json). The private Cargo command
uses opt2/debug0 for Core, TIR, executor and the test harness; all four retain
**debug assertions and overflow checks**. The compiler-emitted
[final profiles](hashed-walk-verified-profiles.json) verify this. The global
Cargo profile label still says `unoptimized`, because these are package
overrides. Consensus remains opt0; other dependency overrides remain as recorded.
No repository production profile is changed. Native archives are reused under
the existing link configuration; the forbidden-CXX wrapper remains active.
RUSTFLAGS and RUST_MIN_STACK are absent, Rayon is one thread, and commands are
offline/locked. Rust is 1.93.0 / aarch64-apple-darwin / LLVM 21.1.8.

## Actual same-job comparison

Both runs use original Pythia-70m revision
`a39f36b100fe8a5377810d56c3f4789b9c53ac42`, the same private wide-Q24 artifact,
input IDs, arithmetic, class, artifact, tokenizer and context. Its model SHA256
is `2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651`.
The model and registration fixture match before/after each run; the compiled
binary also matches after termination. See [input checks](input-sha-hashed-walk.json)
and [validation pins](validation-summary.json). SHA reads warm OS caches.

This remains the original **noncanonical 16-prompt/one-decode** job, with
registered max context 2,048. It is not canonical 255/2 or a full-context run.
Ordinary output is token 273 with the same committed output. Ordinary inference
performs zero witness runs; Panel captures once, retaining 14,157 original
leaves and constructing 213,298 required relations / 13,225,922 target lanes.
Raw capture and canonical claim tapes total **305,797,608 bytes**.

| Observed phase | opt2, ordered membership | opt2, hash membership |
| --- | ---: | ---: |
| Ordinary Miner inference | 0.0791 s | 0.0792 s |
| One-pass Panel capture | 0.5762 s | 0.5273 s |
| Complete signed Panel record construction | 3.6469 s | 3.7021 s |
| Material preparation at common numeric prefix 26,624 | 172.5018 s | 98.3315 s |
| Node submit at that same prefix | 56.8935 s | 56.7853 s |
| Last logged numeric prefix before CPU termination | 26,624 | 40,960 |

These are measured phases/prefixes, not full-job extrapolations or a universal
speedup. The [common-prefix comparison](common-prefix-comparison.json) verifies
identical actual provider usage and evaluator-counter peaks at every common
logged prefix. It does not assert a complete trained-material byte comparison;
the fixture repeat tests cover canonical byte equality separately.

At the later 40,960 prefix, material preparation is 150.4721 s and Node submit
87.1526 s. Provider usage is 1,002,687,169 bytes, 1,978,793 reads, 32,501 openings,
one attempt. Peak evaluator observations are 7,082 elements, 164,864 terms,
170,720 source reads and 808,025,472 workspace request bytes. These are within
the existing per-relation caps and actual logical owners.

[Baseline execution](pythia-optimized-first.json) and
[changed execution](pythia-hashed-walk-first.json) both exit **-24**, with no
outer-monitor stop and no libtest final summary. They produce no final Node
outcome, full record EOF/signature acceptance or native decision. Last logged
prefixes are lower bounds, not invented exact terminal relation counts.
[Model summary](model-run-summary.json) retains the measured phase observations.
Each process group is empty after termination; neither process was restarted
because of a lost tool response.

Bounds remain: 8-GiB shared logical live-request ledger, 1-GiB evaluation
workspace, 256-MiB authentication owner, 262,144 terms/reads, 16,384 elements,
4-GiB actual provider reads, 2^22 provider calls, 2^20 openings, two attempts,
320-MiB cumulative tapes, 240-s CPU and 360-s outer wall. The 4-GiB process-group
RSS stop and 512-MiB free-disk stop are sampled monitors, not kernel/atomic
admission quotas. Symbolic provider envelopes are not actual traffic or fees.
Producer and Node coexist in one process and retain producer owners; this
does not measure an isolated cold Node or qualify all physical memory costs.

## Failed build, disk admission and cache provenance

The [first optimized build](optimized-runtime-first.json) stopped at the
sampled disk floor (minimum 535,080,960 bytes) before any tests ran. It had
already emitted the successful Core/TIR/executor dependencies. Transparent
filesystem compression of those existing files preserved readable bytes,
SHA, size and mtime under the Cargo lock; normal Cargo reuse then completed r2.
No missing/foreign Rust library was restored into the cache.

[Initial retirement](cache-retirement.json) removes four own obsolete opt0 Core
intermediates, retaining the baseline executable. [Later retirement](obsolete-opt0-dependency-retirement.json)
removes 61 own old opt0 intermediates absent from the complete current Cargo
dependency set. These are retired generated caches, without preserved copies;
older receipts do not imply current availability. Sources, originals, logs,
measurement executables and native archives remain. Existing successful
libraries, including external dependencies in the actual Cargo manifest, were
also transparently compressed; the individual compression receipts record
unchanged SHA and mtime. Cache management is not correctness evidence.

Two disk admission refusals preceded the changed-model child. The first is
retained in [the prelaunch record](hashed-walk-prelaunch-refusal.json). The
second console observation was 623,108,096 free bytes against the same
859,445,736 required bytes; its console traceback was not saved as a separate
raw artifact. Neither created a model log/child or spent a model attempt. The
successful launch required the 512-MiB floor plus known aggregate tapes and
16-MiB control headroom. No CPU or model-work cap was enlarged.

Final operator-file versions and their SHA inventory are preserved under
`operator-scripts/`. Commands, source-before/after inventories, binary pins,
raw-log digests, exits and process-group checks are recorded independently.
The unused convenience compression script is retained as such, not claimed
as an executed success. No raw log or diagnostic record was normalized.

[Documentation validation](docs-final.json) passes the existing
`no-new-regressions` check with zero new issues and all 43 known missing
references retained. This is not a clean strict documentation audit; the
baseline was not changed.

## Remaining work

Complete checking of this unchanged short job, separate cold/warm role
CPU/RAM/IO/recovery costs, canonical/full-context costs, native
Assignment/Result/funding/P, every credit consumer, real node restart/reorg/
IBD/pruning and dependency handling, and explicitly approved finite activation
remain required. The native-assignment RFC/ADR drafts remain unadopted. Full
local replay would still not replace actual selected-validator certification
under its explicit honest-fraction assumption.
