# 10 — Attack model

*Normative. This chapter says who the adversary is, what it can do, and which attacks misaka-next must
defeat. It covers every consensus-level attack found against Misaka PoL up to t12. Each attack name
is also the name of a regression test, and misaka-next's adversarial simulator must reproduce each
one before any node, network or storage code is written ([README](../../README.md)). The historical
record behind the entries, with sources, is [appendix A](appendix-a-attack-catalog.md). The
invariants cited here are in [09-invariants.md](09-invariants.md).*

## 1. Purpose

An invariant says what must stay true. An attack says how someone tries to make it false. This
chapter fixes three things so the simulator can be built against them. §2 fixes the adversary: its
capabilities, what the honest side is assumed to have, and what is out of scope. §3 fixes the
vocabulary a regression test is written in. §4 onward fixes each attack's verdict on t12 and its
defence in misaka-next. When the adversary model changes, every verdict here must be re-derived.

## 2. The adversary

### 2.1 Capabilities

The adversary is one coordinated party. It controls any number of accounts and keys, because
identities are free (02 §2.1). Its strength is given by the parameters below, which are also the
simulator's inputs.

| # | capability | parameter | what it buys | what bounds it in misaka-next |
| --- | --- | --- | --- | --- |
| C1 | **heartbeat hash power** | `h`, hashes per second | Heartbeat blocks, each worth `2^24` hashes on t12, on any branch. It also buys grinding of any hash nobody pins. | A heartbeat claims at most one slot per `SLOT_MS` (DAA-R1, DAA-R2) and carries only `LocalDaa` authority (DAA-R9). |
| C2 | **bonded stake fraction** | `s`, its share of eligible posted stake at an anchor snapshot, split into accounts any way it likes | Panel seats, jury votes, a producer's collateral room. | Seats are drawn with replacement, so every seat outcome depends on `s` alone (BOND-R3, INV-BOND-02). Safety is claimed only for `s < s_target` (INV-ECON-07). |
| C3 | **GPU compute** | `c`, inference throughput as a fraction of the honest network's | Honest-looking executions and winning tickets at honest cost on the honest chain. | Weight counts only once verified (FORK-R7) and safe (FORK-R4). Compute does **not** bound a branch the adversary produces: there fake roots cost hashes (04 §2.4), so C11 bounds it. |
| C4 | **grinding budget** | `g`, hashes | Fake roots (≈ 7 BLAKE2b a ticket, 04 §2.4), nonces inside a bucket, timestamps inside the drift, tip ids, registration keys. | No free field is priced (POL-R3). No seed reads a grindable value (POL-R9). A won lottery buys only admission (POL-R5). |
| C5 | **private branches** | unlimited | Build any branch from any block, keep it private, publish at a chosen moment. Produce every block on it, and license claims on it with its own seats. | Fork choice reads no clock (FORK-R8). Maturity is burial by verified weight (FORK-R4). The finalized anchor is never reverted (FINAL-R5). |
| C6 | **message delay** | `Δ`, the maximum delay after GST | Delay, reorder and select which honest node sees which message first. Race honest producers and filers. | Every window must be longer than honest work plus `Δ` (COURT-R7, POL-R11). Selection does not depend on arrival order (FORK-R2). |
| C7 | **colluding panel fraction** | seats drawn from its accounts, plus bribed seats modelled as extra stake `s_b` | Sign `Valid` on false work, stay silent, open decoy courts. | Locks priced at `G_res/k'` (BOND-R8). Silence is never charged to a seat (PANEL-R18). Sessions cannot monopolise a claim (COURT-R5). |
| C8 | **timestamps** | any value in `(PMT, now + DRIFT_MS]` on blocks it produces | A lead of one slot on the local clock. Choice of the execution-lane round. | `DRIFT_MS < SLOT_MS` (DAA-R6). The round index never feeds a `Daa` (DAA-R16). |
| C9 | **registration** | pays the registration price | Classes, profiles within the registration rules, and accounts with stake. | Every computable quantity is derived, never declared (PANEL-R1). Economic admission (ECON-R11). |
| C10 | **adaptivity** | — | Sees all public state and every public seed before choosing its next action. | Seeds are fixed after the objects they seed (POL-R9). Populations are fixed before their seeds (PANEL-R8). |
| C11 | **self-licensing on a produced branch** | `P_cap(s, n, q, seed rule)`: the probability that one admitted claim on a branch the adversary produces is licensed by its own drawn seats, including any seed selection the seed rule leaves it | Verified weight, `SafeDaa` and safe anchors on a private branch at `P_cap(s)` × its bucket rate, with fake roots at hash cost. With `n = 5`, `q = 3` and no seed selection, `P_cap(1/3) = P(Bin(5, 1/3) ≥ 3) ≈ 0.21`. It may also bunch its licences to lift its `SafeDaa` (01 §2.3). | H2's rate condition, POL-R9's post-acceptance ring and admission-index input (OQ-26), and the bucket (CLAIM-R10). Honest seats never sign on a branch they cannot see, so nothing else applies. |

### 2.2 What the honest side is assumed to have

* **H1.** Honest eligible stake is at least `1 − s_target` of eligible stake. Owner question OQ-15
  sets `s_target`; 08 Q8-2 recommends 1/3.
* **H2.** The honest chain's licensed verified weight per tick, `R_honest`, exceeds what the adversary
  can self-license on any branch it produces, with a margin `μ`:
  `P_cap(s_target) · ρ ≤ (1 − μ) · R_honest`. Here `ρ` is 06 W1(a)'s rate constant.
  `R_honest ≈ P_lic(1 − s) · fill · ρ`, where `P_lic` is the probability that honest seats license
  an honest claim when the adversary's seats stay silent, and `fill` is the fraction of the bucket
  that honest compute fills. This is a requirement on `s_target`, on honest compute (through
  `fill`) and on the seed rule (through `P_cap`). It replaces the draft's "`c < 1/2`". A private
  branch's verified weight is bounded by the bucket and `P_cap`, not by its compute (C3, C11).
  *[Synthesis edit, review.]*
* **H3.** For every claim, at least one honest node replays it and files inside its conviction
  window when it is false. This is the detection assumption that INV-ECON-01 is conditional on
  (08 §2.5). The residual, a lie no honest party can refute, is bounded by INV-ECON-07.
* **H4.** Partial synchrony: messages among honest nodes arrive within `Δ` after GST, and honest
  wall clocks agree within `DRIFT_MS/2`.
* **H5.** A node joining, or rejoining after more than `W_trust`, holds a recent trust root
  (FINAL-R11).
* **H6.** Honest producers claim every slot on the honest chain, with heartbeats when nothing else
  is produced, up to `Δ`. This keeps the `SafeDaa` lead a racer can buy at one tick (01 §2.3, S5).
  Where H6 fails for `m` slots, the lead is at most `m + 1` (INV-CLAIM-01).

### 2.3 Out of scope for the first edition

* **Breaking cryptography.** This covers BLAKE2b collisions or preimages and ML-DSA-87 forgery.
  Domain separation between signature contexts is in scope (`cross_network_signature_replay`,
  `equivocation_keeps_fork_weight`).
* **`s ≥ s_target`, or H2 violated.** Beyond these the book promises bounds, not safety. The bounds
  are INV-ECON-07 and INV-FINAL-07.
* **Node, network, storage and EVM faults.** This covers panics, OOM, gossip amplification,
  handshakes, the bridge and model-market UX. They belong to later milestones (§7). The exceptions
  are faults that are consensus rules: `poison_block_panics_every_node`, `one_object_halts_the_chain`
  and supply effects of the market (`unbound_model_sink_output_burn`).
* **Eclipse of a node that holds a trust root, beyond `Δ`.** This is a network milestone. Its
  consensus half is `ibd_asymmetric_weighing` and `long_range_rewrite`.
* **Nondeterminism of honest model runtimes.** Class certification handles it (PANEL-R2). It is not
  treated as an adversary.
* **Concentration of genesis stake.** This is a governance matter. Its consequence is stated as a
  stake share (08 §6.3).

## 3. Reading an entry; simulator vocabulary

Each entry gives:

* the **adversary**: the capabilities it uses (C1–C10) and its preconditions;
* the **mechanism**;
* the **t12 verdict**, with evidence;
* **next**: the rule IDs and invariant IDs that defend against it;
* a **regression sketch**: what the simulator sets up and what it asserts.

Verdicts use a fixed vocabulary:

* **real**: reachable at the reference.
* **partial**: reachable but bounded, or only some of its payoffs are reachable.
* **closed**: no longer reachable.
* **by-design**: an accepted residual with a stated bound.
* **not_applicable**: t12 has no such mechanism.
* **unverified**: no code-level verdict exists.

A verdict written **unverified (catalog: X)** has no code-level verdict. `X` is appendix A's source
status, in this vocabulary, and is kept only as a lead: no chapter re-verified it in code. A later
workflow must re-verify each one before the simulator treats it as closed. Paths use the shorthand
of 09 §0: `core/`, `pipeline/`, `processes/`. *[Synthesis edit, review: these rows were written
"closed [A]" in the same column as code-read verdicts.]*

The simulator's vocabulary, which every sketch uses:

```text
net = Sim::genesis(params)                   // honest producers, honest stake H, seats, filers
atk = net.adversary(Adversary { s, h, c, g, delta, accounts })
b   = atk.fork(block)                        // private branch; b.heartbeats(k), b.attempts(k, class),
                                             // b.fake_attempts(k, class), b.object(o), b.stamp(+ms)
atk.publish(b);  net.run_slots(n);  net.halt_licences(n)
n   = net.node(i);  n.tip();  n.anchor();  n.state();  n.fork_key(tip)
assert_inv!(INV-XXX-NN, net)                 // runs the invariant's checker over the whole trace
```

## 4. The five seed attacks

### 4.1 `private_fake_root_burst`

* **Adversary.** C4 (grinding) and one producer account at the role floor. The variants add C5 (a
  private branch) and C2 (stake for its own seats).
* **Preconditions.** An admitted class with ticket probability `p`. On t12, `p = 1` for any class
  with `CCU ≥ W` (`core/palw_work_target_v1.rs:116-124`).
* **Mechanism.**
  1. The fields `trace_root`, `output_root` and `execution_root` are inside the priced bytes, but
     only a panel checks them, after the claim exists. The fabricator varies them. Each new value is
     a new ticket for about seven BLAKE2b calls (`core/palw_attempt_v2.rs:242-247`). About 270 tries
     win at `p = 2^-8`, and one try wins at `p = 1`.
  2. It mines the attempt header with the winning ticket and publishes it, or keeps it on a private
     branch.
  3. On t12 the fake claim then holds, before any panel sees it:
     * `2^20` of blue work;
     * `β·pwu` of live weight;
     * a hold on the safe frontier;
     * a seed position for panels anchored at its block;
     * a count in the `W` controller.
  4. Afterwards an honest panel refuses it (SEAT-0). The first failure redraws the panel without a
     charge. The second forfeits `w + E + rr`.
  5. On a private branch, the forker produces every anchor and re-rolls every panel
     (`panel_draw_seed_grind`), so it can license its own fake claims.
* **t12 verdict: partial.** All chapters and the appendix agree.
  * The grind is real (`core/palw_attempt_v2.rs:562-589`, `:1119-1180`). The fence doc says so:
    "C-1 stays open" (`core/config/params.rs:2457-2465`, *re-checked*).
  * Payoffs reached before any panel:
    * blue work (`processes/ghostdag/protocol.rs:666-671`, *re-checked*);
    * live weight (`core/palw_state_v2.rs:28436-28444`);
    * the frontier hold (`:20919-20944`);
    * the seed (`pipeline/virtual_processor/processor.rs:10021-10045`);
    * the `W` count (`core/palw_state_v2.rs:28543-28558`, *re-checked*).
  * Payoffs not reached:
    * safe weight and the mint (`processes/coinbase.rs:256-268`);
    * unbounded concurrency, because the exposure ceiling caps it (`core/palw_admission_v2.rs:676-700`).
  * Failure costs the forfeit (`core/palw_state_v2.rs:23733-23763`).
* **next.**
  * A won lottery buys only admission (POL-R5, CLAIM-R3); a lost one is not a valid block (11 BLK-R2).
  * Unverified claims are absent from fork choice (FORK-R7) and add no header weight (FORK-R9).
  * No seed comes from unverified work (POL-R9, CLAIM-R12).
  * The controller counts only verified claims (POL-R6, authority A8).
  * Failure is priced by the staged commitment (BOND-R5, BOND-R13). A fabricated root is convicted
    at the first panel (04 Q3, OQ-9).
  * Invariants: INV-POL-01, INV-CLAIM-03, INV-CLAIM-04, INV-POL-03, INV-POL-07, INV-FORK-05,
    INV-ECON-01.
* **Regression sketch.**
  * *Setup:* two classes, with `p = 1` and `p = 2^-8`, and an attacker account at the role floor.
    `atk` grinds fake roots until it wins in each class (≈ 270 tries for the second).
  * *Public variant:* admit the winner. **Assert:**
    * the fork keys of the carrying chain equal the keys of the same chain without the claim
      (INV-POL-01, INV-FORK-05);
    * every losing try is refused at the header stage, and the admitted header adds no weight of
      any kind (INV-CLAIM-04);
    * no seed, controller count or right changed (INV-CLAIM-03).
    * Then run honest panels. **Assert** a void with forfeit `w + E + rr`, or a conviction under
      OQ-9, and that the attacker's expected value is negative at `s < s_target` (INV-ECON-01,
      INV-ECON-07).
  * *Private variant:* `atk` with `s = 0.2` produces every block on `b`. **Assert** that each claim
    gets exactly one panel, whatever nonce, timestamp or fake root its producer uses (INV-PANEL-02,
    INV-POL-03). Measure `P_cap(0.2)` under POL-R9, including ring shaping by licence timing, and
    assert `b`'s verified weight per tick stays below H2's bound (`private_self_licensing_branch`).

### 4.2 `private_daa_finality_acceleration`

* **Adversary.** C5, C1 and C8. C2, C7 and C11 if it also wants licences on the branch.
* **Preconditions.** The branch forks from a block both sides share, and the honest network keeps
  producing.
* **Mechanism.**
  1. The private branch claims every slot the wall clock allows. On t12 that costs about `2 × 2^24`
     hashes a tick. Honest ticks lag by grind and propagation time.
  2. Claims licensed on the branch pass their 120-DAA challenge window with no honest challenger and
     become `Final`.
  3. These `Final`s raise the branch's safe frontier and safe weight. Locks and exits also release
     through the second clock's escape. Vesting rows do not release during the halt itself, but
     they do once a licence resumes past their expiry plus `2 × window_court`.
  4. The attacker publishes the branch. Fork choice compares frontiers and settled weight that each
     branch computed on its own clock.
* **t12 verdict: partial.** Chapters 01, 02, 03, 05, 06 and 07 and the appendix agree.
  * The local clock is paced by the wall clock once the floor is armed (`processes/difficulty.rs:459-517`;
    `pipeline/header_processor/pre_pow_validation.rs:82-97`).
  * `Final` is reached on the branch's own DAA (`core/palw_state_v2.rs:23662-23668`, `:23764-23771`;
    the window at `:1384-1398`; all *re-checked*).
  * Comparator key 1 is the blue score of the deepest `Final` (`core/palw_fork_choice.rs:72-78`,
    *re-checked*; `core/palw_state_v2.rs:20935`).
  * The second clock escapes after `2 × window_court` (`core/palw_state_v2.rs:2224-2229`,
    *re-checked*). That releases locks and exits; vesting stays blocked during the halt itself and
    matures once a licence resumes (`core/palw_vesting_v1.rs:494`, `:509`).
* **next.**
  * `Final` elapses on a `Deadline<Safe>` (DAA-R10, CLAIM-R9, PANEL-R20), and `Final` enters no fork
    key and no finality count.
  * `SafeDaa` is minted only from licences (DAA-R11), with no escape into the local clock (DAA-R13);
    its lead over a branch that claims every slot is one tick (01 S5, H6).
  * Comparison reads no clock (DAA-R14, FORK-R3, FORK-R8).
  * Maturity is burial by verified weight with no unreleased dispute (FORK-R4, FORK-R16).
  * Finality depth is counted in safe anchors and verified weight (FINAL-R2, FINAL-R7).
  * Exit and vesting read `ChainFinalizedDaa` (BOND-R12, BOND-R14).
  * Invariants: INV-FORK-01, INV-TIME-03, INV-TIME-06, INV-TIME-07, INV-CLAIM-09, INV-FINAL-05,
    INV-BOND-04, INV-BOND-05.
* **Regression sketch.**
  * *Setup:* two branches `A` and `A'` with identical verified content, including dispute openings
    and verdicts. `A` claims every slot (H6). `A'` stamps every block at `now + DRIFT_MS` and claims
    every slot with heartbeats. A second run leaves `m` slots unclaimed on `A`.
  * **Assert** `(safe, live)(A') ≤ (safe, live)(A)` at every corresponding block, with and without
    open disputes (INV-FORK-01).
  * **Assert** `SafeDaa(A') − SafeDaa(A) ≤ 1` at every block, and `≤ m + 1` in the second run (S5,
    INV-TIME-06, INV-CLAIM-01).
  * **Assert** no `FinalClaim`, vesting release or exit occurs on `A'` more than that many ticks
    before it occurs on `A` (INV-CLAIM-09, INV-BOND-05).
  * **Assert** that publishing `A'` moves no node's finalized anchor (INV-FINAL-05).

### 4.3 `heartbeat_clock_acceleration`

* **Adversary.** C1 and C8, on the public chain or a private branch.
* **Preconditions.** None.
* **Mechanism.** The cheap heartbeat lane advances the clock that eligibility, deadlines, refills and
  maturity read. On t12 that lane is the only clock source. `palw_single_lottery` leaves every
  admitted lane unpriced, so only a granted heartbeat moves the DAA score. It also adds blue score,
  which t12's finality, pruning and frontier depths count.
* **t12 verdict: partial.**
  * The rate is closed on the honest chain: the floor rules H3 and H5
    (`pipeline/header_processor/pre_pow_validation.rs:82-97`) allow at most one tick per 120 s plus
    a two-slot lead.
  * But the heartbeat is the whole clock (`processes/difficulty.rs:459-517`, `:705-722`), and every
    DAA rule is paced by an unbonded lane.
  * An absent reference grants a tick unconditionally (`core/palw_clock_cursor_v1.rs:118-122`).
  * Heartbeats add blue score (`processes/ghostdag/protocol.rs:338`), and finality and pruning are
    blue-score depths (`core/config/params.rs:2781`, *re-checked*).
  * The DNS veto's TTL runs on the DAA they drive (`pipeline/virtual_processor/dns_bft.rs:554-563`).
  * They mint nothing (`pipeline/body_processor/body_validation_in_context.rs:74-89`).
  * Chapter verdicts: 01, 02, 03, 06 and 07 say partial. 05 said "unverified" and now defers to 01.
    08 closes the issuance half. The appendix's source status is by-design / closed as acceleration
    (A §3.13.2).
* **next.**
  * A slot clock with the header-committed `clock_slot` (DAA-R1 to DAA-R5) and `DRIFT_MS < SLOT_MS`
    (DAA-R6).
  * A heartbeat's only authority is `LocalDaa` (DAA-R9). Every profitable rule reads `SafeDaa`
    (DAA-R10).
  * Heartbeats mint nothing (ECON-R2). Fork keys take no clock (FORK-R8). Finality ignores blocks
    without verified work (FINAL-R7).
  * Invariants: INV-DAA-02, INV-TIME-06, INV-TIME-07, INV-ECON-05, INV-FORK-05, INV-FINAL-05.
* **Regression sketch.**
  * *Setup:* `atk` with `h` at 100× the honest heartbeat rate, on the public chain and on a private
    branch, over `10^4` slots. The baseline run's honest heartbeats claim every slot (H6).
  * **Assert** `LocalDaa ≤ (now + DRIFT_MS − genesis)/SLOT_MS` (INV-DAA-02).
  * **Assert** that `LocalDaa`, `SafeDaa`, `ChainFinalizedDaa`, every fork key and every subsidy equal
    those of the baseline up to the one-slot drift lead. A slot is claimed once, so extra heartbeats
    add no ticks (INV-TIME-06, INV-FORK-05, INV-ECON-05).
  * **Assert** that no absence conviction, refill, maturity or release fires on an appended
    heartbeat-only stretch (S4, INV-TIME-07); only uncharged `NoRing` voids may fire.

### 4.4 `bond_split_amplification`

* **Adversary.** C2, holding stake `S` split into `N` accounts, with `N` chosen freely.
* **Preconditions.** None.
* **Mechanism.** Any right granted per account rather than per unit of stake multiplies with `N`.
  The candidate rights are:
  * issuance;
  * lottery draws;
  * concurrent capacity;
  * panel seats;
  * the slash taken per conviction;
  * the strike count;
  * per-account allowances.
* **t12 verdict: partial.** 02 and 03 say partial, 05 says partial, and 06 says not applicable.
  The appendix's "FIXED" is corrected in A §3.13.2.
  * **Neutral:**
    * issuance (`core/palw_state_v2.rs:28456-28460`);
    * epoch budgets (`:4317-4325`);
    * the ceiling (`:2609`);
    * tickets (`core/palw_attempt_v2.rs:581-589`).
  * **Not neutral:**
    * seats: successive sampling without replacement, one seat per operator, a 1M cap
      (`core/palw_panel_v2.rs:35`, `:137`, `:1729`, *re-checked*). At 17.29M MSK, P2 is 0 as one
      operator and 0.5003 as 133 operators (ADR-0152 §4.3; Monte Carlo 0.5004, 02 §6.3).
    * tiers `min(x‰·C₀, 3G)` (`core/palw_state_v2.rs:3294-3316`);
    * strikes per bond (`:3337-3348`);
    * the `⌈c/2⌉` class share (`:12239`, *re-checked*);
    * 64 free reporter commitments per bond (`:967`, *re-checked*).
* **next.**
  * Every authority is superadditive in stake (BOND-R2).
  * Seats are drawn with replacement, uncapped (BOND-R3).
  * Tiers are `m·G` with class floors (BOND-R10).
  * Reporter commitments are priced (BOND-R15). There is no per-account escalation (BOND-R16).
  * Per-bond limits are collateral-linear (CLAIM-R6).
  * Invariants: INV-BOND-01, INV-BOND-02, INV-BOND-03, INV-BOND-08.
* **Regression sketch.**
  * *Property:* partition `S` into `N ∈ {1, 2, 17, 133, 1000}` accounts under identical seeds.
    **Assert:**
    * issuance and concurrent capacity are equal for every partition;
    * the per-offence debit and the reporter room are equal;
    * the distribution of seats held matches `Bin(n, s)` by a chi-square test (INV-BOND-02).
  * *t12 regression:* 17.29M MSK against 7.51M honest. **Assert** that P2 equals `s²` ≈ 0.486 both
    as 1 account and as 133.

### 4.5 `failed_lottery_blue_weight`

* **Adversary.** C4 alone. No bond is needed, since the header stage checks only the carried key.
* **Preconditions.** None.
* **Mechanism.** An attempt header whose ticket lost the class lottery, or never faced it, is still a
  valid DAG block. Merged blue, it pays its block-level work to descendants.
* **t12 verdict: real.** The appendix's "[unverified] sink impact" is resolved in A §3.13.2.
  * GHOSTDAG assigns `2^20` to any attempt header from the header alone
    (`processes/ghostdag/protocol.rs:625-671`, *re-checked*).
  * Layer 0 admits any attempt digest once the single lottery is active (`consensus/pow/src/lib.rs:591-596`,
    *re-checked*).
  * The lottery runs only in the virtual processor
    (`pipeline/virtual_processor/processor.rs:11505-11552`), and a merged loser has only its claim
    skipped (`:11630-11652`).
  * Blue work orders the sink heap (`:13636-13644`, *re-checked*), every selected parent
    (`processes/ghostdag/protocol.rs:216-220`, *re-checked*) and headers-proof acceptance
    (`processes/pruning_proof/validate.rs:488-551`).
  * For PALW safe and live weight the attack is closed, because no claim exists without admission
    (06).
  * Chapter verdicts: 03, 04, 06 and the appendix say real. 01 was unverified on the clock side. 02
    said not applicable.
* **next.**
  * No block-level work exists (CLAIM-R3, POL-R5, FORK-R9). A lost ticket is invalid at the header
    stage against the carried target, and a refused attempt confers nothing (11 BLK-R2, BLK-R5,
    BLK-R6; OQ-13).
  * One comparator is used at every site, including the selected parent (FORK-R10).
  * Invariants: INV-CLAIM-04, INV-BLK-02, INV-FORK-05, INV-FORK-06.
* **Regression sketch.**
  * *Setup:* two tips. `T1` tries to add `k = 10^3` lost-lottery attempt headers signed by an
    unbonded key, plus `k` won-but-refused ones. `T2` adds one admitted, verified and buried claim.
  * **Assert** that every lost-lottery header is refused at the header stage, and that the refused
    ones leave every key, the clock and the coinbase as if absent (INV-CLAIM-04, INV-BLK-02).
  * **Assert** that `select_tip` returns `T2` at every selection site (INV-FORK-06).
  * **Assert** that the selected parent of a block merging both is chosen by `compare_chains`.

## 5. Consensus attacks by class

The columns are: adversary and preconditions | mechanism | t12 verdict and evidence | next (rules ·
invariants) | regression sketch. The seed attacks are in §4 and are not repeated here.

### 5.1 Time and clocks

| name | adversary · pre | mechanism | t12 | next | regression sketch |
| --- | --- | --- | --- | --- | --- |
| `unpriced_lane_advances_daa` | any producer | A lane without a price advances the clock, so windows run 10–180× fast. | closed; no V2 lane ticks on its own (`processes/difficulty.rs:705-722`, 01 C02) | DAA-R1, DAA-R4 · INV-DAA-01, INV-DAA-02 | 1,000 round, receipt and attempt blocks in one slot; assert `LocalDaa` rises by ≤ 1. |
| `heartbeat_rows_price_bonded_lane_out` (alias `heartbeat_bits_lockout`) | C1 | Heartbeat rows in the difficulty window tighten the bonded lane's `bits`. | closed (`processes/difficulty.rs:583-585`, 01 C07) | DAA-R8 · INV-TIME-02 | 10^4 heartbeat-only slots, then an honest attempt; assert its target equals `target_in_force(SafeDaa)` of a run without them. |
| `bondless_attempt_row_grind` | C4, no bond | Free roots and anchors grind unadmitted attempt rows that tighten `bits`. | partial: the `bits` payoff is closed on t12 (`processes/difficulty.rs:357`, `:575`, `:583-585`, *re-checked*); the header grind is open (`core/config/params.rs:2457-2465`) and pays through `failed_lottery_blue_weight` | DAA-R8, CLAIM-R3, POL-R4 · INV-CLAIM-04 | 10^6 unbonded attempt headers; assert no target, `W` or fork key moves and each header's work is `≤ ε`. |
| `clock_slot_rule_freeze` (alias `busy_producer_clock_starvation`) | honest busy producers | Blocks that do not tick push the next slot back, freezing the clock. | closed by the cursor (01 C03, C04; C11 is the inert pre-cursor rule) | DAA-R1 · INV-DAA-03 | Attempts every second for 100 slots; assert one tick per claimed slot. |
| `clock_reference_node_local` | none | Archival and pruned nodes derive different clocks. | closed; 01 INV-DAA-06 holds (`processes/difficulty.rs:470-500`) | DAA-R5 · INV-DAA-06 | Replay a chain on archival and pruned nodes; assert equal `ClockState` per header. |
| `economic_deadline_on_heartbeat_clock` | C1, a retiring signer | Liabilities and withdrawals expire on the DAA alone. | unverified (catalog: closed (`6bb8c844`); residual `second_clock_heartbeat_escape`) | DAA-R10, BOND-R14 · INV-TIME-07, INV-BOND-04 | Heartbeat-only stretch after a lying `Valid`; assert the lock and exit remain. |
| `second_clock_heartbeat_escape` (alias `private_daa_vesting_release`) | C1 plus C5, or a licence halt | After `2 × window_court` without a licence the second clock switches off, and locks and exits release on DAA alone. Vesting is blocked during the halt itself, because a row matures only when not halted. Once a licence resumes, every row whose expiry plus `2 × window_court` has passed matures at once: the second clock holds an obligation at most that long. | real (`core/palw_state_v2.rs:2224-2229`, `core/palw_panel_var_v1.rs:241-243`, *re-checked*; vesting: `core/palw_vesting_v1.rs:494`, `:509`; per-obligation bound `core/palw_panel_var_v1.rs:246-259`, *re-checked*) | DAA-R13, BOND-R12, BOND-R14 · INV-TIME-06, INV-BOND-04, INV-BOND-05 | `halt_licences`, then heartbeats on `b` past a row's expiry + `2 × window_court`. Assert that no lock or exit releases. Then produce one licence and assert that no row releases. |
| `quantum_maturity_reads_wall_clock` | C1 (stop beating) | Rights mature on wall-clock rounds while the liability runs on DAA. | unverified (catalog: unverified) | ECON-R7, DAA-R16 · INV-ECON-06 | Stop heartbeats after a Final; assert no right is spendable before the row's horizon. |
| `retarget_ratchet_to_zero` | idle classes | Retargeting over the full share table ratchets targets to 0. | unverified (catalog: closed) | POL-R6 · INV-POL-08 | 9 of 10 classes idle for 100 epochs; assert every target stays in `[W₀, W_max]` and admits. |
| `idle_class_target_relaxation` | idle registrant | Relaxing an idle target buys cadence by waiting. | unverified (catalog: closed) | POL-R6, CLAIM-R10 · INV-CLAIM-02 | Idle 100 epochs, then burst; assert admissions `≤ B + rate·ΔSafe`. |
| `private_work_target_easing` (alias `private_silence_eases_target`) | C5, C1 | Crossing DAA epochs with few model claims eases `W` by ÷4 an epoch to `W₀`, and eases the pooled receipt target. | real, bounded by `W₀` (`core/palw_state_v2.rs:22736-22775`, `:23117-23121`; `core/palw_work_target_v1.rs:98-111`) | POL-R6, DAA-R17, CLAIM-R10 · INV-CLAIM-01, INV-POL-08 | 10 epochs of heartbeat-only `LocalDaa` on `b`; assert `target_in_force` equals the honest branch's at equal `SafeDaa`. |
| `epoch_boundary_budget_mismatch` | none | The parent's budget table is read against the child's epoch index. | unverified (catalog: closed) | CLAIM-R10 (no epoch budget) · INV-CLAIM-02 | Admit at a `SafeDaa` epoch boundary; assert it admits like any other block. |
| `span_unit_mismatch_short_windows` | honest verifiers | 600 s spans are counted as 1 DAA, so windows are 1/5 of intent. | unverified (catalog: real at the reference; fix *pending*) | DAA-R15, POL-R11, COURT-R7 · INV-TIME-02, INV-COURT-06, INV-CLAIM-10 | Register a class with a measured replay time; assert `D(c)` covers it through a named rate function, or the class is refused. |
| `w_controller_counts_nonfinal_blocks` | C4 fabricator | Claims are counted at acceptance and never uncounted, so `W` hardens. | partial (`core/palw_state_v2.rs:28543-28558`, `:40913-40918`, `:22748-22755`, *re-checked*) | POL-R6, authority A8 · INV-CLAIM-01, INV-POL-01 | Fill an epoch with fabricated wins that void; assert the next `W` equals the `W` computed without them. |
| `heartbeat_future_stamp_step` | C1, C8 | 132 s of drift against a 120 s slot lets the clock tick every few seconds. | closed (H5, `pipeline/header_processor/pre_pow_validation.rs:94-97`) | DAA-R1, DAA-R6 · INV-DAA-02 | Stamp every beat at `now + DRIFT_MS`; assert a lead of at most one slot, ever. |
| `heartbeat_sibling_step_delay` | C8 | A future-stamped sibling step pushes the next slot back. | closed (`core/palw_clock_cursor_v1.rs:140-146`) | DAA-R5 · INV-DAA-03 | Publish a future-stamped sibling; assert the next slot is unchanged. |
| `heartbeat_width_burst` | C1 | Sibling beats merged in bulk add blue score. | closed (`pipeline/header_processor/post_pow_validation.rs:153-195`) | DAA-R4, DAA-R9 · INV-DAA-01, INV-FORK-05 | Merge 100 siblings; assert ≤ 4 per mergeset, `LocalDaa` +≤ 1, and no fork-key change. |
| `clock_reference_window_escape` | producers merge no beat | 264 blue blocks at one score push the reference out of the window, and the next beat is granted unconditionally. | real, low (`core/palw_clock_cursor_v1.rs:118-122`; `processes/difficulty.rs:502-505`) | DAA-R5 · INV-DAA-02, INV-DAA-06 | 300 blue blocks at one `LocalDaa`, then an early beat; assert it ticks only if its slot exceeds `last_slot`. |
| `private_absence_conviction` | C5 | Local deadlines convict honest parties who could not act on a private branch. | real if the branch wins fork choice (`core/palw_state_v2.rs:23676-23743`, `:21933-21960`, `:23438-23444`, `:22890-22905`; `core/dns_bft_v1.rs:381-388`) | DAA-R10, PANEL-R21 · INV-TIME-07, INV-CLAIM-09 | `b` passes every receipt, court and DA deadline in `LocalDaa` without licences; assert no conviction, reclamation or charge fires. |
| `blue_depth_unit_mismatch` | C1 plus attempts | Depths measured in blue score are sized from DAA windows. | partial (`core/config/params.rs:2781`, `:2697-2718`) | DAA-R15, FINAL-R2, FINAL-R10 · INV-TIME-02, INV-FINAL-05 | Compile-fail on `BlueScore` against `DaaSpan`; pad blue score 10× and assert finality and pruning are unchanged. |

### 5.2 Claims and Proof-of-LLM

| name | adversary · pre | mechanism | t12 | next | regression sketch |
| --- | --- | --- | --- | --- | --- |
| `one_execution_many_claims` | producer | One execution backs many claims. | closed (`core/palw_state_v2.rs:28336-28360`) | POL-R8 · INV-POL-05 | Re-announce one execution in 2 blocks and 2 siblings; assert one claim. |
| `sibling_identity_pow_reuse` | producer | Commitment fields are swapped under one PoW. | unverified (catalog: closed) | POL-R1, POL-R3 · INV-POL-02 | Mutate each field of an admitted attempt; assert a new ticket or an invalid block. |
| `foreign_bond_signature_theft` | any key | Commit under a victim's account. | unverified (catalog: closed) | CLAIM-R2 · INV-CLAIM-03 | Name another account under one's own key; assert refusal and no charge to the victim. |
| `nonce_free_lottery_draws` | producer | The nonce or timestamp sits inside the ticket. | closed for tickets (`core/palw_attempt_v2.rs:562-572`); open for block identity (see `panel_draw_seed_grind`) | POL-R1, POL-R4 · INV-POL-02 | Sweep a bucket's nonces; assert one ticket. |
| `unpinned_priced_field_free_draw` | producer | A free field inside the priced bytes acts as a nonce. | closed (`core/palw_admission_v2.rs:833-850`; `pipeline/header_processor/pre_ghostdag_validation.rs:266-274`) | POL-R3 · INV-POL-02 | Vary each derived field; assert refusal at admission and on the relay path. |
| `short_job_same_job_id` | producer and seats | A smaller job is run under the same id. | closed (ADR-0117 D3; seat code [unverified]) | POL-R12 · INV-POL-06 | Serve a short job with the right id; assert `check_receipt` refuses a `Valid`. |
| `borrowed_root_claim` | producer Sybils | Another producer's roots are reused. | unverified (catalog: closed; auto-filing *pending*) | POL-R13, COURT-R4 · INV-POL-05, INV-POL-07, INV-COURT-03 | Claim with the lender's roots; assert conviction from the record and the lender untouched. |
| `accepted_but_unprosecutable_claim` | registrant | Claims are admitted that no court can try. | unverified (catalog: closed) | PANEL-R5, COURT-R9, POL-R11 · INV-COURT-06, INV-ECON-10 | Register a class whose worst job exceeds the carriage; assert refusal. |
| `unattributable_2m_claims` (alias `unmeasured_class_admission`) | producer on 2M | Replay (~9.7 d) exceeds every deadline; attention lies are unattributable. | real at the reference: C7, `c = 1` (`core/palw_work_target_v1.rs:217-228`); closure *pending* | ECON-R11, PANEL-R5 · INV-ECON-10, INV-ECON-01 | A class whose reference replay exceeds its window; assert `admit_class_econ` refuses it. |
| `registration_signature_partial_preimage` | anyone | A registration is re-bodied or replayed. | unverified (catalog: closed) | PANEL-R1 · — | Mutate any field of a signed registration; assert refusal. |
| `fp_da_obligation_self_written` | FP producer | The producer writes its own DA obligation. | unverified (catalog: unverified) | POL-R3 · INV-POL-02 | Vary the free-prompt DA fields; assert they are derived and pinned. |
| `declared_canonical_job_weight_inflation` (aliases `pwu_declaration_inflation`, `fake_class_profile`) | registrant | A declared job inflates weight. | closed (`core/palw_admission_v2.rs:452-473`) | POL-R7, POL-R15, PANEL-R1 · INV-POL-04 | Two declarations of one graph; assert identical `DerivedWork`. |
| `declared_class_target_free_weight` | registrant | A declared target of 1 gives pwu = `u64::MAX`. | unverified (catalog: closed) | POL-R7 · INV-POL-04 | Register with target 1; assert `derive_work` stays bounded. |
| `attention_geometry_price_inflation` | registrant | Unbounded geometry prices attention. | unverified (catalog: closed) | PANEL-R1, POL-R15 · INV-POL-04 | Fuzz geometry; assert `DerivedWork` within its ratio bound. |
| `fp_self_reported_work` | FP executor | The executor reports its own work. | unverified (catalog: closed) | POL-R7 · INV-POL-04 | Inflate `work_leaves` ×10; assert refusal. |
| `uncertified_class_fake_weight` | registrant | An uncertified class bears weight. | unverified (catalog: closed) | PANEL-R2 · INV-PANEL-09, INV-ECON-10 | A class with an uncertified kernel; assert no weight and no reward. |
| `pay_falls_with_model_width` | registrant | Pay per compute falls with model width. | unverified (catalog: closed) | POL-R15 · INV-POL-04 | Equal MAC-equivalents at different widths; assert equal pay per unit. |
| `sampled_verification_evasion` | lying producer | Sampling misses a minimal lie. | unverified (catalog: closed) | POL-R10, PANEL-R13 · INV-PANEL-01 | A one-token lie; assert `Sampled` never counts and full coverage at basis 2 is needed. |
| `optimistic_single_seat_licence` | colluding full seat | One replay licenses the claim. | closed: the Final gate redraws, then voids `NotReplayBacked`, a licence awaiting replay (`core/palw_state_v2.rs:23744-23771`, *re-checked*); `:2930-2933` is only the panel-room count | CLAIM-R8, PANEL-R15 · INV-CLAIM-08, INV-PANEL-01, INV-PANEL-08 | The full seat alone says `Valid`; assert no `VerifiedClaim`, no weight, no release. |
| `adjudicator_convicts_honest_execution` | none | The court's arithmetic differs from the engine. | unverified (catalog: closed) | PANEL-R2 · INV-COURT-01 | Vectors: every certified kernel's recomputation equals the engine output. |
| `arithmetic_substitution_attacks` | producer | Cheaper arithmetic is passed off as the class's. | unverified (catalog: closed) | PANEL-R2 · INV-COURT-03 | Port the 13 `attack_*` cases of `core/palw_adversarial.rs` as vectors; assert conviction. |
| `tolerance_band_model_substitution` | producer | A tolerance band accepts a different model. | unverified (catalog: by-design (exact within class)) | PANEL-R2 · INV-COURT-01 | Substitute a near model; assert conviction. |
| `output_text_only_binding` | producer | Only the output text is bound. | unverified (catalog: by-design) | POL-R1 · INV-POL-02 | Flip a seed bit with the same text; assert a different commitment. |
| `accuser_authored_binding_conviction` | any account | The refutation is bound only to the public root. | unverified (catalog: closed) | COURT-R4 · INV-COURT-03 | Accuse an honest claim with a self-bound refutation; assert `Unadjudicable`. |
| `court_never_convicts` | lying producer | Structural acquittal. | unverified (catalog: closed) | COURT-R1 · INV-COURT-01 | Planted-fault drill per family; assert conviction. |
| `self_chosen_close_step_acquittal` | lying producer | The accused closes at a step it chooses. | unverified (catalog: closed) | COURT-R4 · INV-COURT-03 | Close with a step the ladder did not narrow to; assert refusal. |
| `held_attention_consistent_forger` | forger | The held anchor lies after the disputed position. | unverified (catalog: real at the reference; *pending*) | COURT-R6, PANEL-R5 · INV-COURT-05 | Held class with a consistent forger; assert a conviction, or the class is refused. |
| `held_attention_lie_unattributable` (alias `held_attention_lie_8k`) | C7 with a V1 quorum | An `AttnFused` lie on a held class has no conviction route. | real (`core/palw_state_v2.rs:8117-8136`); A-held *pending* | PANEL-R5, COURT-R9, ECON-R11 · INV-ECON-10, INV-POL-07 | Try to admit a class with an unattributable fault class; assert refusal. |
| `decoy_dissection_preemption` (alias `decoy_court_session_monopoly`) | forger's Sybil | A decoy court opened first shields the lie. | partial (`core/palw_state_v2.rs:23985-23987`); *pending* | COURT-R5 · INV-COURT-04 | Decoy, then an honest accusation; assert it is admitted and its window unshortened. |
| `deliberate_court_loss_slashes_signers` (alias `default_as_signer_evidence`) | colluding producer | Losing or defaulting a court slashes honest signers. | partial: defaults closed (`core/palw_state_v2.rs:3805-3818`); dissection verdicts *pending* | COURT-R3 · INV-COURT-02 | The producer defaults its own court; assert only the producer is convicted. |
| `forger_race_challenger_forfeit` (alias `forgers_race`) | consistent forger | The challenger's forfeit is not restored after a later conviction. | real (`core/palw_state_v2.rs:25516-25518`); *pending* | COURT-R6 · INV-COURT-05 | Acquittal at the bottom, then a checkpoint conviction; assert the challenger is made whole. |
| `weight_history_rewritten_by_retarget` | none | pwu taken from the current target rewrites history. | unverified (catalog: closed) | POL-R7 (snapshotted in `ClaimCore`) · INV-FORK-08 | Retarget after Finals; assert past weights unchanged. |
| `open_claim_frontier_pin` | C4 fabricator | An open claim holds the frontier. | partial (`core/palw_state_v2.rs:20919-20944`) | CLAIM-R13, FORK-R4 · INV-CLAIM-10 | Hold a fake claim open to `D_max`; assert honest keys unaffected and the claim terminal by `D_max`. |

### 5.3 Panel and court

| name | adversary · pre | mechanism | t12 | next | regression sketch |
| --- | --- | --- | --- | --- | --- |
| `panel_draw_seed_grind` (alias `panel_anchor_regrind`) | the anchor's producer; C4 | Seeds hash the anchor's block identity, so nonce (≈ 2^22 in a bucket) and timestamp re-roll every panel it anchors at one signature per try. | real, from code (`core/palw_panel_v2.rs:1480-1510`, `:1664-1684`; `core/hashing/header.rs:165-174`; `core/palw_attempt_v2.rs:213`, `:233-235`; all *re-checked*); [A]'s "FIXED" corrected | PANEL-R7, POL-R9, CLAIM-R12 · INV-PANEL-02, INV-POL-03 | The anchor producer varies nonce and timestamp 2^16 times; assert an identical panel every time. |
| `admission_jury_seed_grind` | anchor producer | The jury seed hashes the anchor block. | real (`core/palw_model_registry_v1.rs:465-471`); seed v2 *pending* | PANEL-R3, PANEL-R7 · INV-POL-03 | Vary the anchor nonce; assert the jury is unchanged. |
| `admission_jury_sybil_capture` | C2 Sybils | The jury is one ticket per operator. | partial (`core/palw_state_v2.rs:16909-16960`; `core/palw_panel_v2.rs:1462-1468`) | PANEL-R3 (stake-weighted, OQ-10) · INV-PANEL-09, INV-BOND-02 | 20 floor accounts against honest stake; assert the capture law depends on stake share only. |
| `executor_judges_own_claim` | executor | The executor sits on its own panel. | closed (`core/palw_panel_v2.rs:1052`) | PANEL-R9 · INV-PANEL-03 | The executor holds many accounts under one key; assert none is drawn. |
| `registrant_self_certifying_panel` | registrant | Panels are drawn from the registrant's own bonds. | unverified (catalog: closed) | PANEL-R14(5) · INV-PANEL-06 | A bought class; assert the outsider's `Valid` is required. |
| `seat_sybil_by_cheap_identities` | C2 Sybils | Cheap identities buy seats. | unverified (catalog: closed; residual `undetected_coverage_lie`) | BOND-R3, BOND-R4 · INV-BOND-02 | 1,000 floor accounts; assert seats follow stake share only. |
| `undetected_coverage_lie` (alias `sybil_seat_capture`) | C2 at share `s` | Holding both attesters of a segment (P2), or a V1 quorum (P3), licenses a lie nobody can refute. | partial: Sybil-stake thresholds 17.29M MSK (P2) and ≈ 6.63M (P3, 8k) (ADR-0152 §4.3; 02 §6.3, 08 §4.5) | BOND-R3, ECON-R11(d) · INV-BOND-02, INV-ECON-07 | Sweep `s`; assert P2 = `s²` and P3 = `P(Bin(5,s) ≥ 3)` under every split, and that a class below `s_target` is refused. |
| `colluding_quorum` | C7, 3 of 5 seats | False work is licensed, but one honest replay can detect it. | partial (`core/palw_panel_v2.rs:2437-2591`; `core/palw_offence_v1.rs:448-456`) | PANEL-R14, BOND-R8, COURT-R4 · INV-ECON-01, INV-PANEL-01 | A colluding quorum and one honest filer; assert a conviction and `EconMargin ≥ 0`. |
| `false_valid_signer` | C7 seat | A seat signs `Valid` on refutable work. | partial (`core/palw_offence_attribution_v1.rs:1-50`, `:85-91`); filer *pending* | COURT-R4, COURT-R9 · INV-COURT-03 | Planted fault; assert conviction, including an over-size proof, or the class is refused. |
| `dual_quorum_opposite_licences` (alias `quorum_exclusivity`) | seats | `Valid` and `Unavailable` quorums form on one panel. | closed (`core/palw_panel_v2.rs:355`) | PANEL-R6 · INV-PANEL-04 | `validate_params` with `2q ≤ n` is refused. |
| `relay_loss_convicts_honest_producer` | C6 | `Unavailable` acts as a guilty vote. | unverified (catalog: closed) | PANEL-R13 · INV-PANEL-11 | Drop 30% of material; assert no charge without a DA default. |
| `seat_silence_mispriced` (alias `silence_is_not_checkable`) | seats | Silence is priced wrongly, one way or the other. | unverified (catalog: by-design) | PANEL-R18 · INV-PANEL-11 | Silent seats; assert no seat is charged. |
| `silent_quorum_griefing` | C7 silent seats | A second silent panel charges the honest producer. | real (`core/palw_state_v2.rs:19164-19171`, `:23676-23741`) | PANEL-R19, BOND-R13 (OQ-7) · INV-PANEL-11, INV-ECON-01 | Attacker seats stay silent on honest claims; assert the producer's expected loss per event is at most what the attacker locked. |
| `unavailable_majority_voids_free` | seat majority | A majority voids claims for free. | unverified (catalog: closed) | COURT-R11 · INV-COURT-02 | A majority says `Unavailable` on served material; assert no void without a DA default. |
| `falsevalid_unfileable` | lying seat | A hash fixed point made false-Valid unfileable. | unverified (catalog: closed) | COURT-R4 · INV-COURT-03 | File a false-Valid against a block-lane claim; assert it is admissible. |
| `da_court_single_session_preemption` | Sybil accuser | A single DA session pre-empts others. | unverified (catalog: closed) | COURT-R11, COURT-R5 · INV-COURT-04 | A Sybil opens first; assert the honest session is admissible. |
| `honest_seats_slashed_by_unaligned_checks` | none | Seat checks are weaker than the conviction rule. | unverified (catalog: closed) | POL-R12, PANEL-R22 · INV-POL-06 | For every conviction rule, assert the seat's check refuses the same fault. |
| `executor_conviction_cascades_to_seats` | lying executor | An executor's conviction reaches honest seats. | unverified (catalog: closed) | COURT-R4 · INV-COURT-03 | Convict the executor; assert honest seats are untouched. |
| `free_da_accusation_griefing` | accuser | Unanswered DA accusations grief producers. | unverified (catalog: closed) | COURT-R11 · INV-ECON-01 | 20 accusations against an answering producer; assert the accuser pays. |
| `withholding_producer_uncharged` (alias `withholding_amplification`) | producer | Binding panels and serving nothing pins seats' capital. | closed (`core/palw_state_v2.rs:2904-2906`) | BOND-R7 · INV-BOND-07 | Bind and withhold; assert `Σ duties ≤ commitment` and the producer forfeits. |
| `seat_reward_exceeds_slash` | lying seat | The reward exceeds what the seat can lose. | unverified (catalog: closed) | BOND-R8 · INV-ECON-01 | Every class; assert each lock ≥ the residual gain per `k'`. |
| `registrant_priced_seat_penalty` | registrant | The registrant chooses seat penalties. | unverified (catalog: closed) | BOND-R10 · INV-BOND-08 | Vary registrant fields; assert seat penalties unchanged. |
| `producer_defaulted_unsigned_verdicts` | anyone | Unsigned verdicts burn bonds. | unverified (catalog: closed) | PANEL-R12 · INV-COURT-02 | An object with empty receipts; assert refusal. |
| `forged_false_valid_equivocation` | stranger | Evidence is verified under a key it carries. | unverified (catalog: closed) | COURT-R4 · INV-COURT-03 | Evidence signed by a carried key; assert refusal. |
| `whole_collateral_slash_of_uncarried_signer` | none | An uncarried signer loses its whole collateral. | unverified (catalog: closed) | BOND-R8, BOND-R9 · INV-BOND-09 | Convict an uncarried `Valid`; assert the debit is at most its lock plus tier. |
| `replay_budget_horizon_collapse` | registrant | One short-window class holds every class. | closed (`core/palw_state_v2.rs:12022-12027`, `:17256-17270`) | per-class rate room · INV-PANEL-12 | Register a short-window class; assert other rooms are unchanged. |
| `panel_room_squat_by_small_claims` | FP producer | Small commitments fill the room. | unverified (catalog: closed; 2M *pending*) | CLAIM-R2 · INV-CLAIM-02 | Flood the smallest free-prompt claims; assert room use proportional to compute. |
| `panel_room_zero_readiness_halt` | none | A flag day with zero readiness rows halts panels. | unverified (catalog: closed) | PANEL-R4, PANEL-R10 · INV-PANEL-12 | Genesis with no readiness rows; assert draws bind. |
| `possession_proof_binds_index_only` (alias `readiness_proof_outsourced`) | stakeless party | Readiness is proven by fetching public leaves. | real (`core/palw_model_registry_v1.rs:936-944`, `:1055-1062`); by-design in t12 | PANEL-R4 · INV-BOND-02 | An account holding nothing proves readiness; assert it gains nothing beyond its stake. |
| `panel_bound_poisons_block` | anyone | An unsigned `PanelBound` disqualifies its carrying block. | unverified (catalog: closed) | PANEL-R11 (the fold binds, OQ-8) · INV-PANEL-12 | A lock-ineligible `PanelBound`; assert the carrying block stands. |
| `reporter_reward_front_running` (alias `report_front_running`) | copier | A filing is copied into an earlier block. | unverified (catalog: closed in consensus); INV-COURT-07 unknown | COURT-R8, ECON-R10 · INV-COURT-07 | Copy a reveal earlier; assert no reward. |
| `panel_redraw_inconsistency` | none | A redraw leaves deadlines and horizons inconsistent. | unverified (catalog: closed) | PANEL-R11, CLAIM-R9 · INV-CLAIM-10 | Redraw; assert deadlines, exit and horizons follow the new panel. |
| `anchor_bind_censorship` | anchor producer | Omitting `PanelBound` voids the claims anchored at its block, free. | closed at the reference: bindings are derived by the chain, not published (`pipeline/virtual_processor/processor.rs:11814-11828`, `:12290`, `:2053`; `core/palw_state_v2.rs:23733-23735`, *re-checked*). The only residual lever is grinding the anchor's hash, which is `panel_draw_seed_grind` | PANEL-R11 (kept from t12), OQ-8 · INV-PANEL-12 | The block that fixes a ring carries no binding object; assert every due panel binds and nothing a producer omits can void a claim. |
| `private_readiness_lapse_panel_capture` | C5, C1, C2 | Readiness rows last 8 DAA and, past the audit fence, gate the draw. On a private heartbeat branch honest rows lapse while the forker refreshes its own, so its seats fill the drawable population. | unverified: the lapse and the draw gate are read (`core/palw_model_registry_v1.rs:779-799`, `:859`); whether SW-10's eligible base counts only fresh rows was not read | 01 §2.7 (readiness expiry reads `SafeDaa`), PANEL-R4 · INV-PANEL-12, INV-BOND-02 | On `b`, run heartbeats past every honest row's age without licences; assert honest seats stay drawable until `SafeDaa` passes their expiry. |
| `post_anchor_grinding` | registrant | Registering after seeing the seed buys a seat. | closed (`core/palw_panel_v2.rs:272-292`) | BOND-R4, PANEL-R8 · INV-PANEL-05 | Register after the seed; assert the account is absent from the population. |

### 5.4 Bonds and economics

| name | adversary · pre | mechanism | t12 | next | regression sketch |
| --- | --- | --- | --- | --- | --- |
| `one_bond_backs_unbounded_immature_work` (alias `one_bond_unbounded_immature`) | one account | Unbounded unverified exposure on one bond. | closed (`core/palw_admission_v2.rs:676-700`; `core/palw_state_v2.rs:28488-28532`) | CLAIM-R6, BOND-R6 · INV-BOND-06 | 10^4 claims from one account; assert `committed ≤ ratio·posted`. |
| `collateral_declared_not_locked` | registrant | Collateral is declared but never locked. | unverified (catalog: closed) | BOND-R1 · INV-BOND-09 | Spend collateral in the registering block; assert refusal. |
| `lock_escape_before_conviction` (alias `withdraw_before_conviction`) | signer | Exit before a conviction lands. | partial (`core/palw_state_v2.rs:2796-2821`; escape `:2224-2230`) | BOND-R14 · INV-BOND-04 | Request exit after a lying `Valid`; assert the stake is slashable until the horizon. |
| `exit_freeze_by_retirement` | retirers | Retirements break the draw and freeze locks. | unverified (catalog: closed) | PANEL-R10 · INV-PANEL-12 | Retire down to `n` accounts; assert refusal, or that draws still bind. |
| `lock_ledger_double_backing` | none | One sompi backs two reservations. | closed (`core/palw_state_v2.rs:2601-2616`) | BOND-R6, BOND-R18 · INV-BOND-06 | Random admit, seat and accuse sequences; assert the ledger invariant holds. |
| `collateral_unit_mismatch` (alias `wrong_unit_collateral`) | none | Carve and reservation use different units (44.25×). | unverified (catalog: closed) | ECON-R14, BOND-R18 · INV-ECON-09 | Re-derive genesis floors; assert they equal the pinned values. |
| `weight_unit_gap` | fraudulent `VerifiedClaim` | Fork weight far exceeds the reservation (5,620× on 2M). | real (`core/config/premine.rs:120-128`) | ECON-R12, FORK-R4, FORK-R6 · INV-ECON-09, INV-ECON-01 | A heavy class; assert its fork effect is priced within `w`, or the class is refused. |
| `action_tier_dilution` | C2 | Small accounts pay smaller tiers. | real (`core/palw_state_v2.rs:3294-3316`) | BOND-R10 · INV-BOND-08 | One offence from 1 account and from 100; assert equal debits. |
| `strike_evasion_by_split` | C2 withholder | Rotating accounts evades strikes. | real (`core/palw_state_v2.rs:3337-3348`) | BOND-R16 · INV-BOND-03 | Assert no penalty reads an account's history. |
| `vesting_escape` | payee | Move or pledge an unmatured reward. | closed (`core/palw_state_v2.rs:19948-19972`) | BOND-R12 · INV-BOND-05 | Try to spend or pledge a row; assert there is no path. |
| `execution_quanta_overmint` | colluding quorum | Permits are minted in a unit 2,810× too large. | unverified (catalog: closed) | CLAIM-R14, ECON-R7 · INV-ECON-02, INV-ECON-06 | The largest class's Final; assert minted quanta are bounded and vested. |
| `conviction_leaves_final_rights` | convicted producer | A post-Final conviction takes nothing back. | unverified (catalog: closed) | CLAIM-R14, COURT-R10 · INV-CLAIM-03 | Convict after Final; assert rights revoked and the row burned. |
| `merged_work_payout_mismatch` | producer | Merged work is paid unlike the fold. | unverified (catalog: closed) | ECON-R3, ECON-R4 · INV-ECON-03, INV-CLAIM-05 | Merge admitted, refused and voided claims; assert the coinbase equals the fold. |
| `registration_moves_share_table` | registrant | A registration moves everyone's price. | unverified (catalog: closed) | POL-R6 · INV-POL-04 | Register a class; assert incumbents' pay per unit is unchanged. |
| `registration_spam_state_growth` | registrant | Free registrations grow rooted state. | unverified (catalog: closed) | PANEL-R1 · — | 10^4 registrations; assert the per-block cap and the burn apply. |
| `fp_commitment_flood_state_growth` | FP committer | Commitments grow rooted state at no collateral. | unverified (catalog: unverified) | CLAIM-R10 · INV-CLAIM-02 | Flood commitments; assert the bucket bounds rooted growth. |
| `heavy_recompute_budget_griefing` | anyone | Failing objects burn the recompute budget. | unverified (catalog: closed) | validation budget · — | Failing objects; assert the payer's fee covers the charge. |
| `court_default_cheaper_than_losing` | producer | Defaulting costs less than losing. | unverified (catalog: closed) | COURT-R3, BOND-R13 · INV-COURT-02 | Assert the cost of a default ≥ the cost of a loss. |
| `junk_claim_composite_stall` | one account | Junk floor claims stall useful execution. | unverified (catalog: closed) | CLAIM-R10, BOND-R5 · INV-CLAIM-02 | A junk flood; assert honest throughput stays above its floor. |
| `expensive_tier_cheap_execution` | registrant | An expensive tier is named and a cheap job run. | unverified (catalog: unverified) | POL-R15, PANEL-R1 · INV-POL-04 | Assert pay follows derived compute, not the named tier. |
| `class_id_split_same_artifact` | registrant | A field that changes no arithmetic mints a second class id. | unverified (catalog: real) | PANEL-R1 · — | Registrations differing only in `n_threads`; assert one class id. |
| `three_spellings_of_work` | registrant | Three spellings of work disagree (1.408×). | unverified (catalog: real) | POL-R15 · INV-POL-04, INV-ECON-09 | Assert work, reservation and split read one derivation. |
| `algo4_credit_mint_holes` | — | The retired pre-V2 credit path. | unverified (catalog: closed (retired)) | — · INV-ECON-02 | Kept as historical vectors only. |
| `unbounded_coinbase_fanout` | validators | Coinbase outputs are unbounded. | unverified (catalog: unverified) | ECON-R2 · INV-ECON-03 | Maximum participants; assert the coinbase output count is bounded. |
| `fp_prefix_kv_credit` | FP users | KV credit depends on commit order. | unverified (catalog: closed) | POL-R15 · INV-POL-04 | One conversation in two orders; assert equal credit. |
| `early_extraction` | fraud producer | Buyback, rights and fees are realised before the horizon. | real (`core/palw_state_v2.rs:19243`; `core/palw_economic_safety_v1.rs:95-113`) | ECON-R7 · INV-ECON-06 | A fraudulent Final; assert no leg leaves before the horizon. |
| `self_report_capture` | offender | An offender reports itself to recover the slash. | closed (`core/palw_state_v2.rs:970`) | ECON-R10 · INV-ECON-08 | The offender reports itself; assert a net loss ≥ `(1−r)·collected`. |
| `unbound_model_sink_output_burn` (alias `silent_sink_burn`) | anyone | A sink output with no binding object burns MSK silently. | real (`processes/transaction_validator/tx_validation_in_isolation.rs:102-111`) | ECON-R6 · INV-ECON-04 | A transaction pays a sink output with no object; assert refusal. |
| `model_market_payout_unwithheld` | market | Market payouts are minted with nothing withheld. | unverified (catalog: unverified) | ECON-R1 · INV-ECON-02 | A market payout; assert it is backed by a recorded burn. |

### 5.5 Fork choice

| name | adversary · pre | mechanism | t12 | next | regression sketch |
| --- | --- | --- | --- | --- | --- |
| `heartbeat_padding_buys_frontier_key` (alias `frontier_blue_score_padding`) | C1 | Key 1 is a blue score, so `K` beats plus one matured claim outrank 1,000 matured claims. | partial (`core/palw_fork_choice.rs:72-78`, `core/palw_state_v2.rs:20935`; the sustained live rate [unverified]) | FORK-R6, FORK-R8 · INV-FORK-05, INV-FORK-01 | `P`: 10^4 beats and one buried settled claim; `M`: 1,000 buried settled claims; assert `M` is selected. |
| `unverified_live_weight` | C4 | A `Provisional` claim adds `β·pwu` to live weight. | real (`core/palw_state_v2.rs:28436-28443`, `:18650-18654`) | FORK-R7 · INV-CLAIM-07, INV-POL-01 | Chains with `k` unverified claims and with none; assert equal keys. |
| `path_dependent_sink_split` | none (timing) | Heap by blue work plus a veto only on reorgs gives two sinks for one DAG. | real (`pipeline/virtual_processor/processor.rs:13219`, `:13636-13644`) | FORK-R2, FORK-R11 · INV-FORK-03 | Two nodes, one DAG, different previous tips; assert one `select_tip`. |
| `dns_gate_node_local_abstain` | circumstance | A node-local abstain in the DNS gate precedes fork choice. | partial (`pipeline/virtual_processor/dns_bft.rs:457`, `:532-536`) | FORK-R13 · INV-FORK-03, INV-FINAL-02 | Inject a node-local evaluation failure; assert the selection is unchanged. |
| `ibd_asymmetric_weighing` | IBD peer | The incumbent is weighed at its sink and the staged chain at a pruning point. | partial (`consensus/src/consensus/mod.rs:2479-2482`; `processes/pruning_proof/validate.rs:488-551`) | FORK-R14, FORK-R10 · INV-FORK-06 | IBD from a peer; assert both are weighed in one set from one anchor. |
| `unweighable_fail_open` | none | An unweighable candidate is allowed through. | closed (`pipeline/virtual_processor/processor.rs:13234-13248`) | FORK-R12 · INV-FORK-03 | A candidate with missing state; assert it is excluded. |
| `private_self_licensing_branch` | C5, C11, C4 | On a branch it produces, the adversary admits fake roots up to the bucket rate. Its own drawn seats license them with probability `P_cap(s)`, helped by any ring shaping the seed rule allows. It can bunch its last `D_SAFE` licences to lift its `SafeDaa`. The branch then grows verified weight, safe anchors and a chain anchor at `P_cap · ρ`, which compute does not limit. | real, from code reading: the forker re-rolls every panel on its own branch by the anchor's nonce and timestamp (`panel_draw_seed_grind`; 05 §7, 04 §6.4), so `P_cap` approaches the chance that its stake can fill a licensing set at all | H2's rate condition, POL-R9, CLAIM-R10, OQ-26 · INV-FORK-01 (does not cover it), INV-FINAL-07, INV-POL-03 | Sweep `s ∈ {0.1, 0.2, 1/3}` on a private branch with fake roots and licence bunching; measure `P_cap` and the branch's `safe` per tick; assert `P_cap · ρ ≤ (1 − μ) · R_honest` at `s_target`, or report the violating `s`. |
| `sybil_bond_private_fork_frontier` | C5 | Sybil accounts registered on a fork self-license it. | unverified (catalog: closed for 0.004 MSK bonds); the underlying self-licensing is real in code reading (`private_self_licensing_branch`) | BOND-R4, POL-R9 · INV-PANEL-05, INV-POL-03 | Register on a fork after it forks; assert those accounts are never drawn and the forker cannot choose panels. |
| `private_branch_double_spend` | C5, C3 | Pay publicly and settle a conflicting spend privately. | unverified (catalog: by-design) | FINAL-R1, FORK-R4 · INV-FINAL-07 | Pay, then publish a private settlement; assert confirmations read the finalized anchor. |
| `unsigned_receipt_private_fork` | forger | Unsigned receipts mature a fork. | unverified (catalog: closed) | PANEL-R12 · INV-PANEL-01 | Unsigned receipts; assert refusal. |
| `empty_fork_frontier_advances` | C5 | A fork without attempts advances the frontier. | unverified (catalog: closed) | FORK-R6 · INV-FORK-05 | 60 empty blocks; assert no key gain. |
| `prior_sink_weight_divergence` | none | Weight is read at the node's previous sink. | unverified (catalog: closed) | FORK-R11 · INV-FORK-03 | As `path_dependent_sink_split`. |
| `node_local_input_in_fold` | none | A fold reads node-local data. | unverified (catalog: closed) | FORK-R13 · INV-FORK-03 | Different local stores; assert equal state roots. |
| `selection_sites_disagree` | none | Selection sites use different authorities. | partial; [A]'s "FIXED" corrected (06 T12-D8; `core/palw_fork_authority_v2.rs:43-45`, *re-checked*) | FORK-R10 · INV-FORK-06 | Assert every site calls `select_tip`. |
| `fresh_tip_unresolved_fallback` | none | Fresh tips fall back to blue work. | closed on t12 by β weight, which is `unverified_live_weight` | FORK-R7, FORK-R8 · INV-FORK-08 | Tips carrying only unverified claims; assert the order is verified keys, then tip id, never blue work. |
| `equivocation_keeps_fork_weight` | forger | A context mismatch keeps a fabricated block's weight. | unverified (catalog: closed) | typed signature contexts, COURT-R4 · INV-COURT-03 | An equivocation certificate; assert its effect under the one typed context. |
| `object_poisons_carrying_block` | anyone | A bad relayed object disqualifies its honest carrier. | unverified (catalog: closed) | validation drops refused objects · — | Relay an invalid object; assert the carrier stands. |
| `pruning_witness_selection` | anyone | One cheap block selects the pruning witness. | unverified (catalog: closed) | FINAL-R10 · INV-FINAL-06 | A cheap block at the pruning point; assert the witness is on the selected chain. |
| `safe_frontier_scalar_not_rederived` | eclipse peer | The frontier scalar is carried, never re-derived. | unverified (catalog: unverified) | FORK-R15, FINAL-R11 · INV-FORK-06, INV-FINAL-03 | Staged IBD with a forged scalar; assert it is re-derived. |
| `zero_weight_lane_hash_tiebreak` | C4 tip grinder | Blocks with no weight tie, so the hash decides. | by-design on t12 (ε = 1, `processes/ghostdag/protocol.rs:611-624`); a regression risk in next (§6) | FORK-R5; OQ-12 · INV-FORK-02, INV-FORK-03 | See §6. |
| `heartbeat_only_trap` (alias `heartbeat_red_trap`) | circumstance | A slow bonded block merged red locks the chain on heartbeats. | closed: past `palw_heartbeat_transparent`, armed at 0 (`core/config/params.rs:15855`, 01 §7) | FORK-R9, DAA-R9 · INV-FORK-05 | A slow bonded block after heartbeats; assert its verified weight counts. |
| `execution_block_burst_confirmations` | producer | Fast blocks are shown as confirmations. | unverified (catalog: by-design) | FINAL-R1 · INV-FORK-05 | Many round blocks; assert confirmations are unchanged. |
| `header_level_and_parent_misread` | none | The wrong selected parent is read. | unverified (catalog: closed) | FORK-R10 · INV-FORK-06 | Assert the selected parent comes from `select_tip`. |

### 5.6 Finality

| name | adversary · pre | mechanism | t12 | next | regression sketch |
| --- | --- | --- | --- | --- | --- |
| `dns_veto_expires_on_heartbeat_clock` (alias `finality_ttl_release`) | C1 and waiting | The only vote-based finality is released by a DAA TTL. | real, by design as a liveness release (`pipeline/virtual_processor/dns_bft.rs:554-563`; `core/dns_finality.rs:1464`) | FINAL-R8, FINAL-R13 · INV-FINAL-01, INV-FINAL-02 | A stale anchor for 10^4 slots; assert nothing is released. |
| `settled_claim_reverted_with_branch` | C5 | A `Final` claim reverts with its branch. | real as a property (`core/palw_state_v2.rs:3233-3243`) | FINAL-R1 · INV-FINAL-01 | Reorg a branch holding Finals; assert confirmations and labels read only the anchor. |
| `pruning_point_disagreement` | none | Committed and local pruning points differ. | partial (`processes/pruning.rs:106-156`; `pipeline/pruning_processor/processor.rs:223-228`) | FINAL-R3, FINAL-R10 · INV-FINAL-03, INV-FINAL-06 | Archival and pruned nodes; assert one committed point. |
| `pruning_deletes_evidence` | none | Pruning deletes evidence still needed. | closed for the local store (`pipeline/pruning_processor/processor.rs:223-228`) | FINAL-R10 · INV-FINAL-04, INV-FINAL-06 | An open court near the horizon; assert its evidence is retained. |
| `long_range_rewrite` | withdrawn keys | Retired accounts sign an alternative past. | partial (`core/config/trusted_checkpoint.rs:1-37`; the delay against finalization is [unverified]) | FINAL-R5, FINAL-R11, FINAL-R12 · INV-FINAL-01, INV-FINAL-07 | Withdrawn keys sign a history; assert a trust-rooted node rejects it and `validate_params` refuses a short withdrawal delay. |
| `validator_count_by_bonds` | overlay | Bonds, not validators, were counted. | unverified (catalog: closed) | no overlay in the core (OQ-14) · — | Historical vector. |
| `inactivity_leak_window_mismatch` | none | The leak window mixes units. | unverified (catalog: closed) | DAA-R15 · INV-TIME-02 | Compile-fail. |
| `forged_slash_evidence_via_mergeset` | anyone | Forged slash evidence enters through the mergeset. | unverified (catalog: closed) | COURT-R1, COURT-R4 · INV-COURT-03 | Mergeset-carried evidence; assert adjudication. |
| `unverified_overlay_snapshot_import` | IBD peer | An unverified overlay snapshot is imported. | unverified (catalog: closed) | FINAL-R13 · INV-FINAL-02 | Import a snapshot; assert it is verified. |
| `dns_reorg_gate_wedge` | circumstance | The TTL counted on the node's own stopped chain. | unverified (catalog: closed) | FINAL-R8 (no TTL) · INV-FINAL-02 | Assert no release rule exists. |
| `single_block_satisfies_work_depth` | producer | One block satisfies the work depth. | unverified (catalog: unverified) | FINAL-R2 · INV-FINAL-05 | One heavy claim; assert `k_final` settled anchors are still required. |

### 5.7 Consensus hygiene

| name | adversary · pre | mechanism | t12 | next | regression sketch |
| --- | --- | --- | --- | --- | --- |
| `poison_block_panics_every_node` | anyone | Attacker bytes reach arithmetic that panics. | unverified (catalog: closed) | pure, total functions · — | Fuzz every consensus function; assert it returns `Result` and never panics. |
| `one_object_halts_the_chain` | anyone | One object drives a deterministic failure. | unverified (catalog: closed) | CLAIM-R13, total transitions · INV-CLAIM-10 | Fuzz objects; assert the state machine progresses. |
| `cross_network_signature_replay` | anyone | A signature from one network is accepted on another. | unverified (catalog: partial) | signature domain covers network and genesis · — | Sign on network A and replay on B; assert refusal. |
| `ruleset_change_without_identity_move` | none | A rule change leaves the identity unchanged. | unverified (catalog: closed (process)) | the parameter fingerprint · — | Changing any parameter changes the ruleset id. |
| `registered_graph_differs_from_executed` | registrant | The registered identity is not what runs. | unverified (catalog: real (I7)) | PANEL-R1 · INV-POL-04 | Derive genesis roots from artifacts; assert they equal the pinned roots. |
| `pruning_proof_single_lottery_off` | none | The proof validator runs different rules. | unverified (catalog: closed) | FORK-R10 · INV-FORK-06 | Assert the headers proof uses the same rules. |
| `mutated_witness_poisons_block_id` | anyone | A flipped signature bit poisons invalid-block caches. | unverified (catalog: by-design) | identity hashes raw bytes · — | Flip a signature bit; assert a different identity. |

## 6. Design-level hazards of misaka-next

The misaka-next design itself raises these hazards; none is a t12 attack. Each is a regression test
for the new design and runs in the simulator like the others.

* **`pairwise_context_cycle`** (06 §3.2 (i)). Comparing each pair at `min(t_A, t_B)` is
  intransitive: A > B > C > A.
  *Defence:* FORK-R3. *Invariants:* INV-FORK-02.
  *Sketch:* the table in 06 §3.2 (i); assert `compare_chains` is transitive over it.
* **`junk_candidate_context_drag`** (06 §3.2 (ii)). A minimum over the whole set is dragged to the
  anchor by one empty block.
  *Defence:* FORK-R3. *Invariants:* INV-FORK-04.
  *Sketch:* H, A and J from 06 §3.2; assert adding J does not change H against A.
* **`stalled_leader_context_drag`** (06 §3.2 (iii)). A minimum over "contenders" is dragged by a
  branch that led and then stopped.
  *Defence:* FORK-R3, FORK-R4. *Invariants:* INV-FORK-04.
  *Sketch:* assert H (total 500) beats J2 (total 2).
* **`safe_mark_window_collapse`** (found in synthesis). A window judged on `SafeDaa` but measured
  from the `SafeDaa` at its creation closes almost at once when the safe clock lags by more than the
  span.
  *Defence:* 01 §2.6 (marks are `LocalDaa`), 03 CLAIM-R9. *Invariants:* INV-TIME-08.
  *Sketch:* lag `SafeDaa` by `10 × s`, open a window of span `s`, and assert it stays open for
  `LocalDaa` in `[mark, mark + s]`.
* **`seed_ring_bootstrap_deadlock`** (found in synthesis). Under POL-R9, a panel seed needs `K`
  `FinalClaim` leaves fixed after the claim, and a `FinalClaim` needs a panel. Both waits hold at
  genesis and after any stretch with no `Final`.
  *Defence:* the OQ-1 decision MUST include a bootstrap and halt rule. *Invariants:* INV-PANEL-12.
  *Sketch:* start from genesis, and separately after `net.halt_licences(n)` with every claim pending;
  assert some claim is licensed within the liveness bound.
* **`licence_halt_stake_freeze`** (found in synthesis). In a licence halt `SafeDaa` stops (DAA-R13),
  and so does `ChainFinalizedDaa` (07 FINAL-R2). So 02 Q2-4 (a)'s exit escape, which is measured in
  `ChainFinalizedDaa`, never fires, and staked value stays frozen for the whole halt. Adding back any
  escape on `LocalDaa` is unsafe: during a halt every candidate has equal verified weight, the tip
  id decides (FORK-R5), and a private heartbeat-only branch that ran the escape can win that tie
  (`zero_weight_lane_hash_tiebreak`).
  *Defence:* OQ-4 (the owner's choice). *Invariants:* INV-BOND-04, INV-TIME-06.
  *Sketch:* halt licences, then publish a heartbeat-only branch with a ground tip id. Assert that no
  value is released on either branch, and that the halt's end restores exits.
* **`dispute_hold_griefing`** (found in review). Fork choice holds a disputed entry out of `safe`
  until a proven verdict or `d_dispute` of verified weight (06 §3.3). An accuser can therefore open
  sessions against honest buried claims to delay their safe weight, and with it finality.
  *Defence:* a refuted accusation forfeits the accuser's exposure (02 BOND-R6); sessions per claim
  are bounded (05 COURT-R5); the hold ends by itself after `d_dispute`. The owner must confirm the
  price (OQ-24). *Invariants:* INV-FORK-08, INV-FINAL-05.
  *Sketch:* one Sybil accuser opens the maximum number of sessions against honest claims. Assert that
  each hold ends within `d_dispute` of weight, that the accuser's loss per refuted session is its
  exposure, and that finality resumes.
* **`zero_weight_lane_hash_tiebreak` in next** (a catalog entry; its next-specific form).
  Heartbeats, round blocks and unverified claims weigh zero in fork choice (FORK-R7 to FORK-R9). Two
  tips whose verified content is equal therefore tie on `(safe, live)`, and the smallest tip id wins
  (FORK-R5). Between two `VerifiedClaim`s, a grinder can flip the selected tip by publishing a
  sibling with a smaller id. That orphans honest transactions that no verified weight yet buries.
  The flip is not a safety break, because confirmations read only the finalized anchor (FINAL-R1),
  but it hurts liveness and gives an extra tie-break window during a halt.
  *Defence:* OQ-12. *Invariants:* INV-FORK-02, INV-FORK-03.
  *Sketch:* let honest tip `T` and attacker sibling `T'` differ only by heartbeats and transactions,
  and grind `T'`'s id. Assert that the rule chosen under OQ-12 keeps `T` whenever `T` extends the
  previous selection's verified content, or record the flip rate as a measured bound.

## 7. Deferred: node, network and EVM

The first edition does not model these. They are kept in [appendix A](appendix-a-attack-catalog.md)
§3.9–§3.11, and each will get a regression test in the milestone that owns it. Where the consensus
crate has a hook, that hook is named.

| name | milestone | consensus hook |
| --- | --- | --- |
| `unauthenticated_material_gossip_amplification` | network | — |
| `uncached_state_materialization_per_request` | node / rpc | — |
| `handshake_and_quarantine_abuse` | network | — |
| `gossip_prealloc_abort` | network | — |
| `header_buys_inference_before_parent_check` | node | POL-R16 (no runtime in consensus) |
| `unknown_op_drops_connection` | rpc | — |
| `panel_mempool_accept_treated_as_landed` | node | — |
| `licence_assembler_stall` (alias `licence_stall_greedy_assembler`) | node | INV-PANEL-07: `select_licence` is exported by `pol/panel` |
| `receipt_pool_flush` | node | — |
| `honest_node_resource_exhaustion` | node | — |
| `forged_filing_poisons_node_cache` (alias `decoy_tag57_cache_poisoning`) | node | COURT-R2: the fold's checks are exported for the node |
| `panel_arity_mismatch_defaults_honest` (alias `panel_root_claim_arity_mismatch`) | node | INV-PANEL-10: `court_shape` is exported once (PANEL-R22) |
| `honest_node_misconfiguration_traps` | node | — |
| `bridge_withdrawal_exceeds_backing` | EVM | ECON-R1 (payouts backed by recorded locks) |
| `market_carrier_value_loss` | EVM / model market | ECON-R6 |
| `model_sell_bearer_signature` | model market | signature domain (`cross_network_signature_replay`) |
| `vlt_committee_attacks` | retired (ADR-0134) | no committee in next |

## 8. Names

Every name above is permanent. A name that a chapter coined for an attack the catalog already had
is an alias, and appendix A §3.13.1 maps each alias to its permanent name. The permanent name is the
test name: `fn <name>()` in `tests/adversarial/<class>.rs`.
