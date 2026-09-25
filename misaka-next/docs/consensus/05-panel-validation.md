# 05 — Panel validation: from UnverifiedClaim to VerifiedClaim (or a conviction)

Normative keywords are RFC 2119. A citation `path:line` is at the t12 reference `a0af3c92`
(see [PROVENANCE.md](../../PROVENANCE.md)); one labelled *pending* names a branch that is not in the
reference. `[unverified]` marks a statement this chapter did not check against code.

## 1. Purpose

A PoL block carries a claim that its producer ran a model on a job, and a lottery ticket derived
from the commitment to that run (chapter 04). Winning the lottery proves the producer *committed*
to something. It does not prove the model was executed, or executed correctly: the commitment is a
hash the producer wrote. The panel is the part of consensus that turns "someone committed to an
execution" into "independent, bonded parties re-executed it and put collateral behind the answer",
and the court is the part that settles a disagreement by arithmetic instead of by a vote.

This chapter specifies which models may be judged at all (class admission), who judges a claim
(the draw and its seed), what a judge attests (receipts), which set of attestations makes a claim a
`VerifiedClaim`, what happens when someone says the attestations are wrong (courts, fraud proofs,
data availability), how verdicts become slashes, and on which clock each deadline runs. It ends by
separating the consensus half (what every node checks) from node policy (what an honest node does
so that the checks can succeed). misaka-next's `pol/panel` crate holds only the consensus half.

## 2. Concepts and types

### 2.1 The five facts, and the one this chapter produces

| Fact | Where established | Type after it |
|---|---|---|
| (a) the LLM was executed | never observable; only *re-executed* by a seat | — |
| (b) a commitment/root is held | block body, chapter 04 | part of `UnverifiedClaim` |
| (c) a lottery was won | header validity, chapter 04 | `UnverifiedClaim` is created |
| (d) the claim was verified by the panel | **this chapter** | `VerifiedClaim` |
| (e) the claim earns chain weight / effect | maturity here; weight in chapter 06 | `FinalClaim` |

A panel never establishes (a). It establishes that at least `K_final` independent re-executions of
every segment agreed with the committed trace, under collateral large enough that a colluding
quorum loses more than it gains when any honest party finds the lie (§3.3, INV-ECON-01).

### 2.2 Typestate

The claim types are defined once, in `03-claims.md` §2.3. This chapter's `BoundClaim` is 03's
`UnverifiedClaim` in its `PanelBound` stage.

```
UnverifiedClaim ──bind (chain-derived, 05 PANEL-R11)──▶ BoundClaim ──check_licence → verify──▶ VerifiedClaim ──mature──▶ FinalClaim
   │ AwaitingRing: no ring by ring_by       │ 2 panels fail / Unavailable quorum      │ proven fraud                     │ proven fraud
   │ or no capable panel at the ring block  │                                        │ (before Final)                   │ (after Final: money only)
   ▼                                        ▼                                        ▼                                  ▼
 Voided{NoRing | NoCapablePanel}          Voided{SecondPanelFailed | …}          ConvictedClaim                    ConvictedClaim
```

`FinalClaim` is not immune. An objective offence proven after `Final` moves it to `ConvictedClaim`
and debits vesting rows and collateral. It removes no weight that is already safe (06 §3.3,
COURT-R10).

`pol/panel` and `pol/verification` are upstream of `consensus/claims` (00 §7), so they never touch a
claim typestate. They read plain records and return proof tokens:

```rust
/// The pol-visible projection of 03's ClaimCore, built by consensus/claims::record.
pub struct ClaimRecord { id: ClaimId, admission: AdmissionIndex, class: ClassId, executor: AccountId,
                         roots: CommittedRoots, job_identity: JobIdentity, accepted_at: LocalDaa }
pub struct BoundRecord { claim: ClaimRecord, panel: Panel, bound_at: LocalDaa }
/// Minted only by check_licence (private constructor); consumed by consensus/claims::verify.
pub struct LicenceProof { claim: ClaimId, panel: PanelId, cert: LicenceCertificate, basis_k: BasisK }
pub struct BasisK(u8);           // min over segments of distinct counted Valid SEAT INDICES, capped at 3
pub struct SeatIndex(u16);       // the draw position i of 02 §4.3; one account may hold several
pub struct PanelSeed(Hash64);    // 04 POL-R9's panel_seed; never a block identity
pub enum DrawIndex { First, Redraw }                 // at most one redraw
pub enum ReceiptVerdict { Valid, Unavailable { unit: DaUnit, requested_at: LocalDaa }, Incapable, Sampled }
pub struct SignedReceipt { claim: ClaimId, panel: PanelId, seat: SeatIndex, verdict: ReceiptVerdict,
                           mask: SegmentMask, signed_at: LocalDaa, signer: AccountKey, sig: MlDsa87Sig }
pub struct LicenceCertificate { receipts: Vec<SignedReceipt> }   // the panel proof
pub enum Verdict { Convicted { target: SlashTarget, basis: ConvictionBasis }, Acquitted, Unadjudicable }
pub enum ConvictionBasis { Proven, Default }          // a Default never convicts a third party
```

`VerifiedClaim` can be built only by `consensus/claims::verify`, which needs a `LicenceProof`, and
only `check_licence` mints one. `FinalClaim` can be built only by `mature` (03 §4.1). Code that holds
an `UnverifiedClaim` cannot call anything that asks for a `VerifiedClaim`.

### 2.3 Authority (the panel's projection of 03 §2.5)

The single statement of each claim type's authority is `03-claims.md` §2.5; this table restates
the part the panel touches and MUST agree with it.

| Type | MAY carry | MUST NOT carry |
|---|---|---|
| `UnverifiedClaim` / `BoundClaim` | what chapter 04 grants a won lottery; a producer reservation; seat duties | safe weight; reward payout; seat pay; eligibility as a seed or permit source |
| `VerifiedClaim` | seat locks; eligibility to mature; staged escrow release (chapter 02 BOND-R5); safe weight once buried and undisputed (06 §3.3) | an irrevocable reward |
| `FinalClaim` | reward vesting start; a seed leaf; rights | release of liability: its vesting rows stay convictable until `conviction_ends` |
| optimistic licence (one full replay) | nothing beyond `BoundClaim` | anything `VerifiedClaim` carries |
| `VoidedClaim` / `ConvictedClaim` | nothing new (a convicted claim keeps weight that was already safe) | — |

### 2.4 Classes

A *class* is a model plus its canonical job, identified by the hash of its derived profile. A class
is a consensus object: it is registered, certified and walked through a lifecycle by chain objects,
never by the binary. Lifecycle: `Candidate → Prefetching → Probation → ActiveLimited → Active`, with
`Held` (admits nothing new) and `Registered` (manifest invalid, never admits). Only `Probation`,
`ActiveLimited` and `Active` admit claims. A class confers fork-choice weight only if it is
certified end to end: every kernel its graph reaches lies in a family whose court can convict a
planted fault (ADR-0069, ADR-0075).

### 2.5 Panels

A panel for claim `c` is `n` seats (t12: 5), drawn by stake over a population fixed before the seed
(t12: a race without replacement, one seat per operator; next: independent draws with replacement,
02 BOND-R3), the executor's bond, operator and key excluded. A seat is a **seat index** `i ∈ 0..n`,
the position of draw `i`; one account may hold several seat indices, and every receipt names the one
it answers for (PANEL-R12). The job's
leaves are cut into `K = n − 1` segments; the seed names one **full seat** (replays the whole job)
and gives each other seat exactly one segment, so the partial seats partition the job. A class
bought by a registrant (not genesis) is **outsider-judged**: its first seat is drawn from the
network's base-class population, not the class's own.

### 2.6 Courts

A *court* adjudicates one claim's committed execution. Kinds: the **bisection ladder** (two parties
narrow a disputed leaf, then one step is recomputed), the **one-move court** (a named leaf is
recomputed at once), the **attention dissection** (a fused attention leaf is split k-ary until a
tile is recomputed), the **checkpoint court** (a committed checkpoint chunk is compared with the
committed row), and **DA sessions** (a party demands a committed unit; silence confirms
withholding). Objective offences (fraud proofs) convict without a session.

### 2.7 Glossary of numbers that share a letter

`n` seats, `q` quorum (`2q > n`), `K = n − 1` segments, `K_final = 2` (the basis a licence needs to
mature), `G` the maximum fraud gain of one claim. ADR-0152's **`m = 3`** is the *action multiple*
(an action-tier slash charges `m × G`, `consensus/core/src/palw_state_v2.rs:949`); the held court's
**`m = 2`** is the *replay margin* in a compute turn (*pending*, `feat/t12-aheld-node`). They are
unrelated; next names them `ACTION_MULTIPLE` and `COMPUTE_TURN_MARGIN`.

## 3. Normative rules

### 3.1 Classes, admission and readiness

**PANEL-R1.** A class MUST enter the chain only through a registration object whose profile every
node derives from the graph; no registrant-declared quantity that a function can compute (leaf
counts, reachable kernels, work) is read. *Because:* a declared cost is a free field
(declared_canonical_job_weight_inflation).

**PANEL-R2.** A class MUST NOT confer safe weight or reward unless every kernel it reaches is in a
family the chain certified by a drill whose planted faults the court convicted. *Because:* weight
for work nobody can convict is weight for free (INV-POL-01).

**PANEL-R3.** A registered class MUST stay `Candidate`, admitting nothing, until an admission jury of
`n` operators drawn from the network's base population (excluding the registrant's bond and
operator, registered before the seed's span) holds the class by a strict majority. The jury MUST be
drawn at most once per audit period and MUST be seeded by `jury_seed` (§4), whose source follows the
panel seed's (PANEL-R7: 04 POL-R9, OQ-1) and never a block identity. *Because:* admission_jury_seed_grind, admission_jury_sybil_capture.

**PANEL-R4.** A readiness (possession) proof MUST be treated as evidence of *retrieval*, not of
possession or capability: it MAY gate draw eligibility, it MUST NOT be paid for, and it MUST NOT be
the only thing between a stakeless party and a seat or a jury vote. *Because:*
possession_proof_binds_index_only.

**PANEL-R5.** A class whose derived verification time does not fit its receipt window, or which has
any fault class the court cannot attribute (§3.6 COURT-R9), MUST NOT admit weight- or reward-bearing
claims. *Because:* held_attention_lie_unattributable; a claim nobody can verify in time is licensed
by default or voided by clock.

### 3.2 The draw

**PANEL-R6.** Panel parameters MUST satisfy `1 ≤ q ≤ n`, `2q > n`, `K_ring ≥ 2`, and `W_ring` at
least the honest ring latency (the time for `K_ring` claims accepted around `c` to reach `Final`) with
a stated margin, both in `DaaSpan`. *Because:* with `2q ≤ n` a `Valid` quorum and an `Unavailable`
quorum can form on one claim at once (dual_quorum_opposite_licences). A ring made only of leaves that
reach `Final` after acceptance (04 POL-R9) is what keeps the attempt's own block from seeding its
panel, which t12's `anchor_delay > 0` did.

**PANEL-R7.** The panel seed MUST be 04 POL-R9's `panel_seed(ring, admission_index, draw_index)`
over the claim's post-acceptance `SeedRing`. It MUST NOT include any block identity, any header field
a producer can vary without a new execution (nonce inside the bucket, timestamp), the claim's id, or
any byte of an unverified commitment. The segment assignment (which seat index is the full seat)
MUST derive from the same seed. 03 §2.5 (A7) gives an `UnverifiedClaim` no seed authority, and 04 §2.6
shows the anchor's execution key is re-rollable on the root axis. This chapter's earlier proposal,
`H("misaka-next/panel/seed/v1" ‖ anchor.execution_key ‖ claim_id ‖ draw_index)`, is option (a) of Q1
below and of 04 Q2. The owner's choice is OQ-1 in `00-overview.md` §10, which also carries POL-R9's
missing bootstrap rule. *Because:* panel_draw_seed_grind (INV-PANEL-02, INV-POL-03).

**PANEL-R8.** The draw population MUST be the stake snapshot of the state strictly before the
claim's accepting block: the parent's state, fixed before any leaf of the claim's ring exists
(04 POL-R9). It is restricted to accounts eligible under 02 BOND-R4 and laid out in chain-assigned
registration order (02 BOND-R3). The population cut is a chain position, not a clock reading.
*Because:* a party that saw or could predict the seed could register, or grind account keys into the
draw's cumulative intervals, to sit on a chosen panel.

**PANEL-R9.** The executor's account, and every account with its key (and, where operator ids
exist, every bond of its operator), MUST be excluded. How many seats one account may hold is 02
BOND-R3's rule: seats are independent draws with replacement, so an account drawn `j` times holds `j`
seats. *Because:* self-judging; split-invariance (INV-BOND-02). *[Synthesis edit: this rule said "at
most one seat per operator", t12's rule, which contradicts BOND-R3; the choice is 02 Q2-1 / OQ-6.]*

**PANEL-R10.** Each eligible account MUST be weighted by its posted stake, uncapped (02 BOND-R3),
and a draw whose eligible weight is below `eligible_floor` of the base weight MUST NOT bind.
Eligibility (room for the lock a `Valid` will take) decides *whether* an account is drawn; posted
stake decides *how often*. *Because:* Sybil seats at zero stake; a saturated honest set must not hand
the seats to idle Sybils.

**PANEL-R11.** The chain MUST bind a claim's panel itself, as a derived step of the per-block
transition (11 §4 step 4), with no binding object for anyone to publish or withhold. It binds in the
first chain block in which the claim's seed ring is fixed (04 POL-R9). If `derive_panel` refuses
there, the claim voids `NoCapablePanel`, uncharged. If no ring is fixed by `ring_by`, the claim voids
`NoRing`, uncharged. A claim MAY be redrawn once, after a first panel fails to conclude, with
`DrawIndex::Redraw` on the same ring. *Because:* a draw retried on a later state is a free re-roll,
and a binding carried as an object could be withheld to void claims for free. This is t12's rule
too: the processor derives the bindings a block owes, "derived — not published"
(`consensus/src/pipeline/virtual_processor/processor.rs:11814-11828`), and prepends them to the block's
objects (`processor.rs:12290`). next keeps it (`anchor_bind_censorship`: closed at the reference).

### 3.3 Receipts and the definition of VerifiedClaim

**PANEL-R12.** A receipt MUST sign, under its own ML-DSA-87 context, the network domain, the claim
id, the panel id (ring block and draw index), the **seat index** it answers for, the verdict with all
its fields, the segment mask and `signed_at`. It is admissible only if all of these hold:

* `bound_at ≤ signed_at ≤ min(receipt_deadline, carrying block)`;
* the signer's account holds that seat index on that panel;
* no other receipt for the same `(panel, seat index)` has been counted.

One account holding seats `i` and `j` signs two receipts, one per index. Coverage and `basis_k` count
**distinct seat indices**, never distinct accounts (PANEL-R14). *Because:* replay of a receipt
across panels, claims, seats or verdict contents. Under draws with replacement (02 BOND-R3), a
receipt that named only the account could not say which of its seats it answers for.

**PANEL-R13.** Verdict semantics: `Valid` counts toward quorum and coverage; `Incapable` counts toward
nothing and is never charged, and is refused on the liveness floor class; `Unavailable` names a
committed unit and a request time inside the producer's retention; `Sampled` is an audit and counts
toward no quorum, no coverage and no release. *Because:* a seat that cannot judge must have an
honest, free answer; an audit is not a vote.

**PANEL-R14 (the panel proof).** `check_licence` MUST return a `LicenceProof` (from which
`consensus/claims::verify` builds the `VerifiedClaim`) iff all hold:
1. the panel is the one `derive_panel` returns for the claim's seed and population (recomputed);
2. every receipt passes PANEL-R12, each from a distinct seat index;
3. at least `q` receipts are `Valid` (`q` distinct seat indices);
4. **`basis_k ≥ K_final = 2`**: every one of the `K` segments is covered by `Valid` receipts from at
   least two distinct seat indices, where a whole-job `Valid` covers every segment and a segmented
   `Valid` covers only its assigned mask. Two seat indices held by one account count as two: the
   safety law is stated per seat draw (`P2 = s²`, 08 §4.5; OQ-6);
5. an outsider-judged claim's outsider is among the counted `Valid` signers;
6. every counted signer has posted its lock (`lock ≥ G_res / max(basis_k, 2)`) before it is counted;
7. the claim's class admitted it and is not `Registered`.

On t12's geometry (`n = 5, q = 3, K = 4`) this means one of exactly two shapes: **three or more
whole-job replays** (basis 3), or **all five seats** (the full seat plus every partial; basis 2).

**PANEL-R15.** A licence on fewer attestations (the full seat alone, or with one partial: basis 1)
MUST NOT produce a `VerifiedClaim`. A node MAY track it as progress; consensus gives it no weight,
reward or release, and it MUST be upgraded to basis ≥ 2 by further receipts inside the receipt window
or the claim is redrawn once and then voided. *Because:* optimistic_single_seat_licence.

**PANEL-R16.** What "verified" means, quantitatively. A `VerifiedClaim` states: every segment was
re-executed by ≥ 2 independent seat draws (stake-weighted, with replacement per 02 BOND-R3, so one
account may fill both) from a population fixed before an ungrindable seed, excluding the executor, each posting a slashable lock, the locks of the counted
signers summing to at least the claim's residual gain. It does **not** state that a lie is
impossible: a lie in segment `i` is undetectable without sampling only if both attesters of `i`
(the full seat and `i`'s holder) are the attacker's, or all counted whole-job signers are.
Its security is therefore `P(capture per honest draw) × draws per unit cost`. PANEL-R7 and 04 POL-R9
make the second factor one admitted, licensed claim per try. On a branch the adversary produces,
that is `P_cap` (10 §2.1 C11).

**PANEL-R17.** The node's licence selection MUST be defined as "the largest receipt subset the
acceptance predicate accepts", computed by calling that predicate, never by a second count.
*Because:* licence_assembler_stall (INV-PANEL-07).

### 3.4 Timeouts and silence

**PANEL-R18.** Silence MUST NOT be charged to a seat. The chain cannot observe that a message was not
sent (ADR-0064), only that an object is absent from one branch. *Because:* seat_silence_mispriced.

**PANEL-R19.** A first panel that fails to conclude MUST lead to one redraw with different seats, not
to a charge. A second failure charges what 02 BOND-R13 says: the S0′ forfeit of the claim's
commitment at its stage, until OQ-7 is decided. The charge MUST NOT be larger than what a seat-Sybil
must lock to cause it. *Because:* silent_quorum_griefing. *[Synthesis edit, review: this rule called
the charge open while BOND-R13 already required S0′.]*

### 3.5 Maturity

**PANEL-R20.** `mature` MUST return a `FinalClaim` only when `basis_k ≥ 2`, no court session and no
seat DA session is open on the claim, and `challenge_ends = Deadline::<Safe>::after(licensed_at,
window_challenge(class))` has elapsed at the block's `SafeDaa` (PANEL-R21). A court on a licensed
claim suspends its maturity until the court closes. *Because:* a lie must be contestable for the
whole window. `Final` governs money, seeds and rights. Fork-choice weight is frozen by burial, not
by `Final` (06 §3.3), so this `SafeDaa` window never enters a fork key.

### 3.6 Courts, fraud proofs and verdicts

**COURT-R1.** A court closes with a verdict only on a proof every node can check from the object and
the branch state; a proof that does not adjudicate (out-of-catalog kernel, missing operand, wrong
shape) MUST refuse the close, minting neither `Convicted` nor `Acquitted`.

**COURT-R2.** Defaults MUST be machine facts derived by the sweep from a missing move after its
deadline; no object may assert a default. Each move MUST be signed by its party under a context
used by no other move kind. *Because:* a forged default would void any claim on demand.

**COURT-R3.** A default (`ConvictionBasis::Default`) convicts only the party that was due to move. It
MUST NOT be evidence against any third party (the `Valid` signers). *Because:* a producer's
deliberate silence in its own court would otherwise slash the honest seats that licensed it
(deliberate_court_loss_slashes_signers).

**COURT-R4.** Every conviction MUST be attributed by one adjudicator through the chain *claim →
committed root → execution (binding rebuilds the root) → job identity → leaf → signer and mask →
fault → slash target*. A fault proving the root answers another job (identity) forfeits by claim,
never by root. A segmented signer is liable only for a fault every committed read of which lies
inside its mask.

**COURT-R5.** No session may monopolise a claim: an accusation by another accuser, or at another
leaf, MUST remain admissible while a session is open, within a per-claim bound. A decoy session
MUST NOT shorten any honest party's challenge window. *Because:* decoy_dissection_preemption.

**COURT-R6.** An acquittal that relied on data the accused committed (a checkpoint, an anchor) is
conditional: if the same claim is later convicted on that data, the defeated challenger's forfeit
MUST be restored from the convicted party. *Because:* forger_race_challenger_forfeit.

**COURT-R7.** Turn deadlines MUST be derived per class: a response move gets `base_turn`; a move
whose builder re-executes gets `max(base_turn, ⌈COMPUTE_TURN_MARGIN × replay(class) / anchor_period⌉)`,
and the class is refused at registration if that exceeds a cap. For every admitted class,
`rounds × turn + assembly_reserve < window_court`. The node MUST read these from the same function.
*Because:* a deadline shorter than honest compute convicts honest responders by the clock.

**COURT-R8.** A reporter reward MUST go only to a reporter whose commitment to the offence key was on
the branch before the reveal (commit–reveal); a reveal without a prior commitment earns nothing.
*Because:* reporter_reward_front_running.

**COURT-R9.** A fraud proof MUST fit the carriage it rides. A fault class whose proof cannot be
carried, or which has no conviction route (e.g. an attention lie on a held-context class), is
*unattributable*; PANEL-R5 then applies to every class that can contain it.

**COURT-R10.** A conviction after `FinalClaim` MUST debit the claim's unmatured vesting rows and the
counted signers' locks. No conviction may remove weight already counted in `safe` (06 §3.3). A
conviction reads only the claim record, its licence certificate and the counted signers' locks, all
in rooted state until `conviction_ends`, plus material the accused must serve (DA). It reads no
pruned block body (07 FINAL-R10). Slashed value MUST be burned or paid per the schedule of chapters
02 and 08 (BOND-R11, ECON-R10); nothing is minted.

**COURT-R11 (DA).** A DA session names one committed unit; the fold draws up to 3 more units from the
accepting block; sessions are keyed per `(claim, accuser)`; any bond with a live lock on the claim
may answer; a refuted session costs the accuser; a default charges the producer and the signers
whose mask covers the unit. The disclose deadline MUST be a `Deadline<ChainFinal>`, marked at the
session's opening block (01 §2.6). A default is swept only once the chain's own finalized anchor has
passed the whole disclose window. Every block in which an answer could have landed is then final on
that chain, so a reorg that removes the answer crosses a finalized anchor (07 FINAL-R5). *Because:*
t12's rule, a disclose window of at least twice the finality depth (`palw_state_v2.rs:3852`),
compared a DAA window with a depth that next counts in safe anchors and verified weight, which
DAA-R15 forbids. *[Synthesis edit, review.]*

### 3.7 Clocks

**PANEL-R21.** Every deadline is marked at its creating block's `LocalDaa` (01 §2.6). Which clock
must pass it is 01 §2.7's authority table.

* The seed-ring wait `ring_by` (an uncharged void) is a `Deadline<Local>`.
* The receipt, challenge and turn deadlines are `Deadline<Safe>` (01 DAA-R10, 03 CLAIM-R9). Their
  expiry charges a party or grants authority: redraw-then-forfeit, maturity, a court default.
* The DA disclose deadline is a `Deadline<ChainFinal>` (COURT-R11).
* The draw population cut is a chain position, the parent of the accepting block (PANEL-R8), not a
  clock.
* The audit schedule is epoch-keyed and reads `SafeDaa` (01 DAA-R17).
* Readiness expiry drops a seat from draws, so it reads `SafeDaa` (01 §2.7, census C52).

No rule in this chapter reads wall-clock time. *[Synthesis edit: this rule made every deadline `LocalDaa` and
relied on chapter 07 (fork choice is chapter 06) to keep a private branch non-canonical; that
contradicted 01 DAA-R10 and 03 CLAIM-R9. A private branch that wins nothing in fork choice (06) still
must not convict absent parties by its own clock.]*

### 3.8 The consensus / node-policy boundary

**PANEL-R22.** Every value both the fold and a node compute (the court shape and arity, turn
deadlines, eligibility, the licence predicate, the panel itself) MUST be exported by `pol/panel` as
one pure function; node code MUST NOT re-derive it. *Because:* panel_arity_mismatch_defaults_honest.

| Consensus (`pol/panel`) | Node policy (not in the crate) |
|---|---|
| `derive_panel`, `panel_seed`, `segment_assignment` | fetching material, replaying, deciding to sign |
| `check_receipt`, `check_licence`, `basis_k` (`mature` is `consensus/claims`') | the receipt pool, eviction, gossip |
| `court_shape`, `turn_deadline`, `adjudicate`, DA unit draw | when to open a court, when to file, carrier queues |
| `jury_seed`, `jury`, `lifecycle_step`, readiness check | producing readiness proofs, prefetching artifacts |
| slash amounts and targets | auto-answering DA and court moves, commit–reveal filing |

## 4. Pure functions

All take values and return values; none reads a store, the network or a clock.

```rust
pub fn panel_seed(ring: &SeedRing, admission: AdmissionIndex, draw: DrawIndex) -> PanelSeed;   // 04 §4.6
pub fn derive_panel(claim: &ClaimRecord, seed: PanelSeed, population: &SeatPopulation,
                    params: &PanelParams, policy: &DrawPolicy) -> Result<Panel, DrawRefusal>;
pub fn segment_assignment(seed: PanelSeed, seats: u16) -> SegmentAssignment;   // which seat index is full
pub fn receipt_message(net: &NetworkDomain, body: &ReceiptBody) -> Hash64;
pub fn check_receipt(panel: &Panel, r: &SignedReceipt, keys: &SeatKeys,
                     window: &ReceiptWindow) -> Result<CheckedReceipt, ReceiptRefusal>;
pub fn basis_k(assignment: &SegmentAssignment, counted: &[CheckedReceipt]) -> BasisK;
pub fn check_licence(bound: &BoundRecord, receipts: &[SignedReceipt], ctx: &PanelContext)
    -> Result<LicenceProof, LicenceRefusal>;            // pol/verification; consensus/claims::verify consumes it
// `mature` is consensus/claims's (03 §4.1); its predicate is restated below.
pub fn select_licence(pool: &[SignedReceipt], bound: &BoundRecord, ctx: &PanelContext)
    -> Option<Vec<SignedReceipt>>;                      // defined via check_licence (PANEL-R17)
pub fn jury_seed(class: &ClassId, audit_span: u64, ring: &SeedRing) -> JurySeed;   // source as panel_seed
pub fn lifecycle_step(state: ClassLifecycle, obs: &LifecycleObservation, profile: &DerivedProfile)
    -> ClassLifecycle;
pub fn court_shape(bundle: &RulesetBundle, fences: &FencesAt) -> Result<CourtShape, NoAdmissibleArity>;
pub fn turn_deadline(class: &ClassProfile, kind: MoveKind, base: DaaDelta) -> DaaDelta;
pub fn adjudicate(evidence: &OffenceEvidence, claim: &ClaimRecord, class: &ClassRecord) -> Verdict;
pub fn da_units(accepting_block: &ExecutionAnchor, session: &DaSession, run: &CommittedRun) -> Vec<DaUnit>;
```

Semantics that are easy to get wrong:

```text
basis_k(A, R) = min_{s in 0..K} min(3, |{ r.seat : r in R, r.verdict = Valid, covers(r.mask, s) }|)
                   -- distinct SEAT INDICES (PANEL-R12); a whole-job Valid covers all s;
                   -- K = 0 is read as one segment
select_licence(pool) = the largest sound subset S of order(pool) such that check_licence(S) = Ok,
                   where order = full seat's Valid, then outsider's Valid, then other Valid, then rest;
                   a receipt is sound if adding it to a sound set yields Ok or a *shortfall*
                   (too few Valid, a segment short, outsider missing); anything else poisons.
mature(v, now)  = Ok iff basis_k(v) ≥ 2 ∧ no open court ∧ no open seat DA session
                   ∧ Deadline::<Safe>::after(v.licensed_at, window_challenge(class)).elapsed(now)
```

Required properties: `derive_panel` is deterministic and independent of the order bonds are stored;
the panel is unchanged by any header field of any block (nonce within its bucket, timestamp) and by
the claim's id, and changes only with the ring, the admission index, the draw index and the
population snapshot; `check_licence` counts each seat index at most once, however many seat indices
one account holds; `select_licence(pool)` returns `Some` iff some subset of `pool` verifies;
`adjudicate` never returns `Convicted { basis: Default }` naming anyone but the defaulting party.

## 5. Invariants upheld

Full list and cross-references in 09-invariants.md. Each has a test of the same name.

| ID | Statement |
|---|---|
| INV-POL-01 | An unverified LLM commitment MUST NOT obtain the same consensus authority as verified computation. |
| INV-ECON-01 | Maximum guaranteed attacker gain before detection MUST NOT exceed guaranteed slashable collateral. |
| INV-PANEL-01 | No `VerifiedClaim` exists with `basis_k < 2` or fewer than `q` counted `Valid` seat indices; each seat index counts once. |
| INV-PANEL-02 | The panel of a claim is unchanged by any header field the anchor's producer can vary without a new winning execution. |
| INV-PANEL-03 | The executor's account, operator and key never hold a seat on its own claim. ("No operator holds two seats" was t12's rule; under 02 BOND-R3 an account may hold several seats — OQ-6.) |
| INV-PANEL-04 | A `Valid` quorum and an `Unavailable` quorum can never both form on one panel. |
| INV-PANEL-05 | Every account in a draw's population was registered, and mature, strictly before the seeded claim's accepting block, hence before any leaf of its ring existed. |
| INV-PANEL-06 | An outsider-judged claim never becomes `VerifiedClaim` without its outsider's `Valid`. |
| INV-PANEL-07 | The node's licence selection returns a set iff the acceptance predicate accepts some subset of the pool. |
| INV-PANEL-08 | A licence of basis < 2 carries no weight, reward, seat pay or release. |
| INV-PANEL-09 | A `Candidate` class admits no claim. |
| INV-PANEL-10 | Node and fold derive one court shape, turn deadline and eligibility for the same inputs. |
| INV-PANEL-11 | No transition charges a seat for silence. |
| INV-COURT-01 | An unadjudicable proof yields neither conviction nor acquittal. |
| INV-COURT-02 | A default convicts only the defaulting party and is never a basis against signers. |
| INV-COURT-03 | Every conviction's evidence rebuilds the claim's committed execution root (or proves a job-identity fault). |
| INV-COURT-04 | An open session never prevents or delays an admissible accusation on the same claim beyond its bound. |
| INV-COURT-05 | A challenger defeated on data the accused committed is made whole when that data is convicted. |
| INV-COURT-06 | Every admitted class's worst honest prosecution fits `window_court` at its derived turns. |
| INV-COURT-07 | A reporter reward is paid only against a commitment that preceded the reveal on the branch. |
| INV-COURT-08 | Slashing conserves value: debited = burned + paid; no mint. |

## 6. t12 reference

### 6.1 The lattice and its authority

t12's lattice is `Provisional → PanelBound → ReceiptLicensed → Final | Voided`
(`consensus/core/src/palw_state_v2.rs:51`), plus `DefaultDisputed` for ADR-0062 DA sessions
(`:3858`). An immature claim contributes `⌊β·pwu/1000⌋` to live weight and a `Final` attempt claim
its full pwu to `safe_weight` (`:57-58`, `:2035`); β is 100‰ (`consensus/core/src/palw_fp_devnet_v3.rs:34`).
So a claim no panel has seen already carries a tenth of its weight in `live_total`: INV-POL-01 holds
only in the letter. next leaves any pre-verification weight to chapter 06 and gives the panel no part
in it (§2.3). A class bears weight iff it holds a granted share (`palw_state_v2.rs:2066`), and share
> 0 requires end-to-end certification at admission (ADR-0069 D7; `FamilyCertified` and
`ClassLaneCertified`, `:5147`, `:5157`).

### 6.2 Panel parameters and the draw

* 5 seats, quorum 3 (`palw_fp_devnet_v3.rs:780-781`), built for every V2 network through
  `PalwPanelParamsV2::new` (`:1006`), which refuses `2q ≤ n` and a zero anchor delay
  (`consensus/core/src/palw_panel_v2.rs:355`, `:360`). t12 assembles through the shared V2 builder
  (`consensus/core/src/config/params.rs:10689`, `:15184`) and arms every fence at DAA 0 except three
  (`params.rs:15751` and its doc). The RC lattice is bind 600, receipt 600, challenge 1,200, court
  3,000, anchor delay 20, turn 42 (`palw_fp_devnet_v3.rs:165-196`); that t12 inherits it unchanged is
  inferred from the builder, not traced end to end [unverified]. The short challenge window armed on
  t12 is 120 (`palw_state_v2.rs:1386`, `:1393`).
* The draw (`palw_panel_v2.rs:1120-1244`): exclusions by executor bond, operator and key (`:1052`);
  population cut before the anchor under ADR-0147 (`:272-292`, `:1047`); an outsider seat first for
  bought classes; past `palw_rcore_plus` a stake race keyed `−log2(u)/W`, `W` = posted MSK capped at
  1,000,000 (`:1650`), with an 875‰ eligible-stake floor (`:143`, `:1226`).
* The seed. Every ticket hashes `anchor_block`, the anchor's **block identity**
  (`palw_panel_v2.rs:1484-1510`, `:1667-1684`), and so does the segment assignment
  (`consensus/core/src/palw_verification_v2.rs:117-130`). Block identity covers the real nonce and
  timestamp (`consensus/core/src/hashing/header.rs:169-174`); an attempt's execution anchor covers the
  pre-PoW hash with nonce and time zeroed plus the nonce *bucket* (`hashing/header.rs:211-213`;
  `consensus/core/src/palw_attempt_v2.rs:530-538`), and a bucket is 2^22 nonces
  (`palw_attempt_v2.rs:213`, `:233`). The anchor's producer therefore re-rolls every panel it anchors
  about 2^22 times per timestamp, for free, once it holds one winning execution. The execution lane
  already says so of its own seed (`consensus/core/src/palw_execution_lane_v1.rs:396-401`). The
  module doc's "neither the executor nor the binder can grind it" (`palw_panel_v2.rs:19-23`) and the
  processor's "every other anchor costs one more inference"
  (`consensus/src/pipeline/virtual_processor/processor.rs:10063-10076`) hold for the anchor
  *choice* (attempt blocks only past `palw_rcore_plus`, `processor.rs:10080-10086`) but not for the
  seed. ADR-0152's success probabilities assume "(viii) the draw is not captured" (ADR-0152 v3.1
  §4.1). **Defect D1**; PANEL-R7 is the fix. No pending branch changes claim-panel seeds.
* Bind only in the anchor block, **and the chain binds**. The processor derives "the panel bindings
  this block owes, derived — not published" (`processor.rs:11814-11828`). `palw_v2_objects_of_block`
  prepends them to the block's objects (`processor.rs:12290`), and the acceptance walk calls it
  (`processor.rs:2053`). The fold's own comment says the same: "the chain binds a panel itself the
  moment the anchor is reached" (`palw_state_v2.rs:23733-23735`). Step 4c then voids every
  `Provisional` claim the derivation could not bind (`palw_state_v2.rs:23388-23430`), without forfeit,
  as `BindTimeout` or, for a class that cannot seat a panel, `NoCapablePanel`. No producer publishes a
  `PanelBound` that it could withhold.

### 6.3 Receipts, doors and the meaning of "licensed"

* Verdicts `Valid`, `Unavailable{chunk, requested_daa}`, `Incapable`, `Sampled`
  (`palw_panel_v2.rs:2143-2185`); the V2 message signs network, claim, verdict fields and
  `signed_daa` (`:2211-2231`), the V3 message adds the mask (`:2234-2247`). The panel instance is not
  signed; `signed_daa ≥ bound_daa` separates a redraw's receipts. next signs the panel id (PANEL-R12).
* `Incapable` is refused on the floor class (`palw_state_v2.rs:4211`).
* Coverage door (`ReceiptLicensedV2`): V1 quorum, then every segment attested ≥ 2
  (`palw_panel_v2.rs:2437-2591`; constant `palw_verification_v2.rs:26`; `K = n − 1`, `:79`), each
  `Valid` mask equal to the seat's assignment (`palw_panel_v2.rs:2468`), outsider required (`:2573`).
* Optimistic single-replay door (`OptimisticLicensed`, ADR-0133's "S2"): the full seat's `Valid` alone
  (`consensus/core/src/palw_optimistic_licence_v2.rs:27-60`, `:69`).
* R-core+ recount: `basis_k = min(3, min over segments of covering counted Valid)`
  (`palw_state_v2.rs:2920-2924`), `K_final = 2` (`:961`). A licence of basis < 2 is not counted as
  licensed for the panel room (`:2930`); at its deadline the first panel redraws and the second voids `NotReplayBacked`
  (`:23756-23766`); only basis ≥ 2 finalizes (`:23768-23771`). So on t12, `Final` already requires
  what PANEL-R14 requires. next moves the rule from the maturity sweep into the type: basis < 2 is
  never a `VerifiedClaim`.
* Licence selection (`palw_panel_v2.rs:2594-2766`), called by the processor's assemblers
  (`processor.rs:7455`, `:7625`): node policy inside the consensus crate, correctly defined through
  the acceptance predicate (`is_receipt_set_shortfall`, `palw_panel_v2.rs:492`).

### 6.4 Registry, readiness, jury

* Lifecycle and `admits_claims` (`consensus/core/src/palw_model_registry_v1.rs:329-380`), step
  function (`:483`). Past the 2026-09-23 audit fence utilization is no longer a lifecycle input and
  the panel room is a rate (`palw_state_v2.rs:12022-12027`, `:17256-17270`).
* Readiness challenge seeds are `H(class, bond, span)` (`palw_model_registry_v1.rs:936-944`,
  `:1055-1062`), known for every future span; V2 opens 16 leaves and signs the bytes it opened
  (`palw_state_v2.rs:5634-5648`, `SeatReadinessProvedV2`). The leaves are public artifact
  bytes. **Defect D2**: a bond that holds nothing proves "readiness" by fetching 16 leaves.
* Admission jury: population = base-class bonds at the *registry* floor, registered before the
  seed's span, excluding the registrant (`palw_state_v2.rs:16909-16960`); one ticket per operator,
  not stake-weighted (`palw_panel_v2.rs:1462-1468`); quorum `n/2 + 1`
  (`palw_model_registry_v1.rs:452`). The seed hashes the anchor's block **and** its execution key
  (`palw_model_registry_v1.rs:465-471`, used at `palw_state_v2.rs:16927`), so the block-hash grind of
  6.2 applies. **Defect D3**; *pending* seed v2 drops the block hash (`feat/t12-activation-pool`,
  `palw_admission_jury_seed_v2`).

### 6.5 Timeouts and silence

`slash_silent_seats` has an empty body (`palw_state_v2.rs:19164-19171`). A first receipt timeout
redraws; the second charges the producer (`void_and_slash(ReceiptTimeout)`) past the 2026-09-23
audit fence, whoever caused the silence (`:23676-23741`). ADR-0152 accepts this as "silent-quorum
griefing" (§4.2 row 12). Dissenting seats lose only what they reserved (`:19053`).

### 6.6 Courts

* Closes need checkable proofs; defaults are swept, never carried (`consensus/core/src/palw_court_v2.rs:6-33`).
* Turn 42 is derived at the deepest ladder: `66 × 42 + 216 = 2,988 < 3,000`
  (`palw_fp_devnet_v3.rs:175-196`); the SA-4 two-ended inequality is in
  `consensus/core/src/palw_court_deadline.rs:14-18`.
* One court per claim: a one-move accusation is refused while any session is open on the claim
  (`palw_state_v2.rs:23985-23987`). **Defect D4** (decoy monopoly); *pending* `9bf7accf`.
* Held classes keep ADR-0093's mercy: `court_root_evidence_is_buildable_v1` is false for a held
  class (`palw_state_v2.rs:8117-8136`), so a held responder that withholds its attention root claim
  is not convicted. An attention lie on a held class (8k, 2M) has no conviction route at the
  reference; ADR-0152 §4.2 row 18 names it a launch gate. **Defect D5**; *pending* A-held (object
  57, C1–C5) on `feat/t12-aheld-node`, and 2M closed at launch (*pending*
  `feat/t12-class-verify-deadline`).
* A `ChallengerDefeated` close is final for the challenger's stake (`palw_state_v2.rs:25516-25518`);
  no later conviction restores it (no forfeit record exists at the reference). **Defect D6**
  (forger's race); *pending* `c68479db` (`PalwHeldForfeitV1`).
* Court shape: the processor derives the arity with the held-aware function
  (`processor.rs:11332`), the node's dense root-claim path with the plain one
  (`kaspad/src/palw_panel.rs:6749`); on t12 the plain derivation finds no arity while the processor
  finds 4 (memory lead; the pending fix's own test pins 4). **Defect D7**; *pending* `d2752b54`.

### 6.7 Offences and verdicts

* Kinds (`consensus/core/src/palw_offence_v1.rs:44-75`): `PanelFalseValidV2 = 3` (`:58`),
  `ExecutorRefuted = 4` (`:64`), fold-only `DaDefault = 5`, `CourtConviction = 6`. Contradictions
  0–13 (`:159-227`). One adjudicator with the attribution chain of COURT-R4
  (`consensus/core/src/palw_offence_attribution_v1.rs:1-50`); evidence capped at one carrier
  (`:91`), which puts large softmax/KV step refutations out of kind 3's reach (`:85-89`).
* `CourtDefault` (void reason 7) is written for defaults past `palw_offence_attribution` and refused
  as a kind-3 basis (`palw_state_v2.rs:3806-3818`): COURT-R3 holds at the reference. *Pending*
  `3ee76a99` adds reason 8 (`CourtHeldVerdict`): every dissection verdict is producer-only.
* Collusion sizing: `seat_slashable × 3 > G` (`palw_offence_v1.rs:34`, `:448-456`); action multiple
  `m = 3` (`palw_state_v2.rs:949`).
* Commit–reveal: `ReporterCommitted` (53) and `ReporterRevealed` (54) fold past `palw_rcore_plus`
  (`palw_state_v2.rs:25719-25734`); `PanelUnavailableQuorum` (56) is still refused (`:25743-25746`).
  *Pending* `rcore/p2-file`: the node files `PanelFalseValidV2` from its own evidence
  (`palw_false_valid_filing_v1.rs`).

### 6.8 Data availability

R-core+ sessions keyed per accuser, 3 drawn units, 3 open non-seat sessions and 16 per claim
(`consensus/core/src/palw_da_rcore_v1.rs:1-18`, `:33-37`); the ADR-0062 window rule
`W_disclose ≥ 2 × finality` (`palw_state_v2.rs:3852`). Seat auto-answers are landed (the
`PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` doc, `palw_da_rcore_v1.rs:41-60`). Only a full mask covers a
unit, so partial signers are never charged by a DA default.

### 6.9 Node policy at the reference

`kaspad/src/palw_panel.rs` (seat service: duties, material checks incl. SEAT-0 in
`misaka-palw-base0/src/produce.rs:1388-1398`, receipts, court and DA answers, challenger),
`kaspad/src/palw_producer.rs`, `kaspad/src/palw_receipt_pool.rs` (per-(claim, bond) pool, own
receipts unevictable, `:1-30`). The receipt-pool flush and the greedy assembler are fixed at the
reference; the root-claim arity (D7) is not.

### 6.10 Where next diverges, and why

| t12 | next | Reason |
|---|---|---|
| seat, outsider and assignment seeds hash the anchor block identity | seed without block identity (PANEL-R7; source 04 POL-R9 or the execution key, OQ-1) | D1: free re-roll |
| jury unweighted, seeded with the block hash | jury stake-weighted (Q4), seeded as panels are (PANEL-R7) | D3, Sybil operators |
| basis ≥ 2 enforced at the maturity sweep | basis ≥ 2 is the `VerifiedClaim` constructor | type carries authority |
| receipt does not name the panel | receipt signs the panel id and its seat index | cross-panel replay by construction, not by `signed_daa`; one account's two seats are distinguishable |
| seed from the anchor block identity; panel bound in the anchor block | seed from post-acceptance Final leaves and the admission index; panel bound in the block that fixes the ring | D1; 04 POL-R9 |
| readiness = 16 fetched leaves, pay proposed on it | readiness is retrieval; never paid; never sufficient alone | D2 |
| one court per claim | bounded sessions per claim, no monopoly | D4 |
| held attention unattributable (mercy) | a class with an unattributable fault class confers nothing | D5 |
| acquittal final | acquittal on accused data is conditional | D6 |
| node re-derives court arity | one exported function | D7 |
| β immature weight before any panel | the panel grants nothing before `VerifiedClaim` | INV-POL-01 |

## 7. Attacks this chapter defends against

Detailed in 10-attack-model.md. Verdicts are for the t12 reference.

* **panel_draw_seed_grind** (this chapter's earlier name: panel_anchor_regrind) — real. §6.2 D1.
  Next: PANEL-R7, INV-PANEL-02, INV-POL-03.
* **admission_jury_seed_grind** — real at the reference; pending seed v2. §6.4 D3.
* **colluding_quorum** — partial. Three colluding whole-job `Valid`s license and reach basis 3;
  detection needs one honest replay and a filing inside the window; locks priced so three convicted
  locks exceed `G`. ADR-0152 §4.2 row 13 puts the undetectable coverage lie EV-positive from
  17.29M MSK of Sybil stake, assuming an uncaptured draw, which D1 breaks.
* **private_fake_root_burst** — partial. On the honest chain SEAT-0 seats refuse a fake root and
  the claim voids after two panels (producer charged); before any panel the claim holds β live
  weight. On a private branch the forker produces every anchor, grinds every panel (D1) and licenses
  its own fake claims, so the frontier doc's "its claims cannot mature" (`palw_state_v2.rs:65-69`)
  fails. Chapters 06 and 07 must not rely on it.
* **private_daa_finality_acceleration** — partial. Maturity reads the branch's own DAA
  (`palw_state_v2.rs:23665-23667`); verification on a private branch is attacker-controlled (above).
  The panel cannot defend this alone; cross-reference 01 (safe clock), 06 and 07.
* **heartbeat_clock_acceleration** — partial (verdict owned by 01 §7). Every panel and court
  deadline reads the block's DAA score, so it is exactly as accelerable as chapter 01's clock.
* **bond_split_amplification** — partial. Below the 1M MSK cap, weight is linear in posted stake;
  above it, splitting into operators adds weight (`palw_panel_v2.rs:1650`); operator ids are free, so
  the cap bounds a single operator, not a party.
* **false_valid_signer** (PanelFalseValidV2) — partial. Kind 3 is live with root binding and mask
  liability; out of reach are proofs over one carrier, held attention, partial masks outside their
  segment, and defaults. The node filer is pending.
* **silent_quorum_griefing** (ADR-0064, silence is not checkable) — real. It is accepted in the ADR:
  two silent panels charge an honest producer, and silent seats pay nothing.
* **replay_budget_horizon_collapse** — closed on t12 (audit #4 fence).
* **licence_assembler_stall** (earlier name: licence_stall_greedy_assembler) — closed at the
  reference.
* **receipt_pool_flush** — closed at the reference (node).
* **panel_arity_mismatch_defaults_honest** (earlier name: panel_root_claim_arity_mismatch) — real on
  t12 (D7). Pending fix.
* **forged_filing_poisons_node_cache** (earlier name: decoy_tag57_cache_poisoning) — not applicable to the reference (no tag 57); fixed on the pending
  line (`d2752b54`, checked filings plus eviction).
* **decoy_dissection_preemption** (earlier name: decoy_court_session_monopoly) — partial (D4). Pending.
* **forger_race_challenger_forfeit** (earlier name: forgers_race) — real (D6). Pending `c68479db`.
* **possession_proof_binds_index_only** (earlier name: readiness_proof_outsourced) — real (D2): challenges are predictable and the answers are public
  bytes.
* **admission_jury_sybil_capture** — partial. The jury is unweighted over operators at the registry
  floor; capture moves a class only out of `Candidate`, and its later panels are stake-drawn with
  an outsider.
* **anchor_bind_censorship** — closed at the reference. The anchor producer publishes no
  `PanelBound`: every validating node derives the bindings as part of acceptance (§6.2;
  `processor.rs:11814-11828`, `:12290`, `:2053`; `palw_state_v2.rs:23733-23735`), so omitting a bind is
  impossible. The only residual lever is the anchor producer's choice and grinding of its anchor
  block's hash, which is `panel_draw_seed_grind`. next keeps the derived binding (PANEL-R11).
  *[Synthesis edit, review: this entry was "real, medium confidence", a misread of t12.]*
* **held_attention_lie_unattributable** — real (D5). It is a launch gate in ADR-0152.
* **deliberate_court_loss_slashes_signers** (the default half; earlier name:
  default_as_signer_evidence) — closed for defaults (`CourtDefault`); dissection verdicts pending.

## 8. Open questions for the project owner

**Q1. Seed source for claim panels.** Options: (a) the anchor's execution key (this chapter's draft
PANEL-R7); (b) a commit–reveal beacon over K attempt blocks; (c) keep block identity and cap re-rolls
by a header PoW. *Recommend (a)*: it prices a re-roll at one winning inference, needs no new object,
and matches ADR-0130's own seed.
*Synthesis note:* the book's normative seed is 04 POL-R9 (post-acceptance Final leaves, admission
index), because an execution key is re-rollable on the root axis for hashes (04 §2.6). This
recommendation is option (a) of OQ-1 in `00-overview.md` §10, which recommends (b) there with a
bootstrap rule.

**Q2. Bind censorship by the anchor producer.** *Resolved in synthesis.* The question assumed t12
publishes `PanelBound` objects. It does not: the chain derives the bindings at acceptance (§6.2). So
option (b), "the fold binds due panels itself, with no object", is already t12's rule, and next
keeps it as PANEL-R11. What remains is the anchor producer's grinding of its block hash
(`panel_draw_seed_grind`), which POL-R9 removes. OQ-8 records the same.

**Q3. Who pays when two panels fail silently?** Options: (a) the producer (t12); (b) nobody: void
at S0, and the producer loses only the reward; (c) the silent seats, by an expiring bond-level
"availability strike" that needs no proof of silence, only of non-appearance on the branch.
*Recommend (b) at launch plus (c) as a rate limit*: the chain cannot tell a withholding producer
from a silent seat, so a charge on either side is a free weapon for the other.
*Synthesis note:* the consolidated question is OQ-7, which recommends keeping (a) until OQ-9 (b) is
in force and rejects (c) because it conflicts with 02 BOND-R16 (no per-account escalation). The
normative rule until then is BOND-R13's S0′ (PANEL-R19).

**Q4. Jury weighting.** Options: (a) one ticket per operator (t12, SW-A4); (b) the seats' stake race.
*Recommend (b)*: operator ids are free, so an unweighted jury is priced at the registry floor per
Sybil.

**Q5. Optimistic licences.** Options: (a) keep the optimistic single-replay door (ADR-0133's "S2") as a consensus door with no authority (t12 after
Q-5); (b) drop the object and let nodes track progress locally. *Recommend (b)*: a door that
confers nothing still costs a validator path, a fold arm and a redraw rule.

**Q6. Held and long-context classes.** Options: (a) admit them with an economic cap on the
unattributable residual (ADR-0152's 2M option C); (b) refuse weight and reward until every fault
class has a carried proof (PANEL-R5). *Recommend (b)*. INV-ECON-01 is stated for guaranteed
collateral, and a residual no court can reach is not guaranteed.
