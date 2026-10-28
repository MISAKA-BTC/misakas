# PESG §4 D — the real conviction probability, per class and profile (MEAS)

Agent MEAS, branch `meas/opv-timings` (the PESG integration commit `0fc729c55` merged), 2026-10-10. This is lane MEAS's part of
`probabilistic-economic-security-gate.md` §4 D. It **arms nothing and approves nothing**; it uses PESG's three verdict words only
(**PASS / FAIL / UNKNOWN**) and the labels of `opv-measurements.md`: **M** measured on H1's real artifacts (read-only), **D** derived by a stated formula,
**A** assumed (never used as a result). Amounts are BILI. The measurements behind every M are in `opv-measurements.md` and `opv-measurements-data/`;
this note does not repeat them, it applies them to PESG's five factors. Machine-readable: `pesg-d-conviction-probability.json`.

**The result in one paragraph.** No class and no profile has a proven `p_min`. The only conviction probability that is *measured* is a zero: in
K2-TIR v1/v2 a lie placed in the embedding gather is found in 13 s and cannot be filed (the evidence is 272 MB against a 67 MB ceiling), so for the attacker's best
placement `p_evidence = 0` and `p_min = 0` (M, FAIL). Everything else is UNKNOWN, and two of the unknowns are structural and shown as their own rows: **no watcher** (nothing can observe whether
one exists, so `P_run`'s lower bound is 0) and **cannot check in time** (a 0.5B claim of more than about 230 positions misses the 50-DAA window, and its RAM is out of reach of a 32-GiB host well before that). A closed model has `p = 0`.
`p_check` is UNKNOWN because PESG-B has published no bounds (`pesg/detection-bounds` has no commits yet).

## 1. The decomposition (no independence)

```text
p_min  >=  P_run  ·  p_check|run  ·  p_evidence|detect  ·  p_inclusion|evidence  ·  p_collect|convicted          (chain rule: every factor conditional on the ones before it)
```

Each factor is bounded **from below, for the attacker's best strategy**, and the product of the lower bounds is a valid lower bound without any independence assumption. A factor that is UNKNOWN has the proven lower bound **0**,
so `p_min` is UNKNOWN and is **never filled in**. Where a factor has an upper bound the note shows `p_min ≤ …` as an upper bound and says so; an upper bound is not `p_min`. Everything is **conditional on the verifier
holding the registered model** (ADR-0177 D7): the chain does not make a model obtainable, and for a closed model `P_run = 0`, so `p = 0` (its own FAIL row).

| Factor (PESG §4 D) | Meaning | What bounds it from below today | Verdict |
|---|---|---|---|
| `P_run` | a capable, honest verifier actually checks the claim before its deadline | nothing: a clean check leaves no receipt (RFC-0015 §4.3) and the chain tracks no peers (ADR-0177), so the number of capable watchers is **not observable** | **UNKNOWN** (lower bound 0) |
| `p_check` | that verifier finds the fault | PESG-B's bounds: none yet. Declared by the plans, not approved: 240 bits (K2-TIR v1) / 164 (v2) for Qwen2.5-0.5B and Llama-3.2-1B, 239 / 163 GLM-1.5B, 238 / 162 Qwen3.5-0.8B and SmolLM2-1.7B (**D**, `static`); coverage `c = m/P` when a claim is not checked whole | **UNKNOWN** |
| `p_evidence` | the fault is localized and proven from public material alone | **M**: 4 of 5 lie types convicted on the real class; the embedding-gather lie cannot be filed → 0 for the best placement (K2-TIR v1/v2) | **FAIL** (v1/v2, best placement) |
| `p_inclusion` | the evidence is included in the canonical chain inside the deadline | filing is 31 KB (**M**), inside the carrier; carrier wait, junk lanes and reorg depth are unmeasured; lane capture defects F-C4R3-03/-05 are fixed only on G14R's branch (G14C matrix) | **UNKNOWN** |
| `p_collect` | the defined collateral is actually collected | **D** from the ledger: `convict()` slashes `min(reserved, bond collateral)`; reservation is taken from free collateral, so one collateral is never counted twice (G14C V-node N11, N13 pass on the integration head); the reporter's 49 % is already netted out of `L` | UNKNOWN as a gate cell until BUDGET connects the reward paths (PESG §7); the code value is 1 |

## 2. What was measured or derived, factor by factor

### 2.1 `P_run`: capable, and in time — two separate failures, plus the absent watcher

**R1 — no watcher: UNKNOWN, and FAIL if absent.** Whether any bonded verifier checks claims is unobservable on chain, so no measurement can show it is positive. Until a signed off-chain "checked scope" attestation or a
pilot with known watchers exists (fleet plan F8), `P_run`'s lower bound is 0 and `p_min = 0`. No row below assumes a watcher.

**R2 — cannot check in time: measured, per class** (the verifier holds the model; Panel-licensed with every seat colluding and OPV both give a 50-DAA window, interim values; GAP-07 of `opv-measurements.md` §4 gives the full table):

| Class | Largest claim measured | Time axis (worst lie, 50-DAA window) | RAM axis (reference verifier, ≥ 16 B/param + claim) | Verdict of this axis |
|---|---|---|---|---|
| Qwen2.5-0.5B | P = 32: 624 CPU-s to the verdict, 12 DAA (**M**) | fits up to ≈ 226–230 positions (**D**, fit 95 s + 17.5 s/position) | 10.4 GB (P ≤ 16), 13.2 GB (P = 32), > 15.1 GB (P = 64, run stopped at my 14-GiB cap) (**M**); ≈ 30–40 GB at P ≈ 226 (**D**, two points) | P ≤ 32: **PASS** on a ≥ 14 GB host; P = 64: **UNKNOWN** (not run); P ≳ 130 on a 32-GiB host: **FAIL** (RAM, **D**) |
| Qwen3.5-0.8B | none (needs ≥ 12.3 GB) | P = 3: 12 DAA; P = 64: 117 DAA (**A**; 522 MB of claim material per position) | ≥ 12.3 GB (**D**) | **UNKNOWN** (P = 64 would FAIL on time if the carried fit holds) |
| Llama-3.2-1B | none | P = 64: 23 DAA (**A**) | ≥ 19.9 GB | **UNKNOWN** |
| GLM-Edge-1.5B | none | P = 64: 29 DAA (**A**) | ≥ 23.8 GB | **UNKNOWN** |
| SmolLM2-1.7B | none | P = 64: 31 DAA (**A**) | ≥ 27.6 GB | **UNKNOWN** |
| 9B-8k (**A** only; no artifact) | none | the reference verifier reaches 5 of 8,192 positions; a whole claim needs 41,588 DAA (≈ 58 days) | ≥ 144 GB | **FAIL** for any claim larger than ≈ 5 positions at the interim window (**A**-based) |

The same-profile verdicts are not "PASS" for the class: they say only that the time/RAM axis does not by itself exclude a verifier. Capability and time are separate from presence (R1).

**R3 — closed model: FAIL.** A verifier that does not hold the registered model cannot check (ADR-0177 D7). If no third party holds it, `P_run = 0`, `p = 0` and no finite collateral exists. Nothing in the chain or in these measurements says which models are in that state.

### 2.2 `p_check`: UNKNOWN until PESG-B

`pesg/detection-bounds` carries no commit, so no bound is taken. What is stated, so that PESG-B can combine it: a verifier that checks a claim whole has coverage 1 and the plan's declared error (§1); a verifier that checks `m` of `P` positions has `p_check ≤ m/P`
for a one-position lie (**D**; 5/8,192 = 0.061 % for the 9B-8k reference verifier, **A**). Adaptive placement, beacon grinding and correlated seeds are PESG-B's, not measured here.

### 2.3 `p_evidence`: measured on the real class

Qwen2.5-0.5B, 3 positions, outsider path (one fresh process per sample), the five lie types:

| Lie | n | found in (CPU s) | evidence | accepted | `p_evidence` |
|---|---|---|---|---|---|
| matmul, position 0 (`early`) | 2 | 12.4 | 30,468 B | strict decode, court convicts | 1 (M, n = 2) |
| matmul, last position (`late`) | 2 + 1 at P = 32 | 163.9 (P = 3), 624.2 (P = 32) | 31,492 B at both sizes | convicts | 1 (M) |
| elementwise clamp, last position (`elem`) | 1 | 124.1 | 227 B | convicts | 1 (M, n = 1) |
| wrong delivered token (`decode`) | 2 | 3.0 | 607,896 B | convicts | 1 (M, n = 2) |
| **embedding gather** (`gather`) | 2 | 13.0 | **272,271,336 B** | **refused: past the 67,108,864-byte wire ceiling** (interim carrier 1,583,616 B) | **0 (M)** |

The attacker chooses the placement, so the bound that counts is the minimum: **`p_evidence = 0` for K2-TIR v1/v2 on this class, FAIL (M).** It is structural for the other four classes too: each class's gate bound on a filing (0.54–2.15 GB, **D** from `static`) is larger than the 67 MB wire ceiling.
The remedy PESG's counter-example A anticipates (an element court with a small filing) is K2S's v4 route; its lie types were verified on the integration head by K2S's own node tests, not measured here. A withheld position is a default, not a conviction, and its disclosure (59 MB) does not fit the interim carrier either (**M**).

### 2.4 `p_inclusion`: UNKNOWN

The evidence that does fit is 31 KB, an object far under the carrier cap, so size is not the obstacle. The wait is: carrier inclusion and reorg depth are the policy's 2 + 2 DAA, **not measured** (F6), with junk-lane saturation, DA congestion (§3) and the open lane-capture defects unmeasured. No deadline-meeting probability can be stated.

### 2.5 `p_collect`: derived

`convict()` slashes `min(reserved, producer collateral)`; the reservation comes from free collateral, so concurrent claims do not share one collateral (D); exit while liable and replay/reorg restore are covered by V-node tests N11 and N13 of the integration head (cited, not re-run). The reporter's share is 4,900 bps (ADR-0032), so the loss that counts is
`L = 0.51 · S_collected` (ADR-0176 D6); the interim ledger's 500 permille is shown only as a comparator in `opv-measurements.md`. Value 1 under those invariants; a gate cell only when BUDGET's reward paths are connected.

## 3. The record PESG §4 D asks for

| Item | Value | Label |
|---|---|---|
| Hardware | Apple M1 Max, 10 cores, 32 GiB, macOS 26.7; **shared**: 9 devnet `kaspad` + other lanes' builds (load 13–220 across samples). The reference verifier is single-threaded, `i128`, no streaming | M |
| Claim rate | **not observed on any network.** From code (`palw_issuance_slots_v1`, `t12-bond-reuse-audit-2026-10-10.md`): a 13,000-BILI bond gets 1 claim/DAA at `rho = 10` and 100 claims/DAA at `rho = 1,000` | D (code reading) |
| Cold model fetch | 1.00 s CPU / 1.50 s wall for 0.66 GB from a local file; 0.40 + 1.57 CPU for material + artifact over localhost HTTP (provider CPU 0.7 s). **Off-chain assumption (ADR-0177 D7), never a window term.** Over a real link: unmeasured; 94 s at 1 Gbps / 940 s at 100 Mbps for an 11.7 GB 9B | M (local); A (link) |
| p95 / p99 | **not estimable**: n = 5 honest samples at P = 3, 2 lies, 1 at P = 32. The honest figure is the observed maximum: check 154.1 CPU-s (median 140.4, spread ±16 s), wall 291.6 s (median 189.6) under load 37–220; P = 32 lie: 624.2 CPU-s, wall 750.7 s. p95/p99: UNKNOWN | M (max), UNKNOWN (percentiles) |
| Maximum claim count | interim policy: 3 live per producer, 32 in all (**D**, `interim_v1`); t12 issuance: 2,000 slots for a 13,000-BILI bond at `rho = 1,000` (**D**, code reading) | D |
| DA congestion | **not measured.** Derived demand on a verifier that checks every claim: `claims/DAA × P × material per position ÷ 120 s`. At 60 MB per position (0.5B, M) and P = 3: 1.5 MB/s at 1 claim/DAA, **149 MB/s (1.19 Gbps) at 100 claims/DAA** — for one bond; Qwen3.5-0.8B's 522 MB per position is 8.7× that. CPU for the same coverage: 1.2 cores at 1 claim/DAA, **117 cores at 100** (0.5B, P = 3) | D |
| Evidence bytes | 227 B (elem) … 31,492 B (matmul rows) … 607,896 B (decode) fit; 272 MB (gather) and the 59 MB disclosure do not | M |
| Carrier wait | **unmeasured** (policy 2 DAA + 2 DAA reorg); fleet plan F6 | UNKNOWN |
| DAA vs wall-clock gap | `s_daa ≥ 120 s` (RFC-0012 policy, a lower bound used for every conversion); 125–150 s is cited from drills, not measured here (F1). A 50-DAA window is 6,000–7,500 s. Check wall time under load is 1.1–2.0× CPU (median 1.35×) | D (120), A (125–150), M (1.35×) |

## 4. Rows that must stay separate (and `p_min`)

| Row | Condition | `p_min` | Verdict |
|---|---|---|---|
| N1 no watcher | no capable honest verifier checks the claim | 0 (lower bound) | **UNKNOWN**; FAIL if absent |
| N2 cannot check in time | claim larger than the time or RAM axis of §2.1 | 0 for that claim | **FAIL** where measured or derived (0.5B beyond ≈ 130 positions on a 32-GiB host; 9B-8k), UNKNOWN for the classes that did not run |
| N3 closed model | nobody but the producer holds the model | 0 (`p = 0`) | **FAIL** |
| N4 best-placed lie, K2-TIR v1/v2 | lie in the embedding gather | 0 (`p_evidence = 0`, M) | **FAIL** |
| N5 check probability | all other placements | UNKNOWN (PESG-B) | **UNKNOWN** |
| N6 inclusion in time | carrier wait, junk lanes, reorg | UNKNOWN | **UNKNOWN** |
| N7 collection | reservation, exit, reorg | 1 under the ledger's invariants (D) | UNKNOWN as a gate cell |

**`p_min` is UNKNOWN for every class and profile** (open or closed, Panel-licensed with every seat colluding, OPV, Panel=0). The profiles differ in the window rule and registration, not in this table: at the interim carriers a real class registers only Panel-licensed (`carrier_fit_v1` refuses OPV for all five), the 50-DAA window is the same
under both, and Panel=0 has no seat collateral to count (PESG §3). The legacy V2 route is not G14 and is not scored. What the table does fix is the **required** value: with `L` netted of the 49 %, `E[Π] < 0` needs `p > G / (G + L)` (PESG §1, `C = 0`):

| Reservation `S` | `L = 0.51 S` | required `p` for `G` = 20 BILI (interim) | what the chain above proves |
|---|---|---|---|
| 1,000 (interim) | 510 | > 3.8 % | 0 (N4) |
| 40,157 (9B-8k deterrence-only, A) | 20,480 | > 0.098 % | 0 (N2: 5 of 8,192 positions) |
| any, closed model | — | no finite `S` | 0 (N3) |

## 5. Earlier work, folded in

* **GAP-07 (G14C), worst-case deadlines:** `opv-measurements.md` §4 and `opv-measurements.json` → `gap07_worst_case_deadlines` are the data of R2 and §3 above; the matrix cell is closed only for Qwen2.5-0.5B up to P = 32. Updated here: the P = 64 attempt (stopped by RAM) and the RAM growth finding F-MEAS-10.
* **ADR-0176 §4 plan:** `opv-measurements.md` §7 (claims, reward blocks, rewards and Final credit per DAA per BILI of bond; liability and exposure; inspection/court resources; effective detection and collection rate; compute cost). In PESG's terms it feeds §4 A's `Exposure` and §6's `F_max`, and its item 7 is this note's `P_run` and `p_collect`. It needs BUDGET's engine for items 1–5 and 7; nothing ran.
* **RFC-0004 per-kind inspection cost:** `opv-measurements.md` §8: no real Memory / Retrieval / Composite artifact exists, so for those kinds every factor above is UNKNOWN and no `T_check` is claimed.

## 6. Plan to replace UNKNOWN with numbers (needs the user's approval; nothing below ran)

| Id | Measure | Factor | Needs |
|---|---|---|---|
| P1 | an off-chain "checked scope" attestation, or a pilot with known watchers, against injected known-bad claims: how often a claim is checked before its deadline, by how many verifiers, holding which models | `P_run` | a pilot design and hosts |
| P2 | the reference-verifier matrix for the four other classes on hosts with ≥ 24 / 32 / 48 GB, and the 9B-8k pack on ≥ 160 GB; a quiet-host repeat of the 0.5B matrix for p95/p99 (n ≥ 30) | R2, percentiles | hosts |
| P3 | PESG-B's bounds, combined with this note's conditional factors | `p_check` | PESG-B |
| P4 | the K2S v4 element-court filing sizes for the gather-type lie on a real class | `p_evidence` | H1's 9B pack or a real v4 class |
| P5 | carrier wait, reorg depth and junk-lane saturation on the H1 devnet, then (with approval) a t12 observer; DA congestion at 1, 10, 100 claims/DAA | `p_inclusion`, DA | the devnet; approval for t12 |
| P6 | the DAA tick distribution on t12 over ≥ 24 h (read-only) | DAA vs wall | approval to read |
| P7 | model acquisition from open hosts (bytes, time) and the share of verifiers that can acquire at all | `P_run`, `H` | downloads need approval |
| P8 | BUDGET's counters (`Q/B/R/F` used, reserved collateral, collected / nominal slash) over the run matrix of `opv-measurements.md` §7 | `p_collect`, exposure | BUDGET's reward-path connection |
