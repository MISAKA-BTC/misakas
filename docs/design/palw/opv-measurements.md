# OPV fresh-verifier measurements, and the window and collateral formulas they feed (MEAS)

Agent MEAS, branch `meas/opv-timings` (base `b676927de`; the integration head `b8ae9412b` merged on 2026-10-10), first written 2026-10-09,
revised 2026-10-10 for the user's design changes and for G14C's GAP-07. Harness: `tools/opv-meas` (a test-only binary; no wire id, fence,
parameter or fingerprint of the protocol changes). Amounts are BILI (ADR-0174). Machine-readable table: `opv-measurements.json` (next to
this file; raw results in `opv-measurements-data/`, derivations in `opv-measurements-data/derive/`).

**Status.** A measurement note and a plan. It chooses no parameter, arms no fence and approves nothing: every number below is an *input*
to the user's decision on the OPV terms (`activation-readiness-matrix.md` §3a, order step 3). Nothing here is armable. Each number carries one
label, here and in the JSON:

* **M** — measured: read off a harness run on this Mac, on a real artifact (the run count and spread are given);
* **D** — derived from M by a stated formula (the formulas are executable: `opv-meas derive`, 10 unit tests, all passing, one over the committed inputs; the same arithmetic is mirrored in
  Python, which reproduces the soundness dossier's row K at its own 500 permille);
* **A** — assumed: no run established it. An A is never used as a result; it is named so that a measurement can replace it.

The user's 2026-10-09 ruling applies: the probability that a fraud is actually checked (`q`, `P_run`) and the verifier's cost recovery are
priced from measurements, never from an assumed independence or an assumed rate. This note therefore measures what ONE verifier costs and
covers, states the formulas, and leaves `q`, the number of verifiers, their independence and — since ADR-0177 — **whether they hold the model at all**
as **unmeasured** (§9).

Premise conditions addressed (`docs/PRINCIPLES.md` §6): **5** (collateral against the maximum gain and a *real* detection probability),
**6** (an honest verifier can take part in time, with the resources it has), **7** (the dispute / Final / reorg clock). For G14C's matrix
(`g14-completion-matrix.md`, `g14/completion` @ `82a176a77`): **GAP-07**, the worst-case deadlines of condition C8, are §4.

## 0. What the 2026-10-10 changes did to this note

| Input | Applied here | Where |
|---|---|---|
| G14C GAP-07 (top priority): the measured worst-case deadline of C8 per class and profile, for a fresh non-seat verifier to fetch the claim material, check, localize and file | one table per family and class: what ran (M), what is derived (D), what is assumed (A), what cannot run on this host or has no real artifact. New samples: **P = 32 measured** (12 DAA); the gate's **P = 64 attempted and stopped by RAM** | §4 |
| ADR-0177 D7: the chain does not make a model obtainable | `T_fetch` splits: the claim's material stays a protocol term, the **registered model is an off-chain acquisition assumption** (`T_acquire`, outside the window); every detection probability is **conditional on holding the model**; a closed-model row where it is 0 and no finite collateral exists | §3.1, §3.2, §5 |
| ADR-0032 49 % / ADR-0176 D6 | every collateral and deterrence formula uses the loss **net** of the reporter's 49 % self-return (51 % of the collected slash), never the gross slash; the interim ledger's 500 permille is shown only as a comparator | §3.3, §5 |
| ADR-0176 §4 | the measurement **plan** (it needs lane BUDGET's engine; none of it ran) | §7 |
| RFC-0004 Part II | per-kind inspection cost: **no real Memory, Retrieval or Composite artifact exists**, so it is listed unmeasured, with the structural bounds and unit costs a measurement would use | §8 |
| PESG §4 D (the user's Probabilistic–Economic Security Gate) | the five factors of the real conviction probability are scored per class and profile in `pesg-d-conviction-probability.md`; this note's GAP-07 (§4), ADR-0176 plan (§7) and RFC-0004 list (§8) are folded into it | pesg-d |
| ADR-0175 | a registered model never changes, so a verifier that authenticates its copy once, when it acquires it, stays sound: the `model_auth_once` sensitivity (A) is legitimate | §3.1 |
| a fifth real class | GLM-Edge-1.5B (H1's artifact, byte-identical copy): static bounds, the streaming weight pass and the producer's world ran on 2026-10-10 | §2.2 |

## 1. What was measured

**A fresh verifier**, one new process per sample (`opv-meas verify`): it holds nothing of the producer's. It (1) reads and strictly decodes
the chain's blocks and replays them into a kernel ledger, (2) fetches the claim's public material and the class artifact from a provider
with every chunk hash-checked against a manifest bound to the claim, (3) runs the kernel's own fresh-verifier path (`OutsiderV1::check`,
the one the real-node E2E uses; its private-salt coins — the only soundness-bearing path today, SOUND SG-05), (4) on a mismatch, localizes
it, assembles the filing and encodes it as a `FileProof` object, (5) strict-decodes it and has a node replica's court judge it. Each step
records wall time, CPU time (user+system), bytes moved, peak RSS and the host's 1-minute load.

**Claims.** The producer side (`opv-meas build-world`) registers the real artifact on a kernel ledger, evaluates the class on 3 tokens
with the reference evaluator, commits, and publishes the claim's public material and the artifact as a content-addressed provider tree.
Six claims per world: `honest`; `late` (a matmul in the last position's last occurrence), `early` (a matmul in position 0, occurrence 1),
`gather` (the embedding lookup), `elem` (an elementwise clamp in the last position), `decode` (a wrong delivered token). A lie edits ONE
committed value; everything else is honest.

**Artifacts** (`wh-h1-run/cache/*/class.palwtir.testnet-12.palwtir`, H1's, read-only; byte-identical copies were measured and the larger ones deleted afterwards): Qwen2.5-0.5B, Qwen3.5-0.8B, Llama-3.2-1B, SmolLM2-1.7B and (since 2026-10-10) GLM-Edge-1.5B.

**Host.** Apple M1 Max, 10 cores, 32 GiB, macOS 26.7 — **shared**: nine devnet `kaspad`, other lanes' builds and tests ran throughout.
During the verify samples of 2026-10-09 the 1-minute load was 37–220 (median 57) and swap 15–21 GB; the samples of 2026-10-10 (GLM-Edge-1.5B, the P = 32 claim, the P = 64 attempt) ran at load 13–26 and swap 4–7 GB.
Consequences, stated once:

* CPU time is the comparable figure; wall time under that load is 1.1–2.0× CPU (median 1.35×; M);
* CPU time is not load-immune either (efficiency cores, memory compression): repeated 140 s checks spread 138–154 s, and the linear fit of
  §2.4 leaves a 16 s residual. Treat every check time as ±16 s;
* `peak RSS` is macOS `ru_maxrss`, which does not count compressed or swapped pages: the *maximum* over samples is the honest figure.

**Limits of what was run (not hidden).**

1. **One class end to end.** The reference verifier caches every parameter instance as `i128` for the scope
   (`Ctx::param_cache`, `misaka-palw-kernel/src/verify.rs`): ≥ 16 bytes per parameter. Qwen2.5-0.5B needs ≈ 8 GB and ran (peak RSS up to
   10.4 GB at P ≤ 16, 20.8 B/param, M; 13.2 GB at P = 32, and more than 15.1 GB at P = 64, F-MEAS-10). Qwen3.5-0.8B needs ≥ 12.3 GB, Llama-3.2-1B ≥ 19.9 GB, GLM-Edge-1.5B ≥ 23.8 GB, SmolLM2-1.7B ≥ 27.6 GB: not runnable on this shared
   32 GiB host. For those, only the parts that need no whole-artifact cache were measured: the streaming per-parameter pass (`opv-meas
   weights`), the producer side and the claim's public-material size.
2. **No network.** Every fetch is a local file or the reference HTTP provider on `127.0.0.1`. Link bandwidth, latency, loss and provider
   egress are unmeasured (§9, F3a–F4).
3. **No chain.** Carrier inclusion and reorg depth are the policy's `carrier_daa` / `reorg_slack_daa` (2 / 2 interim), not measured (§9, F6).
4. **The reference implementation.** Single thread, `i128` tensors, no streaming. It is the code the kernel ships, not a ceiling on what an
   optimized verifier could do (see §2.5).
5. **Claims are small** (3, 8, 16 and 32 positions of one class; 64 was attempted and stopped by RAM). Anything larger is an extrapolation and is labelled.

## 2. Results

### 2.1 Unit costs on this host (M, `opv-meas micro`, medians of 5)

| Operation | CPU |
|---|---|
| `GF(2^127 − 1)` multiply-accumulate (Freivalds inner loop) | 8.9 ns |
| exact checked `i128` multiply-accumulate (a localized row's recompute) | 11.5–17.2 ns (three runs, host state differed) |
| `tensor_commitment` (dual-root Merkle) of an `i8` tensor | 24.2 ns per element |
| container tensor → `i128` | 3.8 ns per element |
| fetch + hash-check of public material / artifact from a local file | ≈ 690 MB per CPU-second (0.26 s per 179 MB; 1.00 s per 661 MB) |

### 2.2 The five real classes (M unless marked)

| | Qwen2.5-0.5B | Qwen3.5-0.8B | Llama-3.2-1B | SmolLM2-1.7B | GLM-Edge-1.5B (2026-10-10) |
|---|---|---|---|---|---|
| params | 502.4 M | 765.9 M | 1,245.0 M | 1,725.9 M | 1,485.2 M |
| artifact (container) bytes | 660.7 MB | 1,052.5 MB | 1,533.1 MB | 1,866.7 MB | 1,642.1 MB |
| weight pass, streaming: read / commit×1 / projection ×2 reps, CPU s | 1.5 / 13.1 / 13.5 | 2.4 / 23.3 / 21.5 | 3.8 / 42.2 / 36.9 | 4.6 / 49.1 / 49.6 | 3.5 / 43.5 / 43.3 |
| weight pass model, commit ×2 as the verifier does (D) | 41.2 s (82 ns/param) | 70.5 s (92) | 125.1 s (101) | 152.2 s (88) | 133.8 s (90) |
| public material per position, P=3 | 59.6 MB | **522.1 MB** | 70.5 MB | 106.4 MB | 96.8 MB |
| producer: reference evaluation of 3 tokens, CPU s | 24.9 | 57.1 | 71.4 | 78.8 | 67.4 |
| plan bound `max_verifier_ram` (gate) | 9.51 GB | 2.39 GB | 14.62 GB | 10.85 GB | 12.60 GB |
| lower bound on the reference verifier's RAM, 16 B × params (D) | 8.0 GB | 12.3 GB | 19.9 GB | 27.6 GB | 23.8 GB |
| gate `max_filing_bytes` (D, plan) | 0.54 GB | 1.07 GB | 1.07 GB | 2.15 GB | 1.07 GB |
| `carrier_fit_v1`'s worst filing/response (D, plan) | 9.22 GB | 1.83 GB | 14.03 GB | 10.58 GB | 12.31 GB |
| OPV registration at the interim carriers (1,583,616 B) | **refused** | **refused** | **refused** | **refused** | **refused** |
| verifier verified end to end here | yes | no (RAM) | no (RAM) | no (RAM) | no (RAM) |

The plan-bound rows are the K2-TIR-v1/v2 gate at 4 positions; "refused" is `carrier_fit_v1` ("a worst-case filing of N bytes does not fit
a FileProof"), so the measurements below ran on the same class under the Panel-licensed registration — the check and court path is
identical; only the registration mode differs.

### 2.3 Qwen2.5-0.5B, 3 positions, end to end (M; n = samples)

| Step | CPU s | wall s (load 37–220) | bytes |
|---|---|---|---|
| read, strict-decode and replay the chain (7.7 MB: registration + 6 claims) | 0.03 | 0.04 | 7,707,966 |
| fetch the claim's public material — files (n=16) | 0.26 | 0.35 | 178,650,993 |
| fetch the artifact — files (n=16). **Under ADR-0177 D7 this is `T_acquire`: off chain, an assumption, never a window term; a local file, no link** | 1.00 | 1.50 | 660,666,048 |
| fetch both — reference HTTP provider on localhost (n=2); provider process CPU 0.7 s each (the artifact part is `T_acquire`, the material part `T_fetch`) | 0.40 + 1.57 | 1.45 + 6.08 | same |
| **check, honest claim (`OutsiderV1::check`), n=5** | **140.4** (138.4–154.1) | **189.6** (177.9–291.6) | 839,317,041 opened |
| peak RSS of that process | | | **10.44 GB max** (4.6–10.4; = 20.8 B/param) |
| check through `FreshVerifierV1::check_salted` only (no value authentication, no decode relation), P=3 | 131.8 | 141.8 | |

Counters of the 131.8 s (the kernel's `CheckCostV1`): 1.004·10⁹ field multiplications (8.9 s), 465·10⁶ exact elements rebuilt for derived
values, 1.02 GB opened. Of the 131.8 s, the weight pass is ≈ 41 s (31 %, §2.2 model) and the field multiplications ≈ 9 s (7 %); the other
≈ 82 s (62 %) is derived-value rebuild, per-use clones of the cached `i128` tensors and wire decoding, **not separately timed** (the
release binary is stripped; a symbolised profile build is a fleet-plan item). The Freivalds algebra is a small part of the cost.

**Lying claims** (n as shown; check = `OutsiderV1::check` until the verdict, CPU s; filing = the `FileProof` object):

| Lie | n | check CPU | finding / fault | filing bytes | strict wire decode | court (CPU) |
|---|---|---|---|---|---|---|
| `early` (position 0, occurrence 1, matmul) | 2 | 12.4 | `Prosecute(Kernel)`, row scalar | 30,468 | accepted | 2.5 ms, convicted |
| `late` (last position, lm_head matmul) | 2 | 163.9 | `Prosecute(Kernel)`, row scalar | 31,492 | accepted | 6.9 ms, convicted |
| `elem` (last position, clamp) | 1 | 124.1 | `Prosecute(Kernel)`, element recompute | 227 | accepted | 3.4 ms, convicted |
| `decode` (wrong delivered token) | 2 | 3.0 | `Prosecute(Decode)` (the logits row) | 607,896 | accepted | 4.3 ms, convicted |
| **`gather` (embedding lookup)** | 2 | 13.0 | `Prosecute(Kernel)`, element recompute | **272,271,336** | **refused: past the 67,108,864-byte ceiling** | not reachable |

* **T_localize.** The check stops at the first fault, so a lie in the last instance costs a full pass and the excess over an honest pass
  bounds localization from above: `late` +23.5 s, `elem` −16 s — inside the ±16 s noise, i.e. not distinguishable from zero. The dominant
  term of a matmul-row localization is the exact recompute of the failing row: 896 × 151,936 MACs × 11.5–17.2 ns = **1.6–2.3 s** (D) for
  the lm_head row. For a class this size localization is seconds, not a budget line.
* **T_file.** Assembly 0.00 s CPU for filings up to 31 KB (0.70 s for the 272 MB one); encode, strict decode and the court are
  milliseconds. The filing's cost is its **size**, not its time (§2.6).
* **Withheld position** (`--withhold-pos 0 --serve`, n=1): the check returns `Demand` for the one position; its disclosure is a
  `Respond` object of **59,445,032 bytes**, against the 1,583,616-byte interim object cap — it cannot be carried on a chain with that cap.
  (The harness's replica ledger applied it and reported `Served`; it does not enforce the carrier at apply time, so that is not evidence a
  chain would.) After it, the re-check took 139.8 s CPU.

### 2.4 Position sweep, Qwen2.5-0.5B, `check_salted` (M; fresh path)

| P | public material | CPU s (samples) | wall s | exact elements | opened bytes | field mults |
|---|---|---|---|---|---|---|
| 3 | 178.7 MB | 131.8 | 141.8 | 465 M | 1.02 G | 1.004 G |
| 8 | 477.1 MB | 250.7, 247.1 | 498.8, 469.3 | 1,240 M | 1.62 G | 1.015 G |
| 16 | 956.4 MB | 373.7, 368.0 | 663.7, 752.3 | 2,482 M | 2.58 G | 1.033 G |

Least squares (D): **CPU ≈ 95.4 s + 17.5 s × P** (max residual 16.3 s); per byte of public material **0.29 µs** (3.4 MB/s) over a fixed
95.9 s. The per-position cost is not constant in the data: 23.4 s/position from P=3→8 and 15.2 s from P=8→16 (the P=16 samples ran at load
150–163, the P=3 sample at 65); the exact-element counter, by contrast, is linear (155 M per position). Public material per position is
59.6 / 59.6 / 59.8 MB at P = 3 / 8 / 16: at these tiny histories it does not yet grow with the position index. **Nothing here measures
P in the thousands.**

### 2.5 What the check time is made of, and what it is not

The reference verifier's cost is dominated by its implementation and its commitment scheme — authenticating every value (24 ns/element),
rebuilding derived values (155 M exact elements per position at 0.5B), cloning cached tensors — not by the Freivalds field arithmetic
(7 %). That has one consequence for every window number in §4 and §5: **they price this implementation**. A verifier that streams weights, hashes
once and runs on all cores would change T_check by an order of magnitude or more, and the soundness-relevant question — which verifier
implementation the honest-verifier assumption (RFC-0015 §2) refers to — has no answer in the repository. The policy cannot be derived
before that implementation is named and measured; §4 and §5 therefore show the reference verifier and one sensitivity row, and nothing else.

### 2.6 Findings

| Id | Finding | Premise condition |
|---|---|---|
| **F-MEAS-01** | **No real class is OPV-registrable at the interim carriers under K2-TIR-v1/v2** (all five: `carrier_fit_v1`'s worst-case filing/response 1.8–14.0 GB against 1,583,616 B). OPV timings for a real class presuppose the K2-TIR-v4 route (filing 170 KB at 9B-8k per K2S), which this base does not have. | 4, 6 |
| **F-MEAS-02** | **A lie in the embedding `Gather` is found in 13 s and cannot be filed**: its filing is 272 MB and the strict wire decode refuses it (ceiling 67 MB; interim 1.58 MB). Likewise a demanded position's disclosure is 59 MB. In K2-TIR-v1/v2 the objective court is complete only for faults whose filing fits (this is the K2S v4 element-court motivation, now measured on a real artifact). | 4 (§3 "bounded localization and adjudication") |
| **F-MEAS-03** | **The gate's `max_verifier_ram` is not a bound on the reference verifier's memory.** It is `artifact_bytes + evidence_bytes_per_position` of the plan; the verifier caches 16 B/param. Measured peak 10.44 GB (fresh path 10.47 GB) against a 9.51 GB bound at 0.5B; for 0.8B the bound is 2.39 GB against ≥ 12.3 GB, for GLM-Edge-1.5B 12.60 GB against ≥ 23.8 GB. At 9B the reference verifier needs ≥ 144 GB. | 6 |
| **F-MEAS-04** | Check cost is 62 % non-algebraic (§2.3) and **the per-position public material varies 8.8× across classes of similar size** (522 MB for Qwen3.5-0.8B against 60–106 MB for the others): T_check, DA and the fetch bill are properties of the program, not of its parameter count. | 6 |
| **F-MEAS-05** | A verifier authenticates each weight and each served value **twice** (`StageMaterial::param` / `OutsiderV1::missing`, then `Ctx::authentic`): ≈ 13 s (weights, 0.5B) + ≈ 9 s (values: the outsider-minus-fresh difference) of avoidable CPU in the reference path. Not a soundness issue. | 6 |
| F-MEAS-06 | Respond/disclosure and filing sizes (59 MB, 272 MB) mean the second branch of `OpvPolicyV1::validate` (`disclose_daa`, `localize + court + carrier + reorg ≤ proof_grace`) is not exercised by any real class today: there is no carriable disclosure to time. | 7 |
| **F-MEAS-07** | **The kernel's reservation relation counts the gross slash as the producer's loss.** `OpvPolicyV1::required_reservation` (`misaka-palw-kernel/src/opv.rs`) is `max(G + default_penalty, G / p)` with no reporter return, and `validate` enforces only that. ADR-0032 (49 %) and ADR-0176 D6 say a self-reporting producer recovers the reporter's share, so the loss is `(1 − r) S`: at `p = 8/8,192` the gross rule reserves 20,480 BILI where the net rule needs 40,157 (1.96×). The interim ledger's `accuser_reward_permille` is 500, not ADR-0032's 4,900 bps; which constant the K2/OPV route follows in the single release is open. Neither is changed here. | 5 |
| **F-MEAS-08** | `OpvBudgetsV1::cold_material_daa` is documented as "fetching the cold class / **artifact** / claim material". Under ADR-0177 D7 the registered model is an off-chain acquisition assumption, so the budget (and `verifier_start_cutoff`, which reads it) prices claim material only; the model's bytes leave it. A wording and sizing item for the OPV lanes; no code was changed here. | 6 |
| **F-MEAS-09** | A verifier's detection probability is conditional on its holding the model (ADR-0177 D7), and **nothing measures how many do**. The timings of this note price a verifier who exists and holds the model; for a closed model `P_det = 0`, no finite collateral exists, and the model's economic safety is unestablished (§3.2). | 5, 6 |
| **F-MEAS-10** | **The reference verifier's RAM grows with the claim, and it binds before the interim window does.** Peak RSS of the 0.5B class: 10.4 GB at P ≤ 16, 13.2 GB at P = 32, more than 15.1 GB at P = 64 (a 14-GiB watchdog stopped that run); the plan's `max_verifier_ram` is 9.51 GB for 4 positions. At 0.1–0.17 GB per position a 226-position claim needs ≈ 30–40 GB (D, two points). The time window would admit that claim (≈ 226 positions); the memory of the verifier that is meant to be affordable to an honest participant would not. | 6 |

## 3. Formulas (executable: `opv-meas derive --in inputs.json`)

### 3.1 The clock, with the registered model off it (ADR-0177 D7)

```text
T_challenge(m) >= T_beacon + T_fetch(m) + T_check(m) + T_localize + T_file + T_margin          (the user's 2026-10-08 ruling)

T_fetch(m)  = lambda + m * M_pos / B_eff                the CLAIM's public material only (M_pos per position, m positions checked,
                                                         B_eff = min(link, provider egress, ~690 MB/s hash-check ceiling per core (M))).  PROTOCOL term.
T_acquire   = lambda + A / B_eff                        the registered MODEL (A artifact bytes): an OFF-CHAIN ACQUISITION ASSUMPTION.
                                                         Reported beside the window, never inside it.
T_check(m)  = (W + m * rho * M_pos) / speedup           W = the fixed part (95.4 s at 0.5B, D) of which W_auth = the weight pass that
                                                         authenticates the verifier's own copy against the registered root (41.2 s, D);
                                                         rho = 0.29 us/byte (D); speedup = 1 (the reference verifier)
T_localize  = k * n_row * c_mac  (a matmul row) | ~0 (an element court)                          0.5B lm_head row: 1.6-2.3 s (D)
T_file      = assembly + encode + strict decode + court + carrier inclusion                       assembly/court: ms (M); carrier: policy, unmeasured
T_margin    = reorg_slack + margin_frac * (the sum)                                               margin_frac = 0.35 (median wall/CPU, M)
DAA budget  = ceil(seconds / s_daa),  s_daa >= 120 (a DAA step needs a 120 s slot, rfc-0012-policy-proposal.md; 125-150 cited, not measured here)
```

Why the model is out of the window. The chain no longer guarantees that a verifier can obtain a registered model (ADR-0177 D1, D7): distribution is
off-chain and optional, and no reward, weight or slash depends on a model being served. So

* **Claim material stays a protocol term.** The claim's committed values, openings and demanded positions are public, authenticated and
  claim-specific; their fetch time is `T_fetch(m)`, measured (§2.3) and priced below.
* **The model is a precondition of checking, not a step of it.** A verifier either holds the model when the claim is accepted, or acquires it first.
  `T_acquire` is reported for every class (`t_acquire_registered_model_s_off_chain`), and it matters in exactly one case, a *late acquirer* that
  decides to verify a claim already in its window: it can take part only if `T_acquire + T_challenge` fits that window. For an open model on a
  100 Mbps–1 Gbps link and a class of the sizes here (0.66–1.87 GB) that is 5–150 s; a 9B at 11.7 GB (A) is 94 s at 1 Gbps and ≈ 940 s (8 DAA) at 100 Mbps (all D from an assumed link; the link is not
  measured, F3b); for a closed model it is unbounded and no formula applies.
* **Authenticating the verifier's own copy stays in `T_check`.** The court takes a model operand from the verifier's own copy, authenticated against the
  registered root (ADR-0177 D2). The reference verifier does that on every check (the weight pass, `W_auth`, 31 % of the 0.5B fixed part; F-MEAS-05).
  ADR-0175 makes *authenticate once, when acquired* sound — a registered model never changes — so `derive` takes a labelled-**A** switch,
  `model_auth_once`, that moves `W_auth` out of the window. No implementation does this today; the default is the reference verifier.
* **The interim budget field names the artifact.** `OpvBudgetsV1::cold_material_daa` is documented as "fetching the cold class / artifact / claim
  material". Under D7 it prices claim material only; the model's bytes leave it (F-MEAS-08). That is a wording and sizing change for the OPV lanes, not code
  written here.

`T_beacon` is **0** for the outsider's private-salt check (the only soundness-bearing path today). A public-coin per-claim check (SG-05) would add the
beacon's lock latency after the commit; with the interim onboarding shape (anchor delay 2 DAA + beacon window 120 DAA) that is ≥ 122 DAA, 2.4× the interim
50-DAA window — a public-coin post-commit challenge cannot ride a beacon of that shape inside an OPV window (an input to OPV-BOOT, not decided here).

Mapping to the policy's budgets (`opv.rs`): `T_fetch → cold_material_daa`, `T_check → check_daa`, `T_localize → localize_daa`,
`T_file → court_daa + carrier_daa`, `T_margin → reorg_slack_daa`. `derive` builds the budgets from the measured terms and runs the kernel's own
`OpvPolicyV1::validate` on them, so a derived window is judged by the relations the node enforces.

### 3.2 Detection is conditional on holding the model

```text
c          = m / P                                     coverage of ONE verifier whose positions are drawn uniformly and unpredictably
P_det|acq  = 1 - prod_{i in H} (1 - q_i c_i)           H = the verifiers that HOLD the registered model when the claim is accepted;
                                                        q_i = P(verifier i is present and checks)                         [H, q_i, independence: NOT MEASURED]
P_dc       = P_det|acq * (1 - eps_enf) * kappa         detected AND convicted AND collected; kappa = collected / nominal slash   [eps_enf, kappa: NOT MEASURED]
p          = P(H is non-empty) * P_dc                  the effective probability; H = {} (a closed model) => p = 0
```

Every `P_det` and `P_dc` in this note is **conditional on acquisition** (`P_det|acq`). It is never reported as *the* detection probability of a class.
The tool prints three situations for every row: the verifiers hold the model (`model_held_by_the_verifiers`), a stated share of them does (`H` smaller than `n`,
the `in-v2-three-of-ten-hold-the-model` run), and **nobody but the producer does** (`closed_model_no_third_party_holds_it`, `H = {}`, `p = 0`).
For the last one the collateral is `none`: no finite reservation is enough (the premise's §4 and the 2026-10-08 ruling), the deterrable reward rate of §3.4 is 0,
and ADR-0177 D7's reading applies — the model's economic safety is **unestablished**, not established by the caps. This note does not decide whether a
closed model may earn; it records that the measured timings price a verifier who exists and holds the model, and that the chain does not provide one.

### 3.3 Collateral: the loss is the slash net of the reporter's share (ADR-0032 49 %, ADR-0176 D6)

```text
r       = 0.49 (4,900 bps): the reporter's share of a collected slash. A producer that reports itself recovers r of what it is slashed.
S_coll  = min(reservation, the bond's collateral at conviction)      ledger.rs convict(): `slashed = reserved.min(producer_collateral)`; never the nominal slash
L_net   = (1 - r) * S_coll                                          what a conviction costs such a producer: >= 51 % of the collected slash, NEVER the gross slash

(a) opportunity form (what the kernel's relation and SOUND row K size):
    R  >=  ceil( max( G + default_penalty , ceil( G / P_dc ) ) / (1 - r) )          none when P_dc = 0
(b) cost-saving form (ADR-0176 D6, equal opportunities):
    P_dc * ( R_risk + (1 - r) * S_coll )  >  C_saved            so the effective detection that suffices is   P_dc* = C_saved / ( R_risk + (1 - r) S_coll )
```

`G` is a claim's maximum gain (Final reward + work credit + the stated external bound: 5 + 5 + 10 = **20 BILI** at the interim terms, a policy value);
`R_risk` is what a conviction forfeits of the reward (reward + credit, 10 BILI interim); `C_saved` is the compute a forged claim does not spend.
(a) is the sizing rule: it treats the whole gain as the fraud's profit (a producer with no honest alternative) and divides by `P_dc` instead of `P_dc/(1-P_dc)`,
which is conservative and reproduces SOUND's row K at `r = 0.5`. (b) is the marginal test of a producer that could also compute honestly: it needs `C_saved`, which
this note measures only as reference-evaluator CPU seconds (§7, item 8) and cannot price in BILI.

Two things the 49 % changes, both computed (§4.3):

* the divisor `1/(1-r)` is 1.961 instead of the interim ledger's 2.0 (`accuser_reward_permille` is 500 there, a constant of the K2/OPV route that is not ADR-0032's 4,900 bps; which one the
  route follows in the single release is open: INTF / G14R, F-MEAS-07);
* **counting the gross slash would under-reserve by 1.96×** (20,480 BILI instead of 40,157 at `P_dc = 8/8,192`) — and the kernel's own relation
  (`OpvPolicyV1::required_reservation`) does exactly that: it asks `reservation ≥ max(G + penalty, G/P_dc)` with no reporter return (F-MEAS-07).

### 3.4 Exposure and the deterrable reward rate of a bond (the ceiling BUDGET's caps must respect)

```text
live claims a bond holds     = floor( C_b / R )                           the ledger reserves FREE collateral, so one collateral is never counted twice
peak reserved collateral     = (claims admitted per DAA) * L_h * R        L_h = the liability horizon in DAA: window + post-Final liability = 50 + 200 = 250 at the interim terms (D)
deterrable reward rate       rho_R  <=  P_dc * (1 - r) / L_h              BILI of attributable gain per DAA, per BILI of bond
```

The third line is the aggregate form of (a). A bond `C_b` that has carried undetected gain over `L_h` can be slashed once, for at most `C_b`, with the net loss
`(1-r) C_b` at probability `P_dc`; for a risk-neutral producer the condition is `rho_R * L_h * C_b <= P_dc (1-r) C_b`. It does not matter whether detections of one producer's claims are
correlated (the expected slash is the same); it matters only for a producer that does not mind variance. The claim-capacity multiplier `rho` is absent on purpose:
raising it shrinks one claim's gain (`A -> A/m`, ADR-0176 D2) and leaves the rate where it was. Read as a ceiling on ADR-0176's `R_max(C, W)`:
**`R_max(C, W) <= P_dc (1 - r) C * W / L_h`** for `W <= L_h`. With `P_dc = 0` (a closed model) the ceiling is 0: no positive reward rate is deterrable, and the caps then bound the loss to the network, not the producer's incentive to cheat.

## 4. Worst-case deadlines for G14 condition C8 (G14C GAP-07)

G14C's matrix (`g14-completion-matrix.md`, `g14/completion` @ `82a176a77`) leaves C8's last cell, "the measured worst-case deadline", to this lane:
`T_challenge ≥ T_beacon + T_fetch + T_check + T_localize + T_file + T_margin`, per class and profile, for a **fresh, non-seat, bonded verifier that holds the registered
model** (ADR-0177 D7; C10) to fetch the claim material, check, localize and file. This section answers it with what ran, what is derived, what is assumed, and what cannot
run on this host or has no real artifact. The machine-readable form is `opv-measurements.json` → `gap07_worst_case_deadlines`.

### 4.1 What a deadline is here

```text
T_challenge(claim) = ( T_fetch(claim material) + T_check_to_verdict(worst lie: the LAST position, localization inside it) + T_file ) × 1.35  +  (carrier 2 + reorg 2) DAA × 120 s
```

* **Worst case.** The lie is a matmul in the last position's last occurrence (`late`): the check reads the whole claim before it finds the fault, so check-to-verdict is a whole pass plus
  localization (`check_outsider` runs to the verdict, localization inside it). An honest claim costs the same pass without the localization. The claim is the largest size of the row.
* **CPU seconds, times the measured wall/CPU of 1.35 (M, median).** The host was shared (load 37–220 during the 0.5B samples), so CPU time is the comparable figure; the wall time of the same samples is
  in the JSON (1.1–2.0× CPU). `T_beacon` is 0 for the outsider's private-salt check. Carrier and reorg are the policy's 2 + 2 DAA, **not measured** (F6). `s_daa` = 120 s is a lower bound (F1).
* **The windows it is compared with** are the interim policy constants, not production values: the **OPV** window is 50 DAA (`OpvWindowV1` 40 base + 10 horizon); in the **Panel-licensed** profile with every
  seat colluding the tally can pass as soon as checking starts (the Panel's 100-DAA check window is a maximum to the pass, not a minimum), and a dispute is refused after `pass + challenge_window` = 50 DAA
  (`lifecycle.rs`: `ProbabilisticPass.window_end`; `window_open`), so the adversary-minimum window is the same 50. The
  hard deadline (window + court 20 + grace 10) is 80 DAA; post-Final liability is 200 DAA (conviction is still possible, Final is no longer preventable). The kernel's own `validate` asks the first filing to be
  *included* inside the window; the user's formula, used here, also puts localization and filing inside it, which is stricter.
* **A real class registers only Panel-licensed at the interim carriers** (F-MEAS-01): the verifier's cost is the same under either profile; only the registration mode and the window rule differ.

### 4.2 By family and profile

| Family | Profile | Worst-case deadline | What exists / what is missing |
|---|---|---|---|
| **F1** K2-TIR v1/v2 single-program | Panel-licensed (every seat colluding) and OPV | **M** for Qwen2.5-0.5B at P = 3, 8, 16 and 32 (§4.3; P = 64 attempted, stopped by RAM); **A** for the four other real classes (§4.4: weight pass measured, the rest carried from the 0.5B fit) | the reference verifier needs ≥ 16 B/param of RAM: 0.8B ≥ 12.3 GB, 1B ≥ 19.9, GLM-1.5B ≥ 23.8, 1.7B ≥ 27.6 GB; hosts for fleet plan F2 |
| **F2** K2-TIR v3 pipeline / media | OPV | **unmeasured**: V-ref only, no real pipeline claim, no onboarding path (GAP-20, GAP-21). **D** structure: the sum over stages of each component's `T_check`; edges at inclusion; one logits tensor per edge filing | a real pipeline claim on a node (K2S wire form) |
| **F3** K2-TIR v4 segmented real-scale (8k, 262k, 2M) | OPV only | **unmeasured**. The only real artifact would be H1's 9B pack (not built); **A** rows for 9B-8k are in §5.4: a whole claim needs 41,588 DAA (≈ 58 days) from the reference verifier; 2M is refused by the gate | the 9B pack, a ≥ 160 GB host, the K2S v4 node |
| **F4** K2-TIR v5 encoders / task heads | kernel route | **unmeasured**: the K2S V-unit test fails at the commit; no node test | K2S GAP-40 |
| **F5** private / fused material | — | **no deadline**: never registers (fail-closed) | — |
| **F6** typed roots (Memory / Retrieval / Composite) | OPV + `palw_typed_roots_v1` | **unmeasured**: no real artifact of any kind exists. **D** structure and unit costs: §8 | a real memory class, retrieval snapshot, composite |
| **F7** EXEC work slices | `palw_exec_payload_v2` | **unmeasured**: a slice's kernel claim has the F1/F3 deadline of its own size; leg continuity and suffix void have no timing | X8R's slice node E2E on a real class |
| **F8** onboarding conformance (tag 109), artifact binding | the reward gate | **no outsider deadline on the complete-check path** (judged in the fold: node CPU per block, not measured here); the sampled path cannot gate rewards | node-side per-block timing (a consensus lane) |
| F9–F16 legacy V2 Panel route | exempt `LEGACY_PANEL_ROUTE` | not G14; the live windows are receipt 600 / challenge 1,200 DAA (**D**, code reading in `t12-bond-reuse-audit-2026-10-10.md`) | — |

### 4.3 F1, Qwen2.5-0.5B: measured

| P | what ran (label) | `T_fetch` claim material, CPU s | `T_check` to the verdict, CPU s (localization inside it for a lie) | `T_localize` | `T_file` | wall of the check, as measured under load | `T_challenge` = (sum) × 1.35 + 4 DAA | DAA | fits 50 / 80 DAA | peak RSS |
|---|---|---|---|---|---|---|---|---|---|---|
| 3 | M: `late` lie, outsider path (n=2); fetch n=16 | 0.26 | 164.8 | in the check | 0.01 | 293.5 s | 703 s (11.7 min) | 6 | yes / yes | 10.4 GB |
| 8 | M: honest pass (fresh path); localization A (carried) | 0.65 | 250.7 | 23.5 (A) | 0.10 | 498.8 s | 851 s (14.2 min) | 8 | yes / yes | 6.9 GB |
| 8 | M: honest pass (fresh path); localization A (carried) | 0.76 | 247.1 | 23.5 (A) | 0.10 | 469.3 s | 847 s (14.1 min) | 8 | yes / yes | 10.5 GB |
| 16 | M: honest pass (fresh path); localization A (carried) | 1.35 | 373.7 | 23.5 (A) | 0.10 | 663.7 s | 1,018 s (17.0 min) | 9 | yes / yes | 10.4 GB |
| 16 | M: honest pass (fresh path); localization A (carried) | 1.52 | 368.0 | 23.5 (A) | 0.10 | 752.3 s | 1,011 s (16.8 min) | 9 | yes / yes | 10.6 GB |
| 32 | M: `late` lie, outsider path (n=1), 2026-10-10 | 2.89 | 624.2 | in the check | 0.03 | 750.7 s | 1,327 s (22.1 min) | 12 | yes / yes | 13.2 GB |

**P = 64 (the gate's evaluation point) was attempted and did not complete.** The producer's world built (M: 486 s CPU of reference evaluation, 15.6 GB peak RSS, 3,879,431,872 B of claim material =
60.6 MB per position, a 27 MB chain, a 59.4 MB disclosure object), but the verifier passed **15.1 GB of RSS** about three minutes into its check and the harness watchdog, which I set at 14 GiB to protect the shared host,
stopped it (exit 86; raw record `opv-measurements-data/gap07-attempts.json`). There is **no sample**, so the P = 64 deadline is the fit (18 DAA, D), not a measurement.

Reading.

* **The measured worst case is P = 32:** 624 CPU-s to the verdict (the whole pass plus localization, filing and court, 31,492 bytes), 22 min with the 1.35 margin, **12 DAA against the 50-DAA window and the 80-DAA hard deadline**.
  It came in 6 % (40 s) below the honest-pass fit (≈ 664 s for the outsider path), on a quieter host (load 15–26 against 37–220 for the fit's samples).
* **Time is linear in positions** (≈ 95 s + 17.5 s per position, §2.4), so the interim window ends at about 226–230 positions (D): a 0.5B claim of up to that size is completely checkable in time.
* **RAM binds before the window does.** Peak RSS is 10.4 GB at P ≤ 16, **13.2 GB at P = 32** and more than 15.1 GB at P = 64. At 0.1–0.17 GB per position (D from two points; `ru_maxrss` undercounts compressed pages) a 226-position claim
  needs about 30–40 GB, beyond a 32-GiB host: the reference verifier's memory, not the clock, caps the claim size on this hardware (F-MEAS-10).
* **Claim material per position** is 59.6 / 60.1 / 60.6 MB at P = 3 / 32 / 64 (M): +1.7 % over a 21× larger claim at these tiny histories. It will grow with history; nothing here measures a long one.
* **The worst lie's filing is 31,492 bytes at P = 3 and at P = 32** (M): independent of the claim size, and far inside the carrier.

### 4.4 F1, the four other real classes: assumed

No check of these ran (the reference verifier cannot be held in 32 GiB shared); what is measured is the weight pass (a floor) and the claim material per position at P = 3. The rest
is the 0.5B fit carried over, **A** — shown so that a measurement can replace it, never used as a result.

| Class (no check of it ran: A) | P | `T_challenge` whole claim | DAA | fits 50 DAA | positions within the window |
|---|---|---|---|---|---|
| qwen35-0.8b | 3 | 22.5 min | 12 | yes | 3 |
| qwen35-0.8b | 64 | 3.9 h | 117 | NO | 25 |
| llama32-1b | 3 | 16.4 min | 9 | yes | 3 |
| llama32-1b | 64 | 44.8 min | 23 | yes | 64 |
| smollm2-1.7b | 3 | 18.6 min | 10 | yes | 3 |
| smollm2-1.7b | 64 | 61.3 min | 31 | yes | 64 |
| glm-edge-1.5b | 3 | 17.4 min | 9 | yes | 3 |
| glm-edge-1.5b | 64 | 56.3 min | 29 | yes | 64 |

Qwen3.5-0.8B is the outlier: its claim material is 522 MB per position (8.8× the others, F-MEAS-04), which is what makes a 64-position claim miss the window under the carried fit.

### 4.5 What one reference verifier reaches inside the window (D from M; extrapolations A)

| Claim | positions one verifier can check inside the 50-DAA window | coverage `c` | whole claim needs | with the verifier authenticating its model once (A) |
|---|---|---|---|---|
| 0.5B, P = 3 (D, the fit; sampled in 4.3) | 3 | 100.0 % | 6 DAA (11.9 min) | - |
| 0.5B, P = 16 (D, the fit; sampled in 4.3) | 16 | 100.0 % | 9 DAA (17.0 min) | - |
| 0.5B, P = 64 (D, the fit; not sampled: 4.3) | 64 | 100.0 % | 18 DAA (35.9 min) | - |
| 0.5B, P = 256 (extrapolation, A) | 226 | 88.3 % | 56 DAA (111.5 min) | 229 (89 %), whole claim 56 DAA |
| 0.5B, P = 1,024 (extrapolation, A) | 226 | 22.1 % | 207 DAA (6.9 h) | - |
| 0.5B, P = 8,192 (extrapolation, A) | 226 | 2.8 % | 1,619 DAA (2.2 days) | 229 (3 %), whole claim 1,618 DAA |

A claim larger than the "positions one verifier can check" is not completely checkable in the interim window by the reference verifier; it is covered at `c < 1` (§5 prices that) or the window must grow.
A verifier that authenticates its model once (right-hand column, **A**; sound because registered models are immutable, ADR-0175) buys only a few positions: the weight pass is a small part of the
cost, which is why §2.5's conclusion stands: **the window numbers price this implementation**.

### 4.6 What is not a deadline here

* **The disclosure path.** A demanded position's disclosure is a 59,445,032-byte `Respond` object against the 1,583,616-byte interim carrier cap (M); no real class can carry one, so `disclose_daa` and the proof-grace
  branch of `validate` cannot be timed (F-MEAS-02, F-MEAS-06).
* **A filing that does not fit.** A lie in the embedding gather is found in 13 s (M) but its filing is 272 MB and the strict wire decode refuses it (ceiling 67 MB). The deadline of a fault that cannot be filed is not a number.
* **Fetch under withholding.** If the producer withholds the claim material, the fetch does not complete; the clock is the demand's deadline (court 20 DAA) and the default, not `T_fetch`.
* **Model acquisition** (`T_acquire`) is off the clock by ADR-0177 D7 (§3.1).

## 5. Collateral applied (D from the formulas of §3; the detection probabilities are inputs, not results)

### 5.1 The reservation as a function of a stated `P_dc`, net of the 49 %

`R = ⌈ max(120, gain / P_dc) / 0.51 ⌉` BILI at gain 20, default penalty 100, reporter share 49 %. The table is the formula only: it needs `P_dc`, which is **not measured**, and which is
conditional on a verifier holding the model (§3.2). The 500-permille column is the interim ledger's constant, shown as a comparator (it reproduces SOUND's row K).

| `P_dc` (conditional on acquisition) | reservation `R`, net of the 49 % (BILI) | the same at the interim ledger's 500 permille (BILI) | × the interim 1,000 | gain the interim 1,000 defends (BILI) | live claims a 13,000-BILI bond holds | deterrable reward / DAA / BILI of bond |
|---|---|---|---|---|---|---|
| 1 | 235.29 | 240.00 | 0.24 | 510.00 | 55 | 0.00204 |
| 1/2 | 235.29 | 240.00 | 0.24 | 255.00 | 55 | 0.00102 |
| 1/10 | 392.16 | 400.00 | 0.39 | 51.00 | 33 | 0.000204 |
| 1/100 | 3,921.57 | 4,000.00 | 3.92 | 5.10 | 3 | 2.04e-05 |
| 1/1,000 | 39,215.69 | 40,000.00 | 39.22 | 0.51 | 0 | 2.04e-06 |
| 8/8,192 | 40,156.86 | 40,960.00 | 40.16 | 0.50 | 0 | 1.99e-06 |
| 1/10,000 | 392,156.87 | 400,000.00 | 392.16 | 0.05 | 0 | 2.04e-07 |
| 0 (closed model) | none | none | none | 0.00 | none | 0 |

Reading. At `P_dc = 1` the floor (`gain + penalty`) binds; below `P_dc ≈ 1/6` the detection term does. A 13,000-BILI bond (the t12 producer minimum, **D**: code reading) holds 55 reservations at `P_dc = 1` but **none** below
`P_dc ≈ 1/330`; the interim 1,000 BILI defends a 20-BILI gain only when `P_dc ≥ 20 / (1,000 × 0.51) = 3.9 %`. The last column is the §3.4 ceiling on attributable reward per DAA per BILI of bond for the
interim 250-DAA liability horizon: it falls in proportion to `P_dc` and is exactly 0 for a closed model.

**The cost-saving form (b), ADR-0176 D6** (**A**: `C_saved` is not measured in BILI). With `R_risk` = 10 BILI (the reward and credit a conviction forfeits) and the loss `(1 − r) S`:

| reservation `S` | net loss `0.51 S` | effective detection that suffices, `P_dc* = C_saved / (10 + 0.51 S)`, at `C_saved` = 0.5 / 2 / 5 BILI |
|---|---|---|
| 1,000 (interim) | 510 | 0.10 % / 0.38 % / 0.96 % |
| 40,157 (9B-8k deterrence-only) | 20,480 | 0.0024 % / 0.0098 % / 0.024 % |

The two forms answer different producers (§3.3): (b) needs very little detection when the reward is priced near the compute it pays for; (a), which the kernel and the dossier use, asks the collateral to cover the whole
gain over `P_dc`, and is the stricter. For a closed model both fail: `P_dc = 0`, so `0 > C_saved` is false and cheating saves `C_saved` per claim with no expected loss (bounded in total by ADR-0176's reward cap, not removed).

### 5.2 Who holds the model (ADR-0177 D7): the same claim, four situations

Qwen2.5-0.5B, an 8,192-position claim, a verifier that checks the stated share of positions (coverage), `P_dc` and the reservation it implies.
`n = 10, q = 0.5` are assumptions (**A**), as are the holder counts; the only measured thing in the table is what one holder pays.

| checked | one verifier, present, holds the model | 3 of 10 hold it (q = 0.5) | all 10 hold it (q = 0.5) | nobody but the producer holds it |
|---|---|---|---|---|
| 1 position (0.012 %) | `P_dc` 0.0001221: R = 321,255 | `P_dc` 0.0001831: R = 214,183 | `P_dc` 0.0006102: R = 64,269 | `P_dc` 0: R = none |
| 1 % of positions | `P_dc` 0.009888: R = 3,966 | `P_dc` 0.01476: R = 2,657 | `P_dc` 0.04835: R = 811 | `P_dc` 0: R = none |
| 10 % | `P_dc` 0.09998: R = 392 | `P_dc` 0.1426: R = 275 | `P_dc` 0.4012: R = 235 | `P_dc` 0: R = none |
| 50 % | `P_dc` 0.5: R = 235 | `P_dc` 0.5781: R = 235 | `P_dc` 0.9437: R = 235 | `P_dc` 0: R = none |
| whole claim | `P_dc` 1: R = 235 | `P_dc` 0.875: R = 235 | `P_dc` 0.999: R = 235 | `P_dc` 0: R = none |

Reading. Detection rises with the verifiers that hold the model and are present, and the collateral falls with it — and it is **zero detection and no finite collateral** when the model is closed to everyone but the producer.
Nothing in the chain, and nothing measured here, says which column a given model is in. ADR-0177's allocation pays bonded capital, not holding; whether more capital means more verifiers that hold the model is exactly what is unmeasured (F8). This note only prices each column.

### 5.3 Bond-level exposure (§3.4)

At the interim terms a bond of 13,000 BILI holds 13 live reservations of 1,000 BILI and, with the interim live caps, a producer needs 3,000 BILI free for its 3 and the ledger locks at most 32,000 BILI for 32. At 9B-8k's deterrence-only
reservation (§5.4) the same bond holds **no** claim: the bond, not the cap, is what stops a 9B-8k claim. The deterrable reward rate for a 13,000-BILI bond over the 250-DAA horizon is
`13,000 × P_dc × 0.51 / 250` BILI per DAA: **26.5 BILI/DAA at `P_dc = 1`**, 2.65 at 1/10, and **0.0259 BILI/DAA at `P_dc = 8/8,192`** (D): the ceiling ADR-0176's `R_max(C, W)` has to respect for a bond of that size.

### 5.4 The 9B-8k case (SOUND SG-06) — **assumed rates, shown for scale only**

Inputs: P = 8,192, public material 1.54 GB per position (K2S / `coverage-p1p2-record.md`, shape-level probe: 12.6 TB per claim; the v4 estimate is ≈ 16 TB), artifact ≈ 11.7 GB (**A**: 1.3 B/param × 9.0 B; the five real
containers are 1.08–1.37 B/param, M), check fixed part 190 ns/param × 9 B = 1,710 s (**A**: the 0.5B ratio carried to 9B) and 0.29 µs/byte × 1.54 GB = 451 s per position (**A**: the 0.5B rate carried to another architecture).
None of these was measured on the 9B (H1's pack is not built); they exist to show which direction and how far. The registered model is **not** in the window (§3.1): at 11.7 GB it is an off-chain acquisition of 94 s at 1 Gbps (A, link).

| Verifier (A: nothing here ran on a 9B) | positions in the interim window | coverage | whole claim | `R` at `P_dc = c`, net of 49 % (BILI) | at 500 permille | live claims a 13,000-BILI bond holds |
|---|---|---|---|---|---|---|
| reference, single core, no link | 5 | 0.061 % | 41,588 DAA (57.8 days) | 64,251 | 65,536 | 0 |
| reference, 1 Gbps link (A) | 5 | 0.061 % | 42,724 DAA (59.3 days) | 64,251 | 65,536 | 0 |
| 10x faster (sensitivity, A), no link | 86 | 1.050 % | 4,163 DAA (5.8 days) | 3,736 | 3,810 | 3 |
| 10x faster, 1 Gbps (A) | 67 | 0.818 % | 5,298 DAA (7.4 days) | 4,795 | 4,891 | 2 |

Reading. With the reference verifier the interim window reaches 5 of 8,192 positions (the same order as SOUND's "8 of 8,192" row), and complete coverage inside any window of the interim size is out of reach by three orders of magnitude;
reading the claim alone takes 28 h at 1 Gbps for 12.6 TB (35.6 h for 16 TB) and 2.8 h at 10 Gbps (3.6 h), (D, no CPU at all), i.e. ≥ 84 DAA even with an infinitely fast CPU. **Deterrence-only collateral, net of the 49 %: ≈ 40,157 BILI
(40,960 at the ledger's 500 permille) — 40× the interim reservation, and 3.1× a 13,000-BILI bond.** Condition 5 cannot be met by collateral at this scale unless `gain` per claim is capped to what `R (1 − r) P_dc` defends
(0.50 BILI for the interim 1,000 at 8/8,192), or the window is extended by orders of magnitude, or detection is made cheaper than re-reading the claim (K2S's sublinear-read question; SG-06 stays open).

## 6. What the numbers say about the premise's conditions

* **§6.5 — collateral matched to maximum gain and a real detection probability.** Not derivable yet: the formula needs `P_dc`, and the only measured factor is one reference verifier's coverage (§4.5) — and only for a verifier that holds the
  model. For small claims (P ≲ 226 at 0.5B) one verifier covers everything inside the interim window; for 8k-position claims it covers 3 % (0.5B) or 0.06 % (9B, A). For a closed model there is no finite collateral (§3.2, §5.2).
* **§6.6 — an honest verifier can take part in time, with its resources.** The reference verifier needs ≥ 16 B/param of RAM (≥ 144 GB at 9B), 95 s + 17.5 s per position of CPU at 0.5B, and an implementation that does not exist yet (§2.5).
  Cost recovery per claim, from measurements: **140 CPU-seconds and 0.84 GB of opened data for a 3-position 0.5B claim; ≈ 370 CPU-seconds for 16 positions** (M, ±16 s); 5.6× the reference producer's compute at P = 3 (D). ECON prices the bounty
  from `opv-measurements.json`; the reporter's 49 % is the verifier's revenue on a conviction, and the acquisition of the model is its own, off-chain, cost (F3b).
* **§6.7 — the dispute clock (G14 C8).** §4: the measured worst case fits the interim 50-DAA window for the 0.5B class up to the sizes of §4.3; beyond that it does not, and the disclosure branch cannot be exercised by any real class because
  the disclosure does not fit a carrier (F-MEAS-02, -06). Every other family is unmeasured (§4.2).

## 7. ADR-0176 §4 measurement plan (a PLAN: none of it ran)

ADR-0176 §4 lists the quantities that decide whether the common bond window `W` can be set. Every item below states what it is, what is known **today**
(with a label), how it will be measured, and what it needs first. **All of it needs lane BUDGET's engine** (`palw_bond_budget_v1`, dormant; no commit on
`budget/adr176-177` yet), because the counters it reads (`Q_used`, `B_used`, `R_used`, `F_used`, the reservation and `reuse_not_before` per bond) are the engine's.
Until that engine exists every reward and weight fence stays refused and this plan cannot be executed, only prepared.

Run matrix (one devnet, H1's 9-node network or a fresh one, no live host): bonds {13,000; 100,000 BILI} × claim capacity `rho` {10; 1,000; 10,000} ×
producers {honest; fast-forging (the same claims without the compute); withholding} × 3 consecutive windows `W`, a saturating producer at each cell.
Pass criteria are ADR-0176's own acceptance tests: same bond, same window, same `rho` ⇒ the same `Q/B/R/F` for the honest and the forging producer;
`rho × 100` leaves `B/R/F` unchanged; no early recovery before `d + W`.

| # | ADR-0176 §4 item | Unit | Today | How it will be measured | Needs |
|---|---|---|---|---|---|
| 1 | claims per DAA, per unit of bond | claims / DAA / BILI | **D** (a code reading of `palw_issuance_slots_v1`, `t12-bond-reuse-audit-2026-10-10.md`, not a fleet observation): `u = floor(C / 6,500 BILI)`, refill `u · rho / 20` claims/DAA, so a 13,000-BILI bond gets 1 claim/DAA at `rho = 10` and 100 at `rho = 1,000` (7.7e-5 and 7.7e-3 per BILI per DAA) | the engine's `Q_used(b, I)` over each window of the run matrix, honest and forging | BUDGET |
| 2 | reward blocks per DAA, per unit of bond | blocks / DAA / BILI | unmeasured: claim-backed blocks (RFC-0008) are dormant | `B_used(b, I)`; needs the EXEC_SLICE producer | BUDGET, X8R |
| 3 | reward per DAA, per unit of bond | BILI / DAA / BILI | **A**: the interim claim reward is 5 BILI + 5 BILI credit per claim (policy), so item 1 × 10 BILI | `R_used(b, I)` | BUDGET |
| 4 | Final credit per DAA, per unit of bond | weight / DAA / BILI | unmeasured; the live Final path adds the full contribution (`palw_weight_final_safe_v1`) | `F_used(b, I)` against the weight the fork-choice reads | BUDGET, FINX |
| 5 | liability period and maximum concurrent exposure | DAA; BILI | **D** (interim policy): liability horizon `L_h = 50 + 200 = 250` DAA; live caps 3 claims per producer and 32 in all, so ≤ 3,000 BILI per producer and ≤ 32,000 BILI locked at the 1,000-BILI interim reservation. Production values: not chosen. The formula is §3.4: peak reserved = (claims/DAA) × `L_h` × `R` | the ledger's reserved collateral per bond over time for a saturating producer, against the formula | BUDGET |
| 6 | inspection and court resources | CPU-s, RAM, bytes per claim; cores per DAA for coverage `c` | **M** per claim, five classes (§2): Qwen2.5-0.5B P=3: 140 CPU-s, 10.4 GB RSS, 0.84 GB opened; late-lie filing 31 KB, court 6.9 ms. **D** at a rate: cores to check every claim = `claims/DAA × T_check_cpu / 120 s`, i.e. **1.2 cores** for one 13,000-BILI bond at `rho = 10` (1 claim/DAA) and **117 cores** at `rho = 1,000` (100 claims/DAA) for a 3-position 0.5B claim, divided by `1/c` when only a fraction `c` is checked | the harness per class (fleet plan F2) and the engine's claims/DAA (item 1) | BUDGET; hosts for F2 |
| 7 | effective detection and collection rate | probability; collected / nominal slash | unmeasured. Collection: `ledger.rs convict()` slashes `min(reserved, bond collateral)`, so the rate is 1 unless the bond was slashed elsewhere (**D**, code). Detection: nothing on chain records a clean check (RFC-0015 §4.3) | a voluntary signed "checked scope" attestation off chain, or a pilot with known watchers (F8), against injected, known-bad claims; `slashed / nominal` from the ledger events | a pilot design (a decision), BUDGET |
| 8 | compute cost | CPU-s per claim; BILI | **M** (reference evaluator, 3 tokens): 24.9 s (Qwen2.5-0.5B), 57.1 s (Qwen3.5-0.8B), 71.4 s (Llama-3.2-1B), 78.8 s (SmolLM2-1.7B), 67.4 s (GLM-Edge-1.5B). The reference **verifier** costs 5.6× the reference producer at P=3 on the 0.5B class (140.4 / 24.9, **D**). Production kernels on accelerators: unmeasured. BILI conversion needs an external energy and hardware price (**A**) | the producer's `honest_trace` phase per class on the real hardware miners use (F5), plus a stated price | a price input (decision) |

What the plan does not do: it does not choose `W`, the caps or the reservation; it produces the numbers the choice is made from. Items 1–5 are accounting
identities of the engine and are exact once it exists; items 6–8 are the ones that need a real network, real hardware and a pilot.

## 8. RFC-0004 Part II: inspection cost kind by kind (Memory, Retrieval, Composite) — UNMEASURED

**No real artifact of any of the three kinds exists** in this repository or in H1's evidence (`wh-h1-run/cache` holds weights classes only). The only typed
inputs are the synthetic sketch fixtures `memory_v1` and `memory_ttt_v1` (`misaka-palw-tir-sketch`: a 32-token vocabulary, width 16), whose cost is process overhead, not a
measurement of anything an honest verifier would pay. So **no per-kind `T_check` is reported as measured**. What is stated is the structure a measurement will fill,
from `rfc-0004-part2-typed-roots.md` §2.6, §3.5, §4 and `misaka-palw-kernel/src/spec/mod.rs` (`memory_bounds_v1`, `retrieval_bounds_v1`,
`retrieval_claim_material_bytes_v1`, `composite_bounds_v1`), with the unit costs of §2.1 (**M**) where they apply. ADR-0175 and ADR-0177 cover every typed root: a Memory
class's base weights and `M0`, and a Retrieval snapshot, are registered immutable artifacts the chain does not make obtainable, so their bytes are `T_acquire` (off chain), and only the
claim's own material is `T_fetch`.

| Kind | Claim material (protocol, `T_fetch`) | `T_check` structure | `T_localize` / filing | Acquired off chain (`T_acquire`) | Example size (**A**, illustrative only) |
|---|---|---|---|---|---|
| **Memory** (`S` steps, slots of `M` bytes, rule program class) | step 0's pre-state is on chain; steps `i ≥ 1` and every committed value ride the claim: `S × base material + (S + 1) × M` | `S` step checks of the rule program (**D**: `S × T_check(rule program, one step)`; the steps are independent given the committed pre-states and can run in parallel), plus authenticating the carried post-state: **24.2 ns/element (i8, M)**, which is `M × 24 ns` for `M` bytes | one step: a step fault is a kernel fault, so the 0.5B figures of §2.3 apply to the rule program class (seconds); response `max(base, M + slots·128 + 128)` | the base weights and `M0` | `M` = 64 MiB, `S` = 8: 1.6 s to authenticate a carried state (D from M); the step checks have no real class to price |
| **Retrieval** (`N` items, key length `D`, payload ≤ `P`, top-`k`) | the result (`k` entries) and the opened items; a withheld slice is the producer's default (slice = `B` items) | detecting a *missed* item scans the verifier's own snapshot: `N · D` exact multiply-accumulates (**D**: × the measured **11.5–17.2 ns** per exact MAC) plus hashing `N` leaves (the hash unit cost is not measured here) | one item: `D` MACs + `⌈log2 N⌉` hashes + `k` comparisons (microseconds), filing `item + path + 64 KiB` where `item = 8 + 4D + 4P + 128` and `path = 64 · ⌈log2 N⌉` | the whole snapshot, `N × item` bytes (`retrieval_claim_material_bytes_v1`): **a snapshot nobody else holds makes `P_det = 0`** (ADR-0177 D7) | `N` = 10⁶, `D` = 768, `P` = 64: `item` = 3,464 B, snapshot 3.46 GB, `N·D` = 7.7·10⁸ MACs = **8.8–13.2 s** (D from M unit cost), filing ≈ 70 KB (item + path + 64 KiB header) |
| **Composite** (2–8 stages) | Σ over stages of each stage's material; edges carry no arithmetic | Σ over stages of the component's `T_check` (a model stage: §2.3 per position; a retrieval stage: above); a token edge is recomputed at inclusion (free), a `StageLogits` edge opens one `i32` logits vector | the lying stage's, or one logits tensor (`4·D` bytes + headers) for an edge | every component's artifact and snapshot | no real composite exists; `2 ≤ stages ≤ 8` bounds it to 8× the heaviest stage in the worst case |

What a measurement needs (none exists, so none was attempted): a **real** Memory class (a registered model with a real memory slot and rule program, e.g. through HFX or R4X),
a **real** retrieval snapshot (an embedding model's index of ≥ 10⁵ items), and a **real** composite (a retriever plus a generator from one repository, the case RFC-0004 §II.4 names for the census).
The harness work that would then follow (`opv-meas build-world --kind memory|retrieval|composite`) is not written, because there is nothing to run it on. The kernel's typed-roots
tests (`typed_roots.rs`, the node's `r4x_*`) are correctness tests over those synthetic fixtures; they are not timing evidence and are not used as such.

## 9. Fleet measurement plan (needs the user's approval; nothing below was run)

All items use the same harness (`opv-meas`, `run-class.sh`, `summarize.py`) and write the same JSON. No live node is touched without
approval; reads of the live t12 chain are RPC-read-only.

| Id | Measure | Where / what it needs | Replaces |
|---|---|---|---|
| F1 | DAA tick distribution on t12 over ≥ 24 h (`getBlockDagInfo`, read-only) | one t12 node, approval to read | `s_daa` (120 derived; 125–150 cited) |
| F2 | The full verifier matrix (honest, five lies, withheld/served, HTTP) for the four other real classes, then the 9B-8k pack | hosts with ≥ 24 / 32 / 48 GB (0.8B / 1B and GLM-1.5B / 1.7B) and ≥ 160 GB (9B) RAM with the reference verifier; or a streaming verifier | the four classes not run here; the **A**s of §4.4 and §5 |
| F3a | **Claim-material fetch** (a protocol term): the claim's public material for `m` positions from a provider in the same region, another region and a 100 Mbps / 1 Gbps residential link; 1 and 8 connections; chunk latency, retry rate | two or three hosts, a public-material provider (DA16's transport when merged, else the reference HTTP provider) | `B_eff`, `lambda` of `T_fetch` |
| F3b | **Model acquisition** (an off-chain assumption, ADR-0177 D7, never a window term): how long and what it costs a verifier to acquire a registered model from an open host (bytes, time, retries), and what share of verifiers can at all. Downloads need the user's approval and a disk budget | one or two hosts; a download list approved by the user | `T_acquire`, and the `H` of §3.2 for open models |
| F4 | `n` verifiers at once from one provider: egress saturation, per-verifier throughput | 4–16 hosts or processes | independence of verifiers (an input of `P_det`) |
| F5 | Host classes: Apple M-series (done), x86 8-core cloud VM, 64-core server, an accelerator host; `micro`, `weights`, `verify` on each | three or four hosts | the CPU terms; the ratio between a laptop and a server; production compute cost (item 8 of §7) |
| F6 | Filing inclusion latency and reorg depth: carrier objects of 30 KB and 600 KB, empty and junk-saturated mempool, on the H1 devnet (9 nodes) and, with approval, a t12 observer | the devnet; adversarial junk lane | `carrier_daa`, `reorg_slack_daa` |
| F7 | A symbolised profile build of the reference verifier (`sample`/`perf`) to split the 62 % | one build | §2.3's unattributed share; the size of the speedup §2.5 refers to |
| F8 | Participation and **holding**: how many independent bonded verifiers check, how often a given claim is checked by at least one, and how many of them hold a given registered model. **Not observable on chain** (RFC-0015 §4.3: a clean check leaves no receipt; ADR-0177: the chain tracks no peers), so it needs a voluntary signed "checked scope" attestation off chain, or a pilot with known watchers | a pilot design (a decision, not a measurement) | `q`, `n`, `H`, `P_run` — the inputs the user ruled must be measured |
| F9 | A quiet-host repeat of the Qwen2.5-0.5B matrix and of Qwen3.5-0.8B (16 GB), to remove the ±16 s load noise | one idle Mac or server with ≥ 32 GB free | the ±16 s band |

Stop conditions for any host: free disk < 15 GB, free memory below the sample's need, load > the host's cores × 4 without a quiet window.


## 10. Reading `opv-measurements.json` (for G14C, ECON and K2S)

Every leaf is `{"v", "kind": "measured"|"derived"|"assumed", "src", ...n/min/max}`. Schema `opv-measurements/v2`.

* **`gap07_worst_case_deadlines`** — G14C's GAP-07: `rows` are the per-class, per-claim-size deadlines of §4.3–4.4 (each with its terms, the total in seconds and DAA, the windows it is compared with, and a `status` that says
  MEASURED / ASSUMED), `families` is the §4.2 table (what is unmeasured and what it needs). The top-level **`scopes`** states ADR-0177 D7 (model = off-chain, claim material = protocol), the conditional detection probability and ADR-0032's 49 %.
* **`table`** — the flat view: one row per (class, claim size) with `t_check_cpu_s` / `t_check_wall_s`, the public-material fetch (`da_claim_material_bytes`, `…_fetch_wall_s`), the artifact fetch (read it as `T_acquire`),
  `filing_object_bytes_by_lie`, `t_localize`, `peak_rss_bytes`; a class that did not run carries `t_check_lower_bound_cpu_s` (D) and a labelled-assumed `t_check_estimate_cpu_s`.
* **`classes`** — the nested form per class: `artifact`, `plan_bounds_k2_v2`, `weights_pass_streaming` (M), `producer_side_p3`, `da_p3`, and for the class that ran `t_check_p3_outsider_path`,
  `t_check_by_positions_fresh_path`, `fetch`, `lies`, `t_localize_model`, `demand_flow`. For the classes that did not run: `t_check_lower_bound_cpu_s` (D) and `t_check_estimate_p3_cpu_s` (**A**; do not use it as a result).
* **`not_measured`** — what the table does not contain: the link, chain inclusion, `q` / `n` / `H` / `P_run`, the 9B artifact, any typed-root artifact, the ADR-0176 §4 counters.
* **`opv-measurements-data/derive/`** — the `derive` inputs (`in-v2-*.json`, every input labelled) and the tables of §4.5 and §5, one file per verifier scenario (one present verifier that holds the model; ten at q = 0.5;
  three of ten hold the model); every row also carries the closed-model situation. The files here are `mirror-v2-*.json`, computed by `tools/opv-meas/scripts/derive_mirror.py`, a line-for-line Python mirror of `derive.rs`'s arithmetic, because
  the cargo run of the Rust tool was queued behind the G14 lanes when this note was written (§11). When `opv-meas derive` has run, its `out-v2-*.json` joins them and every shared number must agree.

What K2S needs to compute `q · P_run` that this note does not contain: the number of verifiers that exist, check and **hold the model** (F8), their independence (F4), and the real link (F3a). What ECON needs for the bounty:
per-claim verifier cost (given: CPU-seconds, bytes fetched, RAM) at the claim sizes it prices, the reporter's 49 % as revenue, the acquisition cost as its own off-chain line (F3b), and the implementation the honest verifier
runs (§2.5).

## 11. Reproduce

```text
buildslot.sh cargo build --release --offline -p misaka-palw-opv-meas           # tools/opv-meas
opv-meas static  --container X.palwtir --label L --positions 4,64
opv-meas weights --container X.palwtir --label L --rss-cap-gb 7                # streaming, any host
opv-meas micro   --file X.palwtir
opv-meas build-world --container X.palwtir --out DIR --label L --prompt 3 --max-positions 8 [--claims honest,late]
tools/opv-meas/scripts/run-class.sh L X.palwtir WORK RESULTS.jsonl              # the verifier matrix, one process per sample
opv-meas verify --world DIR/world --claim late --provider DIR/world/provider --cache DIR/cache [--via fresh] [--serve --withhold-pos 0]
python3 -I tools/opv-meas/scripts/summarize.py opv-measurements-data opv-measurements.json
opv-meas derive --in opv-measurements-data/derive/in-v2-one-verifier.json       # the §3 formulas; tests: cargo test -p misaka-palw-opv-meas
python3 -I tools/opv-meas/scripts/derive_mirror.py IN.json OUT.json            # the same arithmetic in Python (used for the tables of this revision)
```

**Status of the Rust tool.** Its revision (`derive.rs`: model off the clock, net-of-49 % reservation, holders, bond-level rows; `verify.rs`'s new `ProsecutionV1::Spec` and `Segmented` arms) is **verified**: it compiled on the merged
PESG integration head (`cargo test --release -p misaka-palw-opv-meas --no-run`, `meas-m4c.log`) and the test binary's 10 tests pass (`derive::tests`, including `the_committed_inputs_reproduce_the_tables_the_note_quotes`). The tables here were produced by the Python mirror;
the test asserts the same key numbers (226 positions, 40,156.86 BILI, closed model = none), but the Rust binary's `out-v2-*.json` were not regenerated (the worktree's target was cleaned) — compare when it next builds. The 2026-10-10 measurements (GLM-Edge-1.5B, P = 32, the P = 64 attempt) ran on the pre-merge harness binary `meas-logs/bin/opv-meas` (commit `76067416e`'s harness over the pre-merge kernel); the
verifier path they time, `OutsiderV1::check`, is the same code the merged head carries, but that is a statement about the source, not a measurement on the merged binary.

Raw results of this note: `docs/design/palw/opv-measurements-data/` (JSON lines; host load, free memory and swap are recorded per sample). The GAP-07 samples taken on the merged integration head are in
`opv-measurements-data/gap07-*.jsonl`.
