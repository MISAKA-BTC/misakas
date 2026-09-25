# 00 — Overview

*The MISAKA Consensus Book is the normative specification of misaka-next, a clean-room reference
implementation of Misaka's Proof-of-LLM (PoL) consensus. A third party should be able to trace
PoL's safety argument from specification to invariant to implementation to test vector. This
chapter is the map. Chapters 01–10 are the territory.*

## 1. What Proof-of-LLM is

A PoL chain orders its history, and pays its producers, for **language-model inference**. That
replaces hashing. A producer runs a registered model on a job the chain names. It commits to the
result's roots and uses that commitment as a lottery ticket. If the ticket wins, it publishes a
**claim**. The claim asserts that it did the work and asks for the claim's weight and reward.

Consensus nodes do not run the model. What a node can check is a hash comparison and a signature.
So PoL splits into two halves that must never be confused.

* **The lottery.** It decides who may claim, and at what rate. It is priced so that an honest
  producer pays one inference per draw. It is *not* evidence that the model ran: a fabricator who
  writes arbitrary roots wins the same lottery for a few hashes per try (04 §2.4).
* **Verification.** A panel of seats, drawn by stake and bonded with collateral, replays the work.
  Only after at least two independent replays of every segment (`basis_k ≥ 2`, counted over seat
  indices) does the claim get the authority verified computation deserves. A court settles disagreements by arithmetic, not by
  vote, and a conviction takes collateral.

Everything a claim may do before verification must be affordable to lose to a fabricator. A claim
earns chain weight only once verified and buried under later verified work with no unreleased
dispute. It becomes irreversible only below the node's **finalized anchor**. Its reward vests until
nobody can convict it any more and the chain has finalized its history.
The economic bar that holds this together is INV-ECON-01. At every moment, the value an attacker has
already extracted must not exceed the value consensus still holds against it.

The frozen reference is testnet-12 (t12, `rcore/int-3` @ `a0af3c92`, [PROVENANCE.md](../../PROVENANCE.md)).
misaka-next is **not** compatible with t12. It takes t12's specification, test vectors and attack
history, and rebuilds the core around three disciplines:

* consensus functions are pure;
* wrong code should not compile (distinct clocks and claim states are distinct types);
* every safety property has an ID and a test with the same ID.

## 2. The pipeline

PoL establishes five facts, and each is a different type and a different state transition.

```text
  (a) the LLM was executed            Execution — off-chain; consensus never observes it
        │ commit roots + job identity
        ▼
  (b) a commitment is held            ExecutionCommitment — bytes in a block (04 §2.3)
        │ ticket = H(commitment)
        ▼
  (c) a lottery was won               LotteryWin — ticket ≤ carried target (header stage, 11 BLK-R2),
        │                                            = target in force at the block's SafeDaa (04 POL-R6)
        │ admit(): three gates, all at one chain point (03 CLAIM-R2)
        │   ├─ economic validity:  account active and signing, exposure ≤ ρ·posted (02), bucket token (03 §2.6)
        │   ├─ lottery eligibility: class admits (05 §2.4), ticket under the target in force (04)
        │   └─ one execution, one claim; every priced field pinned (04 POL-R3, POL-R8)
        ▼
      UnverifiedClaim                 no weight, seed, rights or mint; no block carries header work (03 §2.5)
        │ the chain binds the panel when the claim's seed ring is fixed (04 POL-R9, 05 PANEL-R11)
        │ → receipts → check_licence → LicenceProof → verify (05 PANEL-R14, 03 §4.1)
        ▼
  (d) the panel verified it           VerifiedClaim — basis_k ≥ 2 over seat indices, ≥ q Valid, locks posted
        │ ├─ burial: safe weight once buried by d_bury with no unreleased dispute (06 FORK-R4; clock-free)
        │ └─ mature(): Deadline<Safe> challenge window elapsed, no open court (05 PANEL-R20)
        ▼
  (e) consensus effect                safe weight (06) · FinalClaim: vesting row (02 BOND-R12),
                                      seed leaf (04 POL-R9), rights (03 CLAIM-R14)
        │ k_final safe anchors + d_final verified weight on the chain (07 FINAL-R2)
        ▼
      finalized                       chain anchor (per chain, reverts with it) → node's anchor: never reverts (07 FINAL-R5)
```

The three admission gates are deliberately separate. **Economic validity** is about collateral and
rate. **Lottery eligibility** is about price. **Panel verification** is about truth. None of the
three is evidence for another.

## 3. The three clocks

Chapter 01 owns the precise definitions, and its §2.7 is the only statement of which clock a rule
may read. The three chain clocks are one generic type, `Daa<C>`, with a private field and exactly one
mint per kind. There is no `From` between kinds and no `Daa<C> + DaaSpan`: the only window
constructor is `Deadline::<C>::after(mark: LocalDaa, span)`. The node's anchor DAA is a fourth,
separate type that no state rule may read.

| Clock | Meaning | Minted by | May decide (01 §2.7) |
| --- | --- | --- | --- |
| `LocalDaa` | clock slots (120 s each) claimed along this branch's selected chain; committed in each header | `clock_step` (01 DAA-R1) | uncharged liveness voids of one's own pending objects; header-shape rules; nothing a private branch could profit from |
| `SafeDaa` | `max(ChainFinalizedDaa, LocalDaa of the D_SAFE-th most recent licence)`, read at the parent: time that collateral-backed panels have vouched for; copies `LocalDaa`'s spacing (01 S5) | `safe_daa` only (01 DAA-R11) | `Final` and the redraw, absence convictions, the close of conviction windows, eligibility refills and epochs |
| `ChainFinalizedDaa` | the `LocalDaa` of the chain's own finalized anchor, read at the parent; per chain, reverts with it | `chain_finalized_daa` (07 §2.2) | value release: vesting, exit, seat-lock expiry, right and coinbase maturity, registration maturity, DA defaults; floor of `SafeDaa` |
| `FinalizedDaa` (node) | the `LocalDaa` of the node's finalized anchor; never reverts on that node | `FinalizedAnchor::daa` (07 §2.2) | **no state rule**; confirmations, admissibility (06 FORK-R1), the node's own retention |

The one-line reason for three clocks: *"time passed on this branch"* and *"the honest network had a
chance to act while it passed"* are different facts. A private branch makes the first true almost for
free. Only the second protects a rule whose purpose is to give honest parties an opportunity (01 §1).
Every deadline is marked at its creating block's `LocalDaa` and judged by the clock its kind names
(`Deadline<C>`), so no window is shorter than its span (INV-TIME-08). Every rule in a block reads one
`ClockContext` fixed by the parent (01 DAA-R18, 11 §4). **Fork choice and finality read none of
these clocks** (06 FORK-R8, 07 FINAL-R2). `BlueScore` is structural depth, not time.

## 4. Claim typestate and authority

The typestate (defined once, in 03 §2.3; 05 §2.2 refers to it):

```text
LotteryWin ─admit→ UnverifiedClaim ─verify(LicenceProof) [basis_k ≥ 2]→ VerifiedClaim ─mature [Deadline<Safe>]→ FinalClaim
                    (AwaitingRing │ PanelBound │ Optimistic)                                                    │
                          │ expire/void                  │ convict                                                  │ convict (money only)
                          ▼                              ▼                                                          ▼
                     VoidedClaim                    ConvictedClaim                                             ConvictedClaim
```

**The single statement of what each type may do is `03-claims.md` §2.5.** Every other table that
touches a claim's authority is a projection of it, and it MUST agree: the stake projection
(02 §2.4), the panel's (05 §2.3), fork choice's (06 §2.4) and the finality ladder (07 §2.1). In
summary (03 governs):

| | LotteryWin | UnverifiedClaim | VerifiedClaim | FinalClaim | Voided / Convicted |
| --- | --- | --- | --- | --- | --- |
| header work | none; a lost ticket is not a valid block | none | none | none | none |
| live weight | 0 | 0 by default (`β_u`, OQ-2) | `β·w` until safe | `β·w` until safe | 0, unless already safe |
| safe weight | 0 | 0 | `w` once safe: buried by `d_bury`, no unreleased dispute | the same; `Final` does not enter | 0 if convicted before safe; a conviction after keeps history, takes money |
| safe clock | — | — | its licence feeds `SafeDaa` | — | — |
| seed | none | none | none | one leaf of a post-acceptance ring | none |
| controller input | none | none | counted | counted | none |
| escrow | — | withheld | withheld (reservation may drop) | vesting row, released on `ChainFinalizedDaa` | never minted; row burned |
| rights | none | none | none | quanta, lane credit, probe credit | revoked |

## 5. Fork choice

06 owns fork choice. The owner's seed order and what it became:

| Seed step | misaka-next (06 §3.5) |
| --- | --- |
| `compare_final_anchor` | `admit`: a candidate without the finalized anchor is never compared (FORK-R1) |
| `compute_common_safe_daa` | **dropped.** The context is the finalized anchor itself (FORK-R3). A pairwise minimum is intransitive, and a set-wide or contender minimum is dragged by one cheap candidate (06 §3.2). |
| `compare_safe_frontier` + `compare_safe_weight` | one key, `safe`: the weight of verified claims buried by `d_bury ≥ ρ·W_open + b` of later verified weight with no unreleased dispute; clock-free, and `Final` does not enter (FORK-R4, FORK-R6) |
| `compare_live_weight` | `live = safe + ⌊β·(V − safe)⌋` over verified claims that are safe or not voided; unverified claims contribute 0 (FORK-R7) |
| `deterministic_tiebreak` | the tip's `BlockId`, smaller wins (FORK-R5) |

```rust
pub fn compare_chains(a: &Admissible<'_>, b: &Admissible<'_>, ctx: &SafeContext, p: &ForkParams) -> Ordering;
// SafeContext { anchor: FinalizedAnchor } — one per selection, no clock reading.
```

The order has these properties, each tested:

* symmetry, totality and transitivity (INV-FORK-02);
* determinism and path independence, with no incumbent and no arrival order (INV-FORK-03);
* independence from other candidates (INV-FORK-04);
* private-DAA invariance, exact: a clock-raced branch with the same content never has larger keys
  (INV-FORK-01);
* monotone maturing (INV-FORK-08);
* invariance when the anchor moves (INV-FORK-09).

Every chain-selection site uses this one function, including the DAG's selected parent (FORK-R10).

## 6. Finality

Chapter 07 separates t12's meanings of "final" into a ladder:

```text
accepted → verified → safe (buried, undisputed; clock-free) → chain-finalized (per chain)
         → finalized (the node's anchor; never reverts) → pruned
            verified → settled (FinalClaim; SafeDaa-timed; money, seeds, rights — never weight)
```

A chain's anchor advances only by `k_final` safe anchors plus `d_final` of verified weight
(FINAL-R2). No clock enters, directly or through `Final`. A block's `ChainFinalizedDaa` is its
parent's chain anchor, so every node computes it identically. The node's anchor is the deepest
chain anchor it has selected (FINAL-R4). Nothing times into or out of finality (FINAL-R8). A
conflicting anchor is reported, never resolved by a rule (FINAL-R9). Pruning stops at the anchor's
own claim frontier, below which every claim is resolved (FINAL-R10). A joining node needs a recent
trust root (FINAL-R11).

## 7. Crate map

Consensus crates are pure. They hold no store, network, RPC or wall-clock handle. Where one crate
must mint a type another owns, the minting crate holds the only capability to do so, and a
crate-graph test asserts nothing else imports it. Only `consensus/finality` mints
`ChainFinalizedDaa` and `FinalizedDaa`; only `consensus/claims` mints `SeedLeaf`; only
`pol/verification` mints `LicenceProof`. The `pol/*` crates are upstream of `consensus/claims`, so
they take and return plain records (`ClaimRecord`, `BoundRecord`) and proof tokens, never a claim
typestate. Only `consensus/claims` constructs typestates. No state crate may import the node's
`FinalizedDaa`.

| Crate | Chapter(s) | Owns |
| --- | --- | --- |
| `crates/primitives` | 01, 02 | `Hash64`, `BlockId`, `Sompi`, `Permille`, `Ratio`, `Timestamp`, `SlotIndex`, `DaaSpan`, `BlueScore` (no cross-unit operators) |
| `crates/crypto` | all | keyed BLAKE2b domains; ML-DSA-87 verification; one signature context per object family, bound to network and genesis |
| `consensus/daa` | 01 | `Daa<C>` (`Local`, `Safe`, `ChainFinal`), `Deadline<C>`, `ClockState`, `clock_step`, `check_header_clock`, `LicenceRing`, `safe_daa`, `ClockContext`, `epoch_of` |
| `consensus/validation` | 11, 01, 04, 08 | `check_header` (the lost-ticket check, BLK-R2), `select_parent_and_mergeset`, `apply_block` (the ordered transition, 11 §4), subsidy and coinbase (ECON-R2, ECON-R3, ECON-R16), the supply ledger and minted counter (ECON-R6, ECON-R17) |
| `consensus/bonds` | 02, 08 | `StakeAccount`, rooms, `draw_seats` (BOND-R3), duty/lock/tier, `forfeit`, `apply_slash`, `exit_permitted`, `vesting_releasable`; `econ::{Frozen, Extracted, econ_margin, admit_class_econ, final_split, reporter_reward}` |
| `consensus/claims` | 03 | the typestates (the only constructor), `authority.rs` (the 03 §2.5 table in code), `ClaimBucket`, `admit`, `record`, `bind`, `verify` (from a `LicenceProof`), `mature`, `expire`, `convict`, `seed_leaf`; the only minter of `Weight` |
| `consensus/fork_choice` | 06 | `ChainView`, `Admissible`, `SafeContext`, `fork_weights`, `fork_key`, `compare_chains`, `select_tip` |
| `consensus/finality` | 07 | `ChainAnchor`, `FinalizedAnchor`, `is_finalizable`, `block_finalized_anchor`, `chain_finalized_daa`, `advance_finality`, `pruning_point` |
| `pol/commitment` | 04 §2.2–2.3 | `template_id`, `bucket_of`, `execution_anchor`, `job_for_anchor`, `execution_commitment`, `check_pins` |
| `pol/lottery` | 04 §2.4, §2.6 | `ticket`, `ticket_target`, `draw`, `derive_work`, `step_work_target` (clamped to `[W₀, W_max]`), `target_in_force`; `SeedRing`, `seed_ring`, `panel_seed`, `quantum_seed` |
| `pol/panel` | 05 §3.1–3.4 | class lifecycle, `jury`, readiness, `derive_panel(&ClaimRecord, ..)`, `segment_assignment`, `check_receipt`, `basis_k` (over seat indices), `select_licence`, `court_shape`, `turn_deadline` |
| `pol/verification` | 05 §3.3, §3.6 | `check_licence` (returns the `LicenceProof` that `consensus/claims::verify` needs), `adjudicate`, fraud proofs, DA units |
| later | — | `state`, `storage`, `network`, `node`, `rpc`: started only after every attack in 10 is reproduced (§12) |

The dependencies run in one direction: `primitives` ← `crypto` ← `consensus/daa` ← `pol/commitment` ←
`pol/lottery` ← `consensus/bonds` ← `pol/panel` ← `pol/verification` ← `consensus/claims` ←
`consensus/fork_choice` ← `consensus/finality` ← `consensus/validation`.

## 8. How to read the book

| Chapter | Question it answers |
| --- | --- |
| [01 Time and the DAA score](01-time-and-daa.md) | what time is, and which clock each rule may read |
| [02 Bonds](02-bonds.md) | what stake buys, how much a claim or seat ties up, how it is slashed and returned |
| [03 Claims](03-claims.md) | a claim's states, their authority, and issuance capacity |
| [04 Proof of LLM](04-proof-of-llm.md) | from an execution to a lottery win: what is committed, priced and seeded |
| [05 Panel validation](05-panel-validation.md) | from `UnverifiedClaim` to `VerifiedClaim`, or to a conviction |
| [06 Fork choice](06-fork-choice.md) | which chain is the chain |
| [07 Finality](07-finality.md) | what never reverts, and what may be pruned |
| [08 Economics](08-economics.md) | supply, issuance, rewards, and the design bar INV-ECON-01 |
| [09 Invariants](09-invariants.md) | the canonical list, with t12 status and test names |
| [10 Attack model](10-attack-model.md) | the adversary, and every attack with its regression sketch |
| [11 Block and transition](11-block-and-transition.md) | what a block is, which blocks can be on a chain, and the one ordered transition every rule runs inside |
| [Appendix A](appendix-a-attack-catalog.md) | the historical attack record, with sources |

Every chapter has the same skeleton:

1. Purpose.
2. Concepts and types.
3. Normative rules (`AREA-Rn`, each with a *because*).
4. Pure functions.
5. Invariants upheld.
6. t12 reference.
7. Attacks.
8. Open questions.

Read chapter 01 first, then 03 and 04 (what a claim is). Then read 05, 06 and 07 in that order
(how it becomes weight, then history). Chapters 02 and 08 cover the money. Chapter 11 composes all of
it into one block transition. Keep 09 and 10 open throughout.

**Conventions.**

* RFC 2119 keywords are normative.
* A citation `path:line` is at `a0af3c92` unless it says otherwise.
* *pending* marks a change on a t12 branch not merged into the reference.
* `[unverified]` marks a claim about t12 that nobody checked in code.
* *[Synthesis edit]* marks text the consistency pass changed (§9).
* Rule IDs are unique across the book. Chapter-local open questions are cited as `NN Qk`, and the
  consolidated ones as `OQ-n` (§10).

## 9. Reconciliations made in synthesis

The chapters were written in parallel. The consistency pass resolved these contradictions and
recorded each edit in place.

| # | Conflict | Resolution | Edited |
| --- | --- | --- | --- |
| R1 | 01 defined `SafeContext` as the set-wide minimum `SafeDaa` (`common_safe_daa`). 06 showed every common clock reading fails. | `SafeContext` is 06's `{ anchor }`, and fork choice reads no clock. 01's per-block readings are renamed `ClockContext`, and `common_safe_daa` is a diagnostic only. | 01 §2.5–2.7, DAA-R14, §4 |
| R2 | 06 defined `SafeDaa` as the `LocalDaa` of the safe-frontier block. 01 mints it from licences, because `Final` reads `SafeDaa` and a frontier made of settled claims would be circular. | 01's definition stands. | 06 §2.2, §3.3 |
| R3 | 05 made every panel deadline `LocalDaa`, with `mature(now: LocalDaa)`. 01 and 03 require `SafeDaa` for anything that charges or grants. | Only the uncharged bind is `Deadline<Local>`. Receipt, challenge, turn and disclose deadlines are `Deadline<Safe>`, and `mature` takes `SafeDaa`. | 05 PANEL-R20, PANEL-R21, §4 |
| R4 | 03 measured windows from `accepted_safe`. 01 marks deadlines at `LocalDaa`. | `LocalDaa` marks. With a lagging `SafeDaa`, a window measured from `accepted_safe` closes almost at once (INV-TIME-08, `safe_mark_window_collapse`). | 03 §2.2–2.3, CLAIM-R9, CLAIM-R13, §4.1 |
| R5 | 03 gave `UnverifiedClaim` live weight `β_u·w` and `FinalClaim` immediate safe weight. 06 gives unverified claims 0, and safe weight only once buried. | 06 governs the weight formula. `β_u = 0` by default, with 03's alternative kept as OQ-2. Safe weight needs burial. | 03 §2.5 A2–A3, CLAIM-R4, CLAIM-R5, §5, Q2 |
| R6 | 05 seeded panels from the anchor's execution key (PANEL-R7). 04 POL-R9 and 03 A7/CLAIM-R12 forbid seeds from unverified commitments. | POL-R9 is the normative source. The execution key is OQ-1 option (a). POL-R9 still lacks a bootstrap rule (INV-PANEL-12). | 05 PANEL-R3, PANEL-R7, §4; 04 Q2 |
| R7 | 05 kept "one seat per operator, capped weight". 02 BOND-R3 draws with replacement and uncapped. | BOND-R3 stands, and 02 Q2-1 / OQ-6 stays open. INV-PANEL-03 drops "no operator holds two seats". | 05 §2.5, PANEL-R9, PANEL-R10, PANEL-R16, INV-PANEL-03 |
| R8 | 05 cited wrong chapter numbers (fork choice as 07, economics as 06). | Fixed. | 05 §2.1, §2.3, §6.1, COURT-R10, §7 |
| R9 | Two or three authority tables claimed to be "the one place". | 03 §2.5 is the single source. 05 §2.3 and 06 §2.4 are labelled projections. | 05, 06 |
| R10 | One ID had different test names in different chapters (INV-FORK-01, INV-BOND-01, INV-ECON-01), and INV-CLAIM-06 duplicated INV-BOND-03 and INV-BOND-06. | One name per ID, and INV-CLAIM-06 is retired (09 §4). | 01, 03 |
| R11 | Chapters coined 28 synonyms for catalog attacks. | The catalog names are permanent, and the synonyms are aliases (A §3.13.1). | 01, 02, 04, 05, 06, 07, 08 |
| R12 | The catalog status contradicted chapter code-reads for `panel_draw_seed_grind`, `bond_split_amplification`, `failed_lottery_blue_weight`, `bondless_attempt_row_grind`, `w_controller_counts_nonfinal_blocks` and `selection_sites_disagree`. | The code was re-checked, and the corrections are recorded in A §3.13.2. | appendix A |
| R13 | 06 W1 bounded admission by per-class epoch budgets, which 03 removed. | W1 now names 03's lane bucket, with an additive `B` term that FORK-R16's calibration must include. | 06 §2.2 |
| R14 | 03 CLAIM-R4 required `β < 1`, while 06 FORK-R16 allows `β ≤ 1`. | `β_v ≤ 1`, `β_u < β_v`. | 03 CLAIM-R4; INV-CLAIM-07 |
| R15 | 02 BOND-R12 releases rewards on `FinalizedDaa`, while 07 Q3 recommended release at `Final`. | Left open as OQ-11 (BOND-R12 is option (b)). | 07 Q3 |

## 10. Open questions for the project owner (consolidated)

The chapters raised 45 questions. They are merged below by the decision they need. Each gives its
options and a recommendation, and cites the chapter questions it absorbs. The owner's answer fixes
a rule, and the book changes in one place. Where a chapter and this list disagree, this list states
the synthesis's recommendation and the reason.

**OQ-1. What seeds panels, juries and quanta, and how do seeds start?** (04 Q2, 05 Q1, 05 Q4 seed
half.)
* *Options.*
  * (a) The anchor attempt's nonce-free execution key (05 PANEL-R7's proposal). It is immediate,
    but its producer can re-roll it on the root axis for hashes, paying one forfeit per published
    anchor.
  * (b) A ring of `K` `FinalClaim` leaves from distinct accounts, fixed at a `SafeDaa` after the
    seeded object (04 POL-R9). It cannot be re-rolled below one verified execution, but it has no
    bootstrap: no ring exists at genesis or after a stretch with no `Final`.
  * (c) (a) for the first panel and (b) for the redraw.
* *Recommendation.* (b), with `K = 4` and a one-unit lag, **plus** an explicit bootstrap and halt
  rule. One such rule falls back to (a) while fewer than `K` post-claim leaves exist, and prices
  that window. The simulator must show INV-PANEL-12 before (b) is final. The block hash goes under
  every option.

**OQ-2. Live weight of unverified claims, and `β`.** (03 Q2, 06 Q1.)
* *Options.* `β_u = 0` (06), `β_u < β_v` (03 recommended 50‰ and 100‰), or one `β`. For verified
  work, `β = 1`, `1/2` or → 0.
* *Recommendation.* `β_u = 0` and `β_v = 1/2`. A fabricated root costs only hashes (04 §2.4), so any
  `β_u > 0` sells live weight for hashes. The cost is that fresh tips carrying only unverified claims
  tie, which is OQ-12.

**OQ-3. Deadline marks.** (Synthesis; see R4.)
* *Options.* Mark at the creating block's `LocalDaa` (01), or at its `SafeDaa` (03's draft).
* *Recommendation.* `LocalDaa` marks, as now written. A window then lasts at least its span in local
  terms plus the licence lag (INV-TIME-08).

**OQ-4. What happens in a licence halt?** (01 Q2, 02 Q2-4, 03 Q1, 07 Q5.)
* *Background.* `SafeDaa` stops, and so does `FinalizedDaa`: no settled anchors means no finality.
* *Options.*
  * (a) Freeze every value-releasing rule. Allow only uncharged voids and reservation releases on
    `Deadline<Local>`.
  * (b) 02's exit escape measured in `FinalizedDaa`. It never fires during a halt, because
    `FinalizedDaa` stops too.
  * (c) An exit escape measured in `LocalDaa`.
  * (d) t12's timed escape on everything.
* *Recommendation.* (a), with operator-level recovery (a trust-root / ruleset fence) for halts longer
  than a stated bound. Reject (c) and (d): during a halt every candidate has equal verified weight,
  so the tip id decides, and a private heartbeat-only branch that ran the escape can win that tie
  (`licence_halt_stake_freeze`). Retune `D_SAFE` from measured licence cadence.

**OQ-5. Does any common clock reading survive in fork choice?** (06 Q4; the seed's
`compute_common_safe_daa`.)
* *Recommendation.* No (06 §3.2). The context is the anchor. Confirm.

**OQ-6. How are panel seats drawn?** (02 Q2-1, 02 Q2-3.)
* *Options.* With replacement, uncapped (02). t12's race without replacement, one seat per operator,
  capped. Without replacement, but proportional by systematic sampling.
* *Recommendation.* With replacement, which makes INV-BOND-02 hold by construction. Also accept its
  consequence: one account may fill both attesters of a segment, so `basis_k` counts seat draws, not
  distinct accounts. The safety law stays `P2 = s²`. Add `MIN_DISTINCT = n` as a liveness floor
  only.

**OQ-7. Who pays when two panels in a row fail silently?** (05 Q3, 02 BOND-R13, 08 §4.3.)
* *Options.*
  * (a) The producer forfeits `w + E + rr` (S0′; t12; 02). This prices fake roots and enables
    `silent_quorum_griefing`.
  * (b) Nobody; void at S0 (05). Griefing becomes free, but so does a fake root that fails silently.
  * (c) (b) plus an expiring availability strike on seats. This conflicts with BOND-R16 (no
    per-account escalation).
* *Recommendation.* Keep (a) until OQ-9 (b) is in force and the simulator shows that honest seats
  convict fabricated roots at the first panel. Then move to (b). Reject (c).

**OQ-8. May the anchor's producer void claims by omitting their `PanelBound`?** (05 Q2.)
* *Recommendation.* No. The fold binds due panels itself, with no object to withhold.

**OQ-9. Is a fabricated root convicted at the first panel?** (04 Q3.)
* *Recommendation.* Yes. An honest seat's `Invalid` on a job-identity mismatch opens a court at
  once, with S0′ as the backstop for silent panels.

**OQ-10. How is the admission jury weighted?** (05 Q4.)
* *Recommendation.* By the same stake draw as seats, seeded per OQ-1.

**OQ-11. Is a reward released at `Final` or below the finalized anchor?** (07 Q3, 02 BOND-R12, 08
ECON-R7.)
* *Recommendation.* Below the finalized anchor, as BOND-R12 already specifies: a row releases when
  `FinalizedDaa` passes its expiry. Every other leg vests with the row (08 Q8-1 (a)). Wallets show
  finalized depth (FINAL-R1).

**OQ-12. How are tips ordered when they differ only by blocks without verified work?** (New; 06 Q7;
`zero_weight_lane_hash_tiebreak`.)
* *Options.*
  * (a) Accept the tip-id flip. Confirmations read only the anchor, so the flip is harmless to
    safety.
  * (b) Add a third key, "verified content of the previous selection is a prefix". This is a form of
    hysteresis that FORK-R11 forbids.
  * (c) A DAG, in which non-weight siblings are merged rather than competing (06 Q5 (b)).
  * (d) Tie-break by `H(domain ‖ anchor ‖ tip)`.
* *Recommendation.* (c), because it removes the competition instead of ordering it. Measure the
  residual flip rate in the simulator before accepting (a) for the rest.

**OQ-13. Is the ticket target verifiable at the header stage?** (04 Q1.)
* *Recommendation.* Both: `ε` header work, and a carried target checked like `bits` then re-derived.

**OQ-14. Is there an external finality overlay in the core?** (06 Q6, 07 Q2.)
* *Recommendation.* None at launch. Later, only an input that proposes a deeper anchor (FINAL-R13).
  Never a veto with a TTL.

**OQ-15. What is the target share `s_target`?** (08 Q8-2.)
* *Recommendation.* 1/3 for every door, monitored on live eligible stake.

**OQ-16. What happens to held and long-context classes, and to the 2M row?** (05 Q6, 08 Q8-6,
PANEL-R5, ECON-R11.)
* *Recommendation.* Refuse weight and reward until every fault class has a carried proof and a
  measured verification row. 2M stays closed.

**OQ-17. Clock parameters.** (01 Q1, 01 Q3, 01 Q4, 01 Q6.)
* *Recommendation.*
  * Heartbeat and attempt blocks claim slots.
  * `SLOT_MS = 120,000` and `DRIFT_MS = 60,000`.
  * Launch with no scheduled fences; a later fence activates on `FinalizedDaa`.
  * Keep heartbeats, with exactly DAA-R9's authority.

**OQ-18. DAG shape and depths.** (01 Q5, 06 Q5.)
* *Recommendation.* A DAG whose selected parent is `select_tip` over the parents. Retention is by
  `FinalizedDaa` plus the claim lattice. No blue-score depth survives.

**OQ-19. Lottery controller and capacity.** (03 Q3, 03 Q4, 04 Q4.)
* *Recommendation.*
  * The controller counts replay-licensed claims.
  * Each lane's bucket rate is its target cadence per `SafeDaa`, with `B` one verification window of
    that rate.
  * The liveness floor gets its own bucket.
  * Classes with `CCU ≥ W` are allowed only with the bucket in force.

**OQ-20. May a conviction after `Final` remove safe weight?** (03 Q5.)
* *Recommendation.* No. Money is recovered and history stays (05 COURT-R10, 06 W2).

**OQ-21. Is the optimistic (S2) licence a consensus object?** (05 Q5.)
* *Recommendation.* Drop it; nodes track the progress locally.

**OQ-22. Stake parameters.** (02 Q2-2, 02 Q2-5, 02 Q2-6.)
* *Recommendation.*
  * Remove the per-account in-flight share.
  * Tiers are `m·G`, with class floors `max(F_role, 6G, 2(w+E+rr))`.
  * Slash posted stake before unbonding stake, oldest first.

**OQ-23. Economic parameters.** (08 Q8-1, 08 Q8-3, 08 Q8-4, 08 Q8-5.)
* *Recommendation.*
  * Every leg vests.
  * Action tiers never count as recovery.
  * One rooted supply ledger.
  * The panel pool share is derived per class, with the floor class at 200‰ until measured.

**OQ-24. Fork parameters.** (06 Q2, 06 Q3, 06 Q7.)
* *Recommendation.*
  * `d_bury` is a weight calibrated by FORK-R16, including the bucket term `B` (R13).
  * Disputes also time out on the work clock.
  * The raw tip id is the tie-break (subject to OQ-12).

**OQ-25. Finality parameters and behaviour.** (07 Q1, 07 Q4, 07 Q5.)
* *Recommendation.*
  * `k_final` and `d_final` are derived from INV-ECON-01 and fixed at genesis from a drill.
  * On a `FinalityConflict`, keep the chain and alert. Halt the node's own production.
  * A finality stall is accepted and alerted (see OQ-4).

## 11. Scope of the first edition

**In.** The PoL consensus core:

* clocks (01);
* stake, commitments, slashing, exit and vesting (02);
* the claim lattice (03);
* commitment, lottery and seeds (04);
* panels, courts and DA (05);
* fork choice (06) and finality (07);
* supply and the design bar (08);
* invariants (09) and the attack model (10).

**Out, for now:**

* **The EVM lane and bridge ledger.** Their consensus contact is a supply rule: payouts must be
  backed by recorded locks (ECON-R1). Everything else is execution semantics a later milestone owns.
* **Model market UX.** Only its supply effects are consensus (`unbound_model_sink_output_burn`,
  ECON-R6). Pricing and positions are application logic.
* **Node policy.** Material fetching, receipt pools, filers and auto-answers make the consensus
  checks succeed, but they are not checks themselves (05 §3.8). Their regressions are deferred (10
  §7).
* **P2P.** Gossip, IBD transport and handshakes carry consensus objects but decide nothing. What
  consensus needs from them is stated as an assumption (10 §2.2 H4, H5).
* **Wallet.** Keys and transaction construction sit outside consensus. Confirmations read the
  finalized anchor (FINAL-R1).

## 12. The executable-spec plan

misaka-next is built in this order. No later stage starts before the one before it passes.

1. **Primitives.** `crates/primitives` and `crates/crypto`, with the type discipline of §3 and §4
   proven by compile-fail tests (INV-TIME-02, INV-CLAIM-03, INV-POL-01).
2. **Consensus model.** `consensus/*` and `pol/*` as pure functions over in-memory state, one module
   per chapter. Every `inv_*` test from 09 runs against it as a unit, property or vector test.
3. **Adversarial simulator.** A deterministic, seedable model of honest and adversarial parties with
   the capabilities of 10 §2 (`h`, `s`, `c`, `g`, private branches, `Δ`). It drives the consensus
   model and nothing else.
4. **Consensus tests.** Every attack in 10 §4–§6 is reproduced as `tests/adversarial/<class>.rs::
   <name>`, and the model must refuse it or hold it inside its stated bound. Every t12 test vector
   the book cites is ported (`tests/consensus-vectors`), and a differential harness replays t12
   fixtures where the rules coincide (`tests/differential`).
5. **Only then** come `state`, `storage`, `network`, `node` and `rpc`, with the deferred attacks of
   10 §7 as their first regressions.

## 13. Glossary

* **anchor (panel).** The attempt block at or past `bind_base + anchor_delay`, whose state fixes a
  claim's panel population (05 PANEL-R7, PANEL-R8).
* **anchor (finalized).** The node's irreversible boundary, `FinalizedAnchor` (07).
* **attempt.** A block carrying a lottery ticket for a model class.
* **basis_k.** The minimum, over a job's segments, of distinct counted `Valid` attesters, capped at
  3. It must be ≥ 2 for a `VerifiedClaim` (05).
* **B.** A lane bucket's capacity; unused issuance capacity never exceeds it (INV-CLAIM-02).
* **bucket (nonce).** The high bits of the nonce; one bucket is one execution (04 §2.2).
* **burial.** `d_bury` of verified weight accepted after a claim on the same chain. Maturity for fork
  choice (06 FORK-R4).
* **class.** A registered model plus its canonical job, identified by its derived profile (05 §2.4).
* **conviction horizon.** The last point at which a claim can still be convicted. Vesting rows and
  locks live until it.
* **court.** An adjudication of one claim's committed execution by arithmetic (05 §2.6).
* **`d_bury`, `d_final`, `k_final`.** The burial weight; the finality weight; the count of settled
  anchors (06, 07).
* **Deadline<C>.** A `LocalDaa` mark plus a span, elapsed only when clock `C` passes it (01 §2.6).
* **ε.** The minimum header work of a lane; identical for won and lost tickets (03 CLAIM-R3).
* **escrow `E`.** The PoL reward carved from the carrying block's subsidy and withheld at acceptance
  (08 ECON-R3).
* **Frozen / Extracted.** The two sides of INV-ECON-01 (08 §2.4).
* **`G`, `G_res`.** A claim's whole fraud gain `E + w + R + s`, and the part a Final releases
  before its row matures (08 §2.3).
* **heartbeat.** A bondless, fee-only block at a fixed hash price that claims clock slots and
  nothing else (01 DAA-R9).
* **licence.** The `UnverifiedClaim → VerifiedClaim` transition, and the input of the licence ring
  that mints `SafeDaa` (01 §2.3).
* **live / safe weight.** The fork-choice keys (06 §3.3).
* **panel.** `n` seats drawn by stake for one claim. One full seat plus `K = n − 1` segment seats
  (05 §2.5).
* **`s`, `s_target`.** The adversary's share of eligible stake, and the share below which safety is
  claimed (08, OQ-15).
* **S0′, S1–S4.** Forfeit and conviction tiers: S0′ is the second failed panel; S2 is proven fraud
  before Final; S3 is after Final; S4 is a false `Valid` (02 BOND-R13).
* **settled anchor.** A chain block whose attempt claim is `Final`; the unit of finality depth (07
  §2.3).
* **SeedRing.** The last `K` `FinalClaim` leaves as of a `SafeDaa`; the permitted source of consensus
  randomness (04 POL-R9).
* **t12.** testnet-12, the frozen reference (`rcore/int-3` @ `a0af3c92`).
* **vesting row.** A `Final` claim's reward legs, payees fixed, burnable until released on
  `FinalizedDaa` (02 BOND-R12).
* **`W`, `W₀`.** The network work target and its floor; a class's ticket probability is
  `min(1, CCU/W)` (04 §2.4).
* **work clock `Ω`.** The cumulative verified weight of a chain from genesis (06 §3.3).
