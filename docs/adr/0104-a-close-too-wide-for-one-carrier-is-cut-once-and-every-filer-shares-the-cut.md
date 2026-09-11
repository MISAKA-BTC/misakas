# ADR-0104 — a close too wide for one carrier is cut once, and every filer shares the cut

Status: accepted, 2026-09-11. Supersedes nothing; completes ADR-0080 design A's filing half and
gives ADR-0096 §10 B13's split close a filer.

## 0. The sentence this ADR is

**The chain can already receive a court close in parts; only one program can send one, and it is
not the one that files closes.** ADR-0080 design A's transport is complete — a signed
`CourtCloseDeclared` pins every byte, `CourtCloseChunk`s carry them, the completing chunk assembles
and adjudicates through the same function a one-carrier close takes — and `misaka palw court-close`
drives it. The node's own court does not: it builds one `CourtClosed`, hands it to
`build_lifecycle_tx`, and a close that does not fit one carrier is a warning in a log. So the
cutter, the resume and the two ceilings live in a command-line tool, and the party that actually
prosecutes disputes cannot use them. This ADR moves the cut into consensus-core, where the
assembler already lives, and makes the node a consumer of it.

## 1. What was measured (2026-09-11, `feat/adr-0096-partb-drill` at `c9637c56`)

* **The transport is open, and the refusal that used to shut it is gone.** `processor.rs`'s
  `CourtCloseDeclared` arm verifies the declaring side's signature through
  `check_court_close_declaration_acceptance_v2` and the ruleset's own count through
  `check_close_declared_chunk_count_v2`; its `CourtCloseChunk` arm assembles the completed group,
  checks `close_digest`, decodes and calls `adjudicate_court_close_v2` with the fence at the
  block's DAA. The 2026-09-03 note that "W5 built the state layer and the validation layer refuses
  every declaration" describes a build this tree has left.
* **The transport is generic over the proof.** The completing chunk decodes a `CourtClosed` and
  adjudicates it; nothing in the split path reads the proof's variant. ADR-0096 §10 B5's
  `ConstrainedDecode` and B6's `ConstrainedRendering` therefore ride it unchanged — they were
  appended to `PalwCourtVerdictProofV2` after `ArithmeticOpened` and the arm that carries them is
  the same one.
* **One program can cut a close, and it is the CLI.** `misaka-cli/src/palw_court.rs` holds
  `CarriagePlan`, `plan_carriage_v1`, `parts_to_send_v1`, `court_close_max_parts_v1`,
  `check_assembly_window_v1` and `sign_declaration_v1`, all `pub(crate)`. They are good: the cut is
  the court's own (`palw_court_close_chunk_digest_v1` per index), the resume is the chain's
  (`GetPalwPendingChunkGroup`'s `present` bitmap, a SET and not a prefix), and both ceilings are
  read off the ruleset rather than typed.
* **The node cannot.** `kaspad/src/palw_panel.rs` keeps `court_pending: Vec<(Hash64, u32, bool,
  PalwConsensusObjectV2)>` and submits each entry with one `build_lifecycle_tx`. A close over a
  carrier fails to build or is refused by the mempool, and the arm logs `cannot build the carrier`
  and drops the move. **A close denied through its assembly window is not a delay but a conviction
  of the declarer** (the RC card's own sentence), so the failure mode is not "slow" — it is a
  prosecution the node silently abandons.
* **This is reachable on the shipped RC ruleset today, without ADR-0096.** `max_close_chunks` is
  **27** on `palw_rc_shipped_params()` and **1** on devnet, and the CLI's own
  `every_legal_close_can_be_carried_on_every_shipped_ruleset` builds the widest close each court
  ADMITS: on the RC it cuts into more than one chunk. The node's arithmetic closes already have a
  size at which it stops being able to file them.
* **The unit was misread once, so it is written down here.** A carrier holds
  `PALW_COURT_CLOSE_CHUNK_MAX_BYTES = PALW_OBJECT_CHUNK_MAX_BYTES = 100,000` SERIALIZED bytes.
  `palw_close_bytes_for_chunks_v1(1) = 83,333` is the same carrier expressed in the cost rule's
  COUNTED bytes, through W5's 10/12 framing allowance for the binding and borsh's own framing. The
  cost ceiling counts a proof payload; the carrier ceiling measures the serialized object, which
  also carries a `PalwStepBindingV2` the cost rule does not count. **A chunk count is decided by
  serializing the finished `CourtClosed`, never by predicting it from a counted-byte estimate.**
* **No pipeline-level test files a split close.** The state layer's coverage is thorough
  (`a_three_carrier_close_and_a_whole_one_reach_the_same_state`, the reorg, the pruning round trip,
  the lapse, the signature). `consensus/src/pipeline/virtual_processor/tests.rs` mentions neither
  `CourtCloseDeclared` nor `CourtCloseChunk`: the arm that adjudicates an assembled group has no
  test that reaches it through blocks.

## 2. The requirement

1. One cutter. The bytes a filer sends and the bytes the chain assembles are decided by one
   function, in the crate that owns the assembler, so a second answer cannot come to exist.
2. The node files what it builds. A close the node's court produces is carried whole when it fits
   and in parts when it does not, without the court arm knowing which.
3. A filer resumes from the chain, not from its own memory. Chunks arrive in any order and a
   carrier can be orphaned; the truth about which parts landed is the group's `present` bitmap.
4. The carriage costs what the ruleset says and no more: the mover's turn, the assembly window, the
   inflight carriers a node already bounds.
5. The path is proven through blocks, not only through the fold.

## 3. Decisions

**Decision 1 — the cut moves to consensus-core, and only the cut.** A new pure module
`consensus/core/src/palw_close_carriage.rs` owns `PalwCourtCloseCarriageV1` (session, side, parts,
serialized bytes, chunk count, close digest), `palw_plan_court_close_carriage_v1`,
`palw_court_close_parts_to_send_v1`, `palw_court_close_max_parts_v1` and
`palw_court_close_assembly_fits_v1`. It takes a `PalwConsensusObjectV2` and a `PalwCourtParamsV2`
and returns objects and indices. It holds no key, no wallet, no RPC client and no I/O — signing
stays with the caller, whose key it is, and whose crate has the signer.

**Decision 2 — the chain's answer arrives as a plain view, not as an RPC type.**
`PalwCourtCloseGroupSeenV1 { count, present, close_digest }` is what a resume compares against.
The CLI fills it from `GetPalwPendingChunkGroup`; the node fills it from
`PalwChainStateV2::court_close_group`, which it already holds — a node asking itself over RPC for
state it has in hand would be a second answer to the same question. consensus-core does not depend
on `kaspa-rpc-core` and this keeps it that way.

**Decision 3 — messages stay where their reader is.** The moved functions return a structured
`PalwCloseCarriageError`. `misaka-cli` keeps its own wrapper functions with today's signatures and
today's operator-facing text, built from the error's fields; the node logs its own. The CLI's
existing tests are the guard that this refactor changed no message, and they pass unedited.

**Decision 4 — the node's pending court move becomes a carriage, not an object.** `court_pending`
carries `PalwCourtCloseCarriageV1`-shaped parts for a close and a single object for every other
move. The submit loop sends the declaration first, then the chunks the chain does not already hold,
one carrier at a time, chained on the fee UTXO exactly as today and bounded by the same
`MAX_INFLIGHT_CARRIERS`. It is a state machine over what the CHAIN holds, re-read every tick, so a
restart, an orphan or a carrier the mempool dropped resumes rather than re-pays.

**Decision 5 — a plan is made once and re-checked every tick.** The parts are cut when the close is
built, because cutting is a function of the bytes and the bytes do not change. What is re-read is
the group: its count and digest must be this plan's, or the node stops filing into it rather than
completing somebody else's assembly — which under W7 convicts the declaring side.

**Decision 6 — the side is derived from the duty, never guessed.** `PalwCourtDutyV2::i_am_responder`
says which of the session's two bonds this node is: the responder is the executor and the
challenger is the other. The CLI refuses to default `--side` for the same reason a node must not
infer it loosely, and both now read the one mapping.

**Decision 7 — the split path is proven through the processor.** A test in
`virtual_processor/tests.rs` mines a declaration, then its chunks, in separate blocks under a court
whose `max_close_chunks` admits them, and asserts the completing block adjudicates the assembled
close to the verdict the declaration announced. The court used is built with
`with_cost_ceilings`, not a shipped preset: no fingerprint moves for a test.

**Decision 8 — nothing in consensus changes.** No new object, no new chunk kind, no
constrained-specific carriage. The transport ADR-0080 W5–W7 built is what carries every close this
project will add, which is the property that makes ADR-0096 §10 B13's worst case a filing question
rather than a consensus one.

## 4. What this costs

* A `k`-chunk close spends `k + 1` carriers and `k + 1` of the mover's blocks
  (`palw_court_move_cost_daa_v1` already takes `close_blocks` for this), and opens an assembly
  window of `PALW_COURT_CLOSE_INCLUSION_MARGIN × k` DAA that the session's backstop caps.
* A declaring side that does not finish loses the assembly deposit, which is why the node refuses
  to declare when the window will not hold the chunks — checked before the declaration is funded,
  with the same function the acceptance layer uses.
* Nothing changes for a close that fits one carrier: it is a `CourtClosed` on one transaction, the
  declaration is not paid for and no group is opened.

## 5. Invariants the tests must hold

1. The cut is the court's: every part's digest is `palw_court_close_chunk_digest_v1` of its bytes,
   the declaration pins them in index order, and the concatenation is the serialized close.
2. Whole and split reach the same state and the same verdict.
3. A resume sends exactly the parts the chain's bitmap lacks — a set, never a prefix.
4. A group whose count or digest is not this plan's is refused before a carrier is spent.
5. `max_close_chunks` binds: devnet's 1 refuses the split path by name; the RC's 27 admits it.
6. The CLI's messages and signatures are unchanged by the move.

## 6. Order of work

1. `palw_close_carriage.rs` in consensus-core, with the CLI delegating to it and its tests
   unedited. **Landed first, because everything else is a consumer of it.**
2. The node's submitter (Decisions 4–6).
3. The processor-level split-close test (Decision 7). **Reordered after measurement** — see §10.
4. *Not in this ADR:* the builder that assembles ADR-0096 §10 B5/B6 closes from a seat's own
   capture. The node's court builds arithmetic closes only; a constrained claim's decode dispute
   has an adjudicator and a carriage after this ADR, and still no assembler. That is ADR-0096's
   court half to finish, and it is named in its §10 B13.

## 7. Supersession

Supersedes no ADR. It completes ADR-0080 design A (which specified the transport and left the
filing side to "a node's own panel loop", measured here as the half that was never built) and it
is the carriage half ADR-0096 §10 B13 says a worst-case constrained close needs.

## 8. Number hygiene

**This ADR was written as 0102 and renumbered to 0104 on 2026-09-11, which is the rule working
rather than failing.** It was drafted against `feat/adr-0099-sharded-seat`'s README, which said the
next free number was 0102, and two other sessions were drafting at the same time: 0102 went to
`the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity` and 0103 to
`the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm`, both
resident before this one was pushed. Each ADR's own §"Number hygiene" says a concurrent claimant
renumbers the later writer, and this was the later writer, so it moved.

The renumber is the file, its title, and every `ADR-0102` in the tree that meant this document —
nineteen of them across `consensus-core`, `kaspad` and `misaka-cli`, none left behind. Checked
against every branch rather than against a README: 0104 was free on all of them, and 0105–0109
still are. **The next free number is 0105.**

## 9. Implementation record

**Stage 1 — the shared cutter (`90ec2317`).** `consensus/core/src/palw_close_carriage.rs`:
`PalwCourtCloseCarriageV1`, `palw_plan_court_close_carriage_v1`,
`palw_court_close_parts_to_send_v1` (and `palw_court_close_parts_owed_v1`, the plan-less form the
node resumes with), `palw_court_close_max_parts_v1`, `palw_court_close_assembly_fits_v1`,
`PalwCourtCloseGroupSeenV1`, `PalwCloseCarriageError`. Eight tests of its own, one per invariant in
§5. `misaka-cli/src/palw_court.rs` keeps every operator sentence and delegates the rule;
`CarriagePlan` is a type alias and `journal_key` an extension trait, so its **sixteen tests passed
unedited** — the only test-side change is one import line for the three names the cut took with it.
`misaka-cli/src/main.rs`'s claim that the split path was "planned but not yet filable" is corrected;
W6 and W7 have landed.

**Stage 2 — the node's submitter (`d22f718a`).** `court_pending` carries `CourtMoveV1 { parts,
group }`; `plan_court_move_v1` cuts a close, reads the side off `PalwCourtDutyV2::i_am_responder`,
checks the window against `session_deadline_daa` and signs the declaration. The submit loop resumes
from `palw_court_close_group_v1`'s bitmap each pass, refuses a group that is not this plan's, and
files what is missing one carrier at a time under the existing `MAX_INFLIGHT_CARRIERS` and funding
chain.

**Two defects found by writing it, fixed in the same commit.** Neither was in the design:

* **An armed-rent network prices a declaration by the close it buys.**
  `palw_court_close_min_fee_v1(count)` is the relay fee for the whole close's counted bytes, while
  the declaration object is small — its digests are 64 bytes a chunk. `build_lifecycle_tx` paid the
  carrier's own relay minimum, so the declaration would have been dropped, no group would have
  opened, and the resume — reading no group — would have re-filed the whole carriage every pass.
  The builder now floors the fee with what the chain charges. **This entry claimed, when it was
  first written, that the certification lane "has the identical gap"; it does not, and §11 records
  what the follow-up measured.**
* **A carriage kept across ticks re-files what is still in flight.** A split close is the first
  move this loop holds AFTER sending; the bitmap only moves when a carrier reaches a block, and the
  panel ticks every couple of seconds. Rate-limited by `COURT_MOVE_REPLAN_DAA`, the window the
  planner already uses, and abandoned past the session's backstop, where the chain accepts no
  further chunk.

**Stage 3 — the cutter run against the assembler (`8afb6fdd`).**
`the_planner_the_filers_use_cuts_a_close_this_arm_assembles`, in `palw_state_v2`'s own test module
beside the fixtures. The existing whole-versus-split test cuts with `split_close_v1`, a test's
cutter sized so three carriers can be exercised cheaply — the right trade for testing the ARM, and
it left the shipped cutter never run against the assembler at all, which is the exact shape this
ADR removes. This one builds a close that genuinely outgrows a carrier (the padding is operand
opening bytes, the part that grows with real evidence), cuts it with
`palw_plan_court_close_carriage_v1`, files what that returns byte for byte, and asserts the group
pins the planner's digest and the assembly reaches the same state root as the same close on one
carrier at the same DAA. It also pins that the planner returns the declaration UNSIGNED.

## 10. What Decision 7 costs, measured

Decision 7 called for a processor-level test and put it second. Measured before writing it, it is
the most expensive item in this ADR and it was reordered behind the submitter. The carriage half is
solved: `palw_v2_a_funded_carrier_is_priced_by_the_fee_the_utxo_walk_read`
(`virtual_processor/tests.rs:10637`) is a working template for funding a `0x4b` transaction from a
two-wide row's coinbase, signing it ML-DSA-87 and mining it — **and it is the only test in the file
that mines a lifecycle object at all**; every other PALW test calls `palw_v2_validate_objects` or
the fold directly.

The court half does not exist. **No test in `kaspa-consensus` has ever created a
`PalwCourtSessionStateV2`**, through blocks or otherwise. `split_close_fixture` gets its session
for free from helpers that are `#[cfg(test)] pub(crate)` inside consensus-core — `apply`, `ctx`,
`h64`, `bond_key`, `court_open`, `disclose`, `rung_verdict`, and above all
`palw_step_refute::tests::skeleton_refutation` — none of which crosses the crate boundary, and
`consensus/Cargo.toml` exposes no test-only feature that would let them. So the test must build
through mined blocks: a licensed claim, a signed `CourtOpened` from a SECOND registered bond, then
four signed disclosure/verdict rungs to collapse `[0,16)` to Terminal — five or six blocks of setup
before the first chunk.

Two facts make it tractable when it is written. `PALW_COURT_CLOSE_MAX_PER_BLOCK` is 1, but only the
COMPLETING chunk spends that slot (`palw_court_close_completes_a_group_v1` matches
`CourtCloseChunk` alone), so the declaration and the earlier chunks may share blocks. And the
harness bundle uses `PALW_RC_WINDOWS_V1` despite its devnet name, so `max_close_chunks` is 27 there
and the split path is admitted rather than refused.

The deepest gap is the proof. The processor DROPS a completing chunk whose assembly fails
`adjudicate_court_close_v2`, so without a structurally real `PalwCourtVerdictProofV2` the group
never completes and the state never moves. **The first version should therefore assert the
CONVICTION path** — bytes that assemble but do not decode to this session's close are admitted at
acceptance by design and convict the declarer in the block carrying the last chunk — and leave
"adjudicates to the declared verdict" to a follow-up that either publishes a skeleton-proof builder
or borrows the real fixture in `misaka-palw-base0/tests/constrained_court_e2e.rs`.

Until that lands, what stands behind the split path is the state layer's own coverage
(`a_three_carrier_close_and_a_whole_one_reach_the_same_state`, the reorg, the pruning round trip,
the lapse, the signature) plus this ADR's eight. **No split close has been filed on a live chain**,
and devnet's `max_close_chunks = 1` refuses one, so a drill that exercises it needs a network whose
court pays for more than one carrier.

## 11. The certification gap this ADR named, measured — and the one that was really there

§9 recorded, as a known-and-deferred defect, that "the certification lane has the identical gap":
that the panel had never paid `palw_certification_min_fee_v1` for a `FamilyCertified`. **That was
wrong, and it was wrong in the way a plausible sentence is wrong — by asserting a call site nobody
had looked for.** The panel does not file a `FamilyCertified`. It does not file an `ObjectChunk`
either. Every construction of either object in the tree is inside `consensus/`: the production
arms, the rent functions and their tests. The objects the panel builds are `CourtOpened`,
`CourtDisclosed`, `CourtVerdictPosted`, `CourtClosed` (now with its declaration and chunks),
`MaterialDisclosed`, `ClassRegistered`, `BondRegistered` and an assembled `ReceiptLicensed` — and
of those, only the close declaration is priced by any rule.

Certifications are filed by `misaka palw submit-object`, from a file `palw-certify` writes, and
**that path already paid every rent**: `misaka-cli`'s `carrier_rent_v1` had worked out all three
charges, including the one this ADR's author did not know about — that the chunk which COMPLETES a
certification group owes the grading rent, and cannot know the vector count, so it must pay for
`PALW_CERTIFICATION_MAX_VECTORS`.

**The acceptance filter holds exactly three refusals**, all behind `palw_certification_rent`:
the chunk SLOT charged to the opener (`palw_object_chunk_group_rent_v1`), the close's ADJUDICATION
charged to the declaration (`palw_court_close_min_fee_v1`), and the GRADING charged to a
`FamilyCertified` or to the chunk completing one (`palw_certification_min_fee_v1`). Separately and
**not** behind that fence, `palw_object_rent_ceiling_v1` decides how much of a carrier's fee is
BURNED rather than paid to the miner — which is where ADR-0088 Decision 11's three model-registry
objects are priced. Burning is not dropping: an underpaying model object is not refused, it simply
burns its whole fee.

**So the real defect was not a missing payment but a second answer.** After §9, two filers priced
the same carrier by different rules: `misaka-cli` paid unconditionally, and the panel paid only
when the fence read armed at the DAA it BUILT at. A carrier is judged at the DAA it LANDS at, so a
fence arming in between drops it — and a dropped declaration is not a retry but a lost dispute and
a forfeited assembly deposit. Measured, the premium for paying early is 28,125,000 sompi, under a
third of one MSK, on the widest close a shipped ruleset admits; and it is burned rather than paid
to anyone, so overpaying enriches no adversary. The fence check was removed, the rule was promoted
to `palw_carrier_min_fee_v1` in `palw_state_v2` beside the ceiling it must not be confused with,
and both filers are one-line delegates. The panel's builder reads it for every carrier rather than
at the call sites, so no future lane can forget it.

`a_filers_floor_covers_every_rule_that_would_drop_its_carrier` pins each of the three against the
object its rule reads. The panel's own builder needs a key, a wallet and a funded outpoint, so the
floor is pinned where it is a pure function — which covers both filers, because both delegate. The
test found an off-by-one in its own author's assumption on its first run: in a 255-part group the
completing chunk is index **254**, and index 255 completes nothing.
