# ADR-0055: BASE-0 runtime acceleration — backends below the semantic boundary

Status: **Accepted.** Consensus-inert: no class id, no catalog entry, no ruleset field and no
fingerprint moves under anything this ADR permits. A change that would move one is, by that fact,
outside this ADR's scope.

Date: 2026-08-27

Relates to: ADR-0040 Decision E (integer associativity — the property every decision here spends),
ADR-0049 (the canonical IR, which is where dispatch lives), ADR-0051/0053 (the float-runtime family
and its withdrawal — the road this ADR exists not to retake), ADR-0052 (the Qwen3.6 hybrid class,
the workload being accelerated), ADR-0038 Decision D (wall-clock never enters consensus).

---

## Context — the question, and the wrong answer already taken once

The engines are correct and slow. The reference-shaped Rust runs the real 33 GiB Qwen3.6-35B-A3B at
~9 s per canonical (8, 2) job on the reference M4 Pro; the A16 dense tier reached 30 tok/s only
after a first round of hand-written CPU kernels (13.5× over the scalar reference, measured on
Qwen2.5-1.5B). A public network feels this in four places: producer cadence, panel seats answering
duties for the model tiers, court replay cost, and the from-genesis IBD headroom multiple — every
one of them a wall-clock quantity that consensus deliberately does not price (ADR-0038 D), which is
exactly why making them faster is free.

An external survey (2026-08-27) recommends the obvious sources: llama.cpp's backends, FlashInfer's
CUDA kernels, MLC/TVM compilation, MoE expert-streaming runtimes. The naive reading — swap the
runtime — is a road this repository has already taken and withdrawn: Family M rode a pinned
llama.cpp + Metal build, and ADR-0053 removed it because a per-vendor float class verifies by
tolerant replay, and tolerant replay can never convict. Whatever is adopted from that survey must
not reintroduce the thing ADR-0053 buried.

What makes adoption possible at all is ADR-0040 Decision E. BASE-0's arithmetic is integer and
exact: `i32`/`i64` accumulation is associative and commutative, so the order a dot product is
reduced in — across SIMD widths, tile shapes, thread counts, GPUs, vendors — **cannot change the
result**, within the overflow bounds the catalog already makes premises. A float kernel library is
fast because it renegotiates arithmetic; an integer backend can be fast while performing,
bit-for-bit, the same function. `misaka-palw-base0/src/optimized.rs` already proves the property in
miniature (interleaved, blocked and reversed reductions, all bit-identical to the scalar fold), and
`kernels.rs` already ships fast grouped matmuls pinned by
`the_fast_kernel_is_bit_identical_to_the_reference`. This ADR is that precedent, generalized and
given rules.

## Decision 1 — the semantic boundary is the catalogued kernel

The unit of optimization is one `kernel_semantics_id`. A backend may implement a kernel any way it
likes — SIMD, GPU, fused, tiled, out of order — provided its output is **byte-identical to the
reference for every input in the kernel's declared domain**. Equality is exact; there is no
tolerance, no epsilon, no rank correlation, no "p95 within bounds". A backend that needs tolerance
is a different class wearing an optimization's name, and ADR-0053 already decided what happens to
those.

Everything above the boundary is untouched: the class id (the graph's), the artifact root, the
step-leaf trace, receipts, the court, the catalog. **A conforming backend is consensus-invisible.**
Which host ran which backend is not a chain fact, needs no registration, and appears in no object.

## Decision 2 — no kernel certificates

The survey proposes certifying optimized kernels on-chain and replaying the reference only in
disputes. Rejected. A certificate is a declaration, and this repository's recurring lesson is that
declared facts drift from derived ones. Here the derived fact is checkable locally (equality
against the reference) and the failure mode is already priced: a backend that ever diverges
produces a wrong committed row, which is a refutable step fault — the existing court convicts it
and the existing bond pays for it. The chain does not need to know backends exist, because a wrong
one is indistinguishable from a dishonest producer, and that machinery is built.

For the same defense-in-depth reason the split stays asymmetric: **producers and panel seats may
run any conforming backend; the court's adjudicator keeps the reference path** (and `ref2`, the
author-independent second implementation, keeps existing beside it). The party whose output becomes
a verdict is the one party that never takes the fast path.

## Decision 3 — the gate is differential, per backend, and it must fire

A backend merges only with its equality gate wired into the default-member test set:

* the full KAT corpus (17,881 vectors) run through the backend and compared byte-for-byte;
* the structural sweeps `optimized.rs` pioneered — interleaved, blocked, reversed — at the
  backend's real tile shapes;
* adversarial vectors at the edges the catalog names: saturation boundaries, SRDHM rounding
  midpoints (the vendored gemmlowp is the spec), zero-point extremes, maximum-K reductions,
  ragged final tiles;
* one end-to-end job per registered class, backend vs reference, compared at every committed row.

A gate that exists but is not in the default members is the gate this repository has been burned by
four times; wiring it is part of the backend, not a follow-up.

## Decision 4 — the order of work

1. **CPU SIMD** for the one hot loop every class shares: `matmul-quant/i8xi8-i32-exact` in its
   grouped and wide forms (Qwen3.6's eight-expert concatenation is a single `[4096 × 2048]` GEMM by
   construction — the IR already shaped the workload for exactly this). x86 AVX2 / AVX-512-VNNI and
   ARM NEON `sdot`/`i8mm`, plus the fused requantize (legal under Decision 6). This is where the
   A16 tier found 13.5×, applied to the tier that matters most.
2. **Apple Metal, integer compute.** Not simdgroup float matrices — `i8` products accumulated in
   `i32`, where GPU-parallel reduction is exact for the same reason CPU-parallel reduction is. The
   target host is the M-class producer already running the 33 GiB artifact; residency stays mmap.
3. **CUDA `dp4a`/IMMA.** Int8 tensor-core paths accumulate in `i32` exactly. FlashInfer is a
   *scheduling* reference here — paged KV layouts, MoE grouping, graph capture — never a kernel
   source: its kernels are float-centric, and anything transcribed must pass Decision 3 whole.
4. **Residency** (the FreeToken-adjacent idea, kept): expert page cache and NVMe streaming for the
   33 GiB artifact on small-RAM hosts. Pure memory locality; touches nothing committed; interacts
   only with the existing host lease.
5. **Research, explicitly not decided:** batched verification of decode steps (admissible only if
   every committed step leaf still materializes bit-identically — the draft-model half of
   speculative decoding is a foreign forward pass and stays out), and codegen from the canonical IR
   (the MLC/TVM direction; ADR-0049 built the IR it would consume, but a compiler toolchain inside
   consensus-adjacent code is a dependency decision this ADR does not make).

## Decision 5 — what the survey suggested that is refused by name

* **Runtime substitution** (llama.cpp, ik_llama.cpp, FreeToken as the engine): re-runs the
  ADR-0051 → ADR-0053 arc. llama.cpp remains a design reference for backend structure; its
  arithmetic does not enter.
* **New quantizations** (IQK, Trellis, TurboQuant…): a different weight encoding is a different
  artifact root and usually a different graph — a **class registration**, with ADR-0052's whole
  burden (catalog coverage, adjudicators, court cost), never an optimization. The Qwen2.5 PTQ
  degeneration (`[11,11,11,11]`) is the standing reminder of what "just requantize it" costs.
* **Tolerant equivalence** in any form. The Ambient PoL survey (ADR-0026) already concluded
  tolerance belongs to systems that cannot convict; this one can.

## Decision 6 — fusion stops at the committed row

The observable set is what the trace commits: every step-leaf row, checkpoint row and logits
opening the class's profile declares. Below that, backends may fuse freely — matmul + requantize in
one pass, norm + projection, whole-layer megakernels — because an intermediate nobody committed is
nobody's business. At the boundary, the committed row must materialize byte-identically. A fusion
that changes which rows *exist* is a profile change, i.e. a new class.

## Consequences

* No re-mint, ever, from anything conforming: backends land per-host, per-release, incrementally,
  and hosts on different backends interoperate silently.
* Producer cadence, seat latency, court replay and IBD headroom all improve without a consensus
  variable moving; `pwu` stays the counted step-leaf cost (ADR-0038 D), so acceleration changes
  who can afford to participate, not what work is worth.
* CI grows a per-backend differential matrix; that cost is the price of Decision 2's "no
  certificates" and is paid in tests rather than in trust.
* A divergent backend in the wild is a slashed producer, not a chain event. The safety argument is
  unchanged because the court never learned backends exist.

## What this ADR does not decide

Per-class hardware requirements, a serving/scheduling stack above the engine, remote attestation of
backends (deliberately nothing — Decision 2), and the compiler question (Stage 5 research).
