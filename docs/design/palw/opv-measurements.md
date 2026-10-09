# OPV fresh-verifier measurements, and the window and collateral formulas they feed (MEAS)

Agent MEAS, branch `meas/opv-timings` (base `b676927de`), 2026-10-09. Harness: `tools/opv-meas` (a test-only binary; no wire id, fence,
parameter or fingerprint of the protocol changes). Amounts are BILI (ADR-0174). Machine-readable table: `opv-measurements.json` (next to
this file; raw results in `opv-measurements-data/`).

**Status.** A measurement note and a plan. It chooses no parameter, arms no fence and approves nothing: every number below is an *input*
to the user's decision on the OPV terms (`activation-readiness-matrix.md` §3a, order step 3). Each number carries one label, here and in
the JSON:

* **M** — measured: read off a harness run on this Mac, on a real artifact (the run count and spread are given);
* **D** — derived from M by a stated formula (the formulas are executable: `opv-meas derive`, 4 unit tests, the soundness dossier's
  row K reproduced);
* **A** — assumed: no run established it. An A is never used as a result; it is named so that a measurement can replace it.

The user's 2026-10-09 ruling applies: the probability that a fraud is actually checked (`q`, `P_run`) and the verifier's cost recovery are
priced from measurements, never from an assumed independence or an assumed rate. This note therefore measures what ONE verifier costs and
covers, states the formulas, and leaves `q`, the number of verifiers and their independence as **unmeasured** (§7).

Premise conditions addressed (`docs/PRINCIPLES.md` §6): **5** (collateral against the maximum gain and a *real* detection probability),
**6** (an honest verifier can take part in time, with the resources it has), **7** (the dispute / Final / reorg clock).

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

**Artifacts** (`wh-h1-run/cache/*/class.palwtir.testnet-12.palwtir`, H1's, read-only; byte-identical copies were measured and the three
larger ones deleted afterwards): Qwen2.5-0.5B, Qwen3.5-0.8B, Llama-3.2-1B, SmolLM2-1.7B.

**Host.** Apple M1 Max, 10 cores, 32 GiB, macOS 26.7 — **shared**: nine devnet `kaspad`, other lanes' builds and tests ran throughout.
During the verify samples the 1-minute load was 37–220 (median 57) and swap 15–21 GB. Consequences, stated once:

* CPU time is the comparable figure; wall time under that load is 1.1–2.0× CPU (median 1.35×; M);
* CPU time is not load-immune either (efficiency cores, memory compression): repeated 140 s checks spread 138–154 s, and the linear fit of
  §2.4 leaves a 16 s residual. Treat every check time as ±16 s;
* `peak RSS` is macOS `ru_maxrss`, which does not count compressed or swapped pages: the *maximum* over samples is the honest figure.

**Limits of what was run (not hidden).**

1. **One class end to end.** The reference verifier caches every parameter instance as `i128` for the scope
   (`Ctx::param_cache`, `misaka-palw-kernel/src/verify.rs`): ≥ 16 bytes per parameter. Qwen2.5-0.5B needs ≈ 8 GB and ran (peak RSS up to
   10.4 GB, 20.8 B/param, M). Qwen3.5-0.8B needs ≥ 12.3 GB, Llama-3.2-1B ≥ 19.9 GB, SmolLM2-1.7B ≥ 27.6 GB: not runnable on this shared
   32 GiB host. For those, only the parts that need no whole-artifact cache were measured: the streaming per-parameter pass (`opv-meas
   weights`), the producer side and the claim's public-material size.
2. **No network.** Every fetch is a local file or the reference HTTP provider on `127.0.0.1`. Link bandwidth, latency, loss and provider
   egress are unmeasured (§6, F3–F4).
3. **No chain.** Carrier inclusion and reorg depth are the policy's `carrier_daa` / `reorg_slack_daa` (2 / 2 interim), not measured (§6, F6).
4. **The reference implementation.** Single thread, `i128` tensors, no streaming. It is the code the kernel ships, not a ceiling on what an
   optimized verifier could do (see §2.5).
5. **Claims are small** (3, 8 and 16 positions of one class). Anything at 8k positions is an extrapolation and is labelled.

## 2. Results

### 2.1 Unit costs on this host (M, `opv-meas micro`, medians of 5)

| Operation | CPU |
|---|---|
| `GF(2^127 − 1)` multiply-accumulate (Freivalds inner loop) | 8.9 ns |
| exact checked `i128` multiply-accumulate (a localized row's recompute) | 11.5–17.2 ns (three runs, host state differed) |
| `tensor_commitment` (dual-root Merkle) of an `i8` tensor | 24.2 ns per element |
| container tensor → `i128` | 3.8 ns per element |
| fetch + hash-check of public material / artifact from a local file | ≈ 690 MB per CPU-second (0.26 s per 179 MB; 1.00 s per 661 MB) |

### 2.2 The four real classes (M unless marked)

| | Qwen2.5-0.5B | Qwen3.5-0.8B | Llama-3.2-1B | SmolLM2-1.7B |
|---|---|---|---|---|
| params | 502.4 M | 765.9 M | 1,245.0 M | 1,725.9 M |
| artifact (container) bytes | 660.7 MB | 1,052.5 MB | 1,533.1 MB | 1,866.7 MB |
| weight pass, streaming: read / commit×1 / projection ×2 reps, CPU s | 1.5 / 13.1 / 13.5 | 2.4 / 23.3 / 21.5 | 3.8 / 42.2 / 36.9 | 4.6 / 49.1 / 49.6 |
| weight pass model, commit ×2 as the verifier does (D) | 41.2 s (82 ns/param) | 70.5 s (92) | 125.1 s (101) | 152.2 s (88) |
| public material per position, P=3 | 59.6 MB | **522.1 MB** | 70.5 MB | 106.4 MB |
| producer: reference evaluation of 3 tokens, CPU s | 24.9 | 57.1 | 71.4 | 78.8 |
| plan bound `max_verifier_ram` (gate) | 9.51 GB | 2.39 GB | 14.62 GB | 10.85 GB |
| lower bound on the reference verifier's RAM, 16 B × params (D) | 8.0 GB | 12.3 GB | 19.9 GB | 27.6 GB |
| gate `max_filing_bytes` (D, plan) | 0.54 GB | 1.07 GB | 1.07 GB | 2.15 GB |
| `carrier_fit_v1`'s worst filing/response (D, plan) | 9.22 GB | 1.83 GB | 14.03 GB | 10.58 GB |
| OPV registration at the interim carriers (1,583,616 B) | **refused** | **refused** | **refused** | **refused** |
| verifier verified end to end here | yes | no (RAM) | no (RAM) | no (RAM) |

The plan-bound rows are the K2-TIR-v1/v2 gate at 4 positions; "refused" is `carrier_fit_v1` ("a worst-case filing of N bytes does not fit
a FileProof"), so the measurements below ran on the same class under the Panel-licensed registration — the check and court path is
identical; only the registration mode differs.

### 2.3 Qwen2.5-0.5B, 3 positions, end to end (M; n = samples)

| Step | CPU s | wall s (load 37–220) | bytes |
|---|---|---|---|
| read, strict-decode and replay the chain (7.7 MB: registration + 6 claims) | 0.03 | 0.04 | 7,707,966 |
| fetch the claim's public material — files (n=16) | 0.26 | 0.35 | 178,650,993 |
| fetch the artifact — files (n=16) | 1.00 | 1.50 | 660,666,048 |
| fetch both — reference HTTP provider on localhost (n=2); provider process CPU 0.7 s each | 0.40 + 1.57 | 1.45 + 6.08 | same |
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
(7 %). That has one consequence for every window number in §4: **they price this implementation**. A verifier that streams weights, hashes
once and runs on all cores would change T_check by an order of magnitude or more, and the soundness-relevant question — which verifier
implementation the honest-verifier assumption (RFC-0015 §2) refers to — has no answer in the repository. The policy cannot be derived
before that implementation is named and measured; §4 therefore shows the reference verifier and one sensitivity row, and nothing else.

### 2.6 Findings

| Id | Finding | Premise condition |
|---|---|---|
| **F-MEAS-01** | **No real class is OPV-registrable at the interim carriers under K2-TIR-v1/v2** (all four: `carrier_fit_v1`'s worst-case filing/response 1.8–14.0 GB against 1,583,616 B). OPV timings for a real class presuppose the K2-TIR-v4 route (filing 170 KB at 9B-8k per K2S), which this base does not have. | 4, 6 |
| **F-MEAS-02** | **A lie in the embedding `Gather` is found in 13 s and cannot be filed**: its filing is 272 MB and the strict wire decode refuses it (ceiling 67 MB; interim 1.58 MB). Likewise a demanded position's disclosure is 59 MB. In K2-TIR-v1/v2 the objective court is complete only for faults whose filing fits (this is the K2S v4 element-court motivation, now measured on a real artifact). | 4 (§3 "bounded localization and adjudication") |
| **F-MEAS-03** | **The gate's `max_verifier_ram` is not a bound on the reference verifier's memory.** It is `artifact_bytes + evidence_bytes_per_position` of the plan; the verifier caches 16 B/param. Measured peak 10.44 GB (fresh path 10.47 GB) against a 9.51 GB bound at 0.5B; for 0.8B the bound is 2.39 GB against ≥ 12.3 GB. At 9B the reference verifier needs ≥ 144 GB. | 6 |
| **F-MEAS-04** | Check cost is 62 % non-algebraic (§2.3) and **the per-position public material varies 8.8× across classes of similar size** (522 MB for Qwen3.5-0.8B against 60–106 MB for the others): T_check, DA and the fetch bill are properties of the program, not of its parameter count. | 6 |
| **F-MEAS-05** | A verifier authenticates each weight and each served value **twice** (`StageMaterial::param` / `OutsiderV1::missing`, then `Ctx::authentic`): ≈ 13 s (weights, 0.5B) + ≈ 9 s (values: the outsider-minus-fresh difference) of avoidable CPU in the reference path. Not a soundness issue. | 6 |
| F-MEAS-06 | Respond/disclosure and filing sizes (59 MB, 272 MB) mean the second branch of `OpvPolicyV1::validate` (`disclose_daa`, `localize + court + carrier + reorg ≤ proof_grace`) is not exercised by any real class today: there is no carriable disclosure to time. | 7 |

## 3. Formulas (executable: `opv-meas derive --in inputs.json`)

### 3.1 The clock

```text
T_challenge(m) >= T_beacon + T_fetch(m) + T_check(m) + T_localize + T_file + T_margin          (the user's 2026-10-08 ruling)

T_fetch(m)  = lambda + (A + m * M_pos) / B_eff          A artifact bytes, M_pos public material per position, m positions checked,
                                                         B_eff = min(link, provider egress, ~690 MB/s hash-check ceiling per core (M))
T_check(m)  = (W + m * rho * M_pos) / speedup           W = fixed weight pass (95.4 s at 0.5B, D), rho = 0.29 us/byte (D), speedup = 1 (reference)
T_localize  = k * n_row * c_mac  (a matmul row) | ~0 (an element court)                          0.5B lm_head row: 1.6-2.3 s (D)
T_file      = assembly + encode + strict decode + court + carrier inclusion                       assembly/court: ms (M); carrier: policy, unmeasured
T_margin    = reorg_slack + margin_frac * (the sum)                                               margin_frac = 0.35 (median wall/CPU, M)
DAA budget  = ceil(seconds / s_daa),  s_daa >= 120 (a DAA step needs a 120 s slot, rfc-0012-policy-proposal.md; 125-150 cited, not measured here)
```

`T_beacon` is **0** for the outsider's private-salt check (the only soundness-bearing path today). A public-coin per-claim check (SG-05)
would add the beacon's lock latency after the commit; with the interim onboarding shape (anchor delay 2 DAA + beacon window 120 DAA) that
is ≥ 122 DAA, 2.4× the interim 50-DAA window — a public-coin post-commit challenge cannot ride a beacon of that shape inside an OPV window
(an input to OPV-BOOT, not decided here).

Mapping to the policy's budgets (`opv.rs`): `T_fetch → cold_material_daa`, `T_check → check_daa`, `T_localize → localize_daa`,
`T_file → court_daa + carrier_daa`, `T_margin → reorg_slack_daa`. `derive` builds the budgets from the measured terms and runs the
kernel's own `OpvPolicyV1::validate` on them, so a derived window is judged by the relations the node enforces.

### 3.2 Detection and collateral

```text
c      = m / P                                (coverage of ONE verifier whose positions are drawn uniformly and unpredictably)
P_det  = 1 - prod_i (1 - q_i c_i)             over the verifiers i that exist; q_i = P(verifier i is present and checks)       [q_i, i: NOT MEASURED]
P_dc   = P_det * (1 - eps_enf)                detected AND convicted                                                           [eps_enf: NOT MEASURED]
R      >= ceil( max(gain + default_penalty, ceil(gain / P_dc)) / (1 - accuser) )      none when P_dc = 0 ("no finite collateral")
G_def   = R * (1 - accuser) * P_dc            the gain a reservation R defends (the inverse)
exposure: producer needs live_claims_per_producer * R free collateral; the ledger locks live_claims_total * R at most
```

At the interim terms: `gain` = reward 5 + work credit 5 + external bound 10 = **20 BILI**, `default_penalty` 100, `accuser` 500 ‰,
reservation **1,000 BILI**, 3 live claims per producer, 32 in total. `P_det` with independent verifiers is `1 − (1 − qc)^n`; the
independence is an **assumption** (verifiers that fetch from the same provider, run the same implementation and sample with the same
habits are correlated) and nothing in this note establishes it.

## 4. Applying the formulas

### 4.1 The interim budgets against what was measured (Qwen2.5-0.5B, P = 3)

D, from §2.3 at 120 s/DAA: cold fetch 2–8 s → **1 DAA**; check 140 s CPU / 190 s wall → **2 DAA**; localization ≤ 24 s → 1 DAA; filing and
court ≈ 0 → 1 DAA; with the policy's carrier 2 and reorg 2: **a first step of ≈ 7 DAA (1 + 2 + 2 + 2) against a 50-DAA window** (interim budgets: cold 10, check 10,
localize 2, court 3). At 3 positions the interim terms are far looser than the measurement. They say nothing about a claim of thousands
of positions, which is the case that matters (§4.2).

### 4.2 Coverage inside the interim window — what ONE reference verifier reaches (D)

Window = 50 DAA = 6,000 s, less carrier and reorg (480 s), `margin_frac` 0.35, fixed 95.4 s + 17.5 s per position, 59.8 MB per position:

| Claim | positions one verifier can check | coverage `c` | whole claim needs |
|---|---|---|---|
| 0.5B, P = 16 | 16 | 100 % | 9 DAA |
| 0.5B, P = 64 | 64 | 100 % | 18 DAA |
| 0.5B, P = 256 | 226 | 88 % | 56 DAA |
| 0.5B, P = 1,024 (extrapolation) | 226 | 22 % | 207 DAA |
| 0.5B, P = 8,192 (extrapolation) | 226 | 2.8 % | 1,619 DAA (≈ 54 h) |

(A 1 Gbps link, **A**, moves the last row to 220 positions: the CPU, not the link, binds. Public material at 8k is 490 GB if the
per-position size stays 59.8 MB, which it will not once history grows — a lower bound.)

### 4.3 Collateral against `P_dc` (D; the formula only — it needs `P_dc`, which is not measured)

`R = ⌈ max(120, gain/P_dc) ⌉ / 0.5` BILI at gain 20, penalty 100, accuser 500 ‰:

| `P_dc` | 1 | 1/2 | 1/10 | 1/100 | 1/1,000 | 8/8,192 | 1/10,000 |
|---|---|---|---|---|---|---|---|
| reservation `R` (BILI) | 240 | 240 | 400 | 4,000 | 40,000 | 40,960 | 400,000 |
| × the interim 1,000 | 0.24 | 0.24 | 0.4 | 4 | 40 | 41 | 400 |
| gain the interim 1,000 defends (BILI) | 500 | 250 | 50 | 5 | 0.5 | 0.49 | 0.05 |

With one verifier that is certainly present, `P_dc = c`: Qwen2.5-0.5B at 8k positions gives `c = 2.8 %` and `R ≈ 1,450 BILI` (1.45× the
interim). **That is the best case for one reference verifier, not a derived collateral.** More verifiers lower it by an amount that is
unmeasured; fewer, or none, raise it without bound (`P_dc = 0` ⇒ no finite collateral).

### 4.4 The 9B-8k case (SOUND SG-06) — **assumed rates, shown for scale only**

Inputs: P = 8,192, public material 1.54 GB per position (K2S / `coverage-p1p2-record.md`, shape-level probe: 12.6 TB per claim),
artifact ≈ 11.7 GB (**A**: 1.3 B/param × 9.0 B; the four real containers are 1.08–1.37 B/param, M), check fixed part 190 ns/param × 9 B =
1,710 s (**A**: the 0.5B ratio carried to 9B) and 0.29 µs/byte × 1.54 GB = 451 s per position (**A**: the 0.5B rate carried to another
architecture). None of these was measured on the 9B (H1's pack is not built); they exist to show which direction and how far.

| Verifier | positions in the interim window | coverage | whole claim | `R` at `P_dc = c` |
|---|---|---|---|---|
| reference, single core | 5 | 0.06 % | 41,588 DAA (≈ 58 days) | ≈ 65,500 BILI (65×) |
| 10× faster (**A**, sensitivity only) | 86 | 1.0 % | 4,163 DAA (≈ 5.8 days) | ≈ 3,800 BILI (3.8×) |

Reading: with the reference verifier the interim window reaches 5 of 8,192 positions (the same order as SOUND's "8 of 8,192" row; a
different route, with assumed rates), and complete coverage inside any window of the interim size is out of reach by three orders of magnitude; reading the 12.6 TB
alone takes 28 h at 1 Gbps and 2.8 h at 10 Gbps (D, no CPU at all), i.e. ≥ 84 DAA even with an infinitely fast CPU.
**Condition 5 cannot be met by collateral at this scale unless `gain` per claim is capped to what `R · (1 − a) · P_dc` defends
(0.49 BILI for the interim 1,000 at 8/8,192), or the window is extended by orders of magnitude, or detection is made cheaper than
re-reading the claim** (K2S's sublinear-read question; SG-06 stays open).

## 5. What the numbers say about the premise's conditions

* **§6.5 — collateral matched to maximum gain and a real detection probability.** Not derivable yet: the formula needs `P_dc`, and the only
  measured factor is one reference verifier's coverage (§4.2). For small claims (P ≲ 64 at 0.5B) one verifier covers everything inside the
  interim window; for 8k-position claims it covers 3 % (0.5B) or 0.06 % (9B, A). The interim 1,000 BILI defends a 20-BILI gain only when
  `P_dc ≥ 1/25` (by the G14-R4 form with the accuser share: `P_dc ≥ gain/(R(1 − a)) = 4 %`).
* **§6.6 — an honest verifier can take part in time, with its resources.** The reference verifier needs ≥ 16 B/param of RAM (≥ 144 GB at
  9B), 95 s + 17.5 s per position of CPU at 0.5B, and an implementation that does not exist yet (§2.5). Cost recovery per claim, from
  measurements: **140 CPU-seconds and 0.84 GB of opened data for a 3-position 0.5B claim; ≈ 370 CPU-seconds for 16 positions** (M, ±16 s);
  ECON prices the bounty from `opv-measurements.json`.
* **§6.7 — the dispute clock.** For P ≤ 64 at 0.5B the interim 10-DAA check budget (1,200 s) is enough; beyond that it is not, and the
  disclosure branch (`disclose_daa`, `proof_grace`) cannot be exercised by any real class because the disclosure does not fit a carrier
  (F-MEAS-02, -06).

## 6. Fleet measurement plan (needs the user's approval; nothing below was run)

All items use the same harness (`opv-meas`, `run-class.sh`, `summarize.py`) and write the same JSON. No live node is touched without
approval; reads of the live t12 chain are RPC-read-only.

| Id | Measure | Where / what it needs | Replaces |
|---|---|---|---|
| F1 | DAA tick distribution on t12 over ≥ 24 h (`getBlockDagInfo`, read-only) | one t12 node, approval to read | `s_daa` (120 derived; 125–150 cited) |
| F2 | The full verifier matrix (honest, five lies, withheld/served, HTTP) for all four classes, then the 9B-8k pack | hosts with ≥ 24 / 32 / 48 GB (0.8B / 1B / 1.7B) and ≥ 160 GB (9B) RAM with the reference verifier; or a streaming verifier | the three classes not run here; the **A**s of §4.4 |
| F3 | Network: cold fetch of the artifact and of `m` positions from a provider in the same region, another region and a 100 Mbps / 1 Gbps residential link; 1 and 8 connections; chunk latency, retry rate | two or three hosts, a public-material provider (DA16's transport when merged, else the reference HTTP provider) | `B_eff`, `lambda` |
| F4 | n verifiers at once from one provider: egress saturation, per-verifier throughput | 4–16 hosts or processes | independence of verifiers (an input of `P_det`) |
| F5 | Host classes: Apple M-series (done), x86 8-core cloud VM, 64-core server; `micro`, `weights`, `verify` on each | three hosts | the CPU terms; the ratio between a laptop and a server |
| F6 | Filing inclusion latency and reorg depth: carrier objects of 30 KB and 600 KB, empty and junk-saturated mempool, on the H1 devnet (9 nodes) and, with approval, a t12 observer | the devnet; adversarial junk lane | `carrier_daa`, `reorg_slack_daa` |
| F7 | A symbolised profile build of the reference verifier (`sample`/`perf`) to split the 62 % | one build | §2.3's unattributed share; the size of the speedup §2.5 refers to |
| F8 | Participation: how many independent bonded verifiers check, how often a given claim is checked by at least one. **Not observable on chain** (RFC-0015 §4.3: a clean check leaves no receipt), so it needs a voluntary signed "checked scope" attestation off chain, or a pilot with known watchers | a pilot design (a decision, not a measurement) | `q`, `n`, `P_run` — the inputs the user ruled must be measured |

Stop conditions for any host: free disk < 15 GB, free memory below the sample's need, load > the host's cores × 4 without a quiet window.

## 7. Reading `opv-measurements.json` (for ECON and K2S)

Every leaf is `{"v", "kind": "measured"|"derived"|"assumed", "src", ...n/min/max}`. **`table`** is the flat view: one row per
(class, claim size) with `t_check_cpu_s` / `t_check_wall_s`, the public-material fetch (`da_claim_material_bytes`, `…_fetch_wall_s`), the
artifact fetch, `filing_object_bytes_by_lie`, `t_localize`, `peak_rss_bytes`; a class that did not run carries `t_check_lower_bound_cpu_s`
(D) and a labelled-assumed `t_check_estimate_cpu_s`, and CPU-floor fetch times (D). The nested form, per class: `artifact`, `plan_bounds_k2_v2`,
`weights_pass_streaming` (M), `producer_side_p3`, `da_p3` (public-material bytes), and — for the class that ran — `t_check_p3_outsider_path`,
`t_check_by_positions_fresh_path` (samples, counters, fits), `fetch` (files and localhost HTTP; bytes, wall, CPU), `lies` (T_check to
verdict, filing bytes, wire verdict, court), `t_localize_model`, `demand_flow`. For the three classes that did not run:
`t_check_lower_bound_cpu_s` (D: the measured weight pass, a floor) and `t_check_estimate_p3_cpu_s` (**A**: carries the 0.5B ratios over; do
not use it as a result). `not_measured` lists what the table does not contain — in particular the link, chain inclusion, and
`q` / `n` / `P_run`.

What K2S needs to compute `q · P_run` that this note does not contain: the number of verifiers that exist and check (F8), their
independence (F4), and the real link (F3). What ECON needs for the bounty: per-claim verifier cost (given: CPU-seconds, bytes fetched,
RAM) at the claim sizes it prices, and the implementation the honest verifier runs (§2.5).

## 8. Reproduce

```text
buildslot.sh cargo build --release --offline -p misaka-palw-opv-meas           # tools/opv-meas (commit 76067416e)
opv-meas static  --container X.palwtir --label L --positions 4,64
opv-meas weights --container X.palwtir --label L --rss-cap-gb 7                # streaming, any host
opv-meas micro   --file X.palwtir
opv-meas build-world --container X.palwtir --out DIR --label L --prompt 3 --max-positions 8 [--claims honest]
tools/opv-meas/scripts/run-class.sh L X.palwtir WORK RESULTS.jsonl              # the verifier matrix, one process per sample
opv-meas verify --world DIR/world --claim late --provider DIR/world/provider --cache DIR/cache [--via fresh] [--serve --withhold-pos 0]
python3 -I tools/opv-meas/scripts/summarize.py RESULTS_DIR opv-measurements.json
opv-meas derive --in inputs.json                                               # the §3 formulas; tests: cargo test -p misaka-palw-opv-meas
```

Raw results of this note: `docs/design/palw/opv-measurements-data/` (JSON lines; host load, free memory and swap are recorded per
sample).
